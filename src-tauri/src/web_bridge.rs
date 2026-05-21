#![allow(dead_code)]
//! Web-player bridge — fills in track metadata for browser-based players
//! that don't expose `navigator.mediaSession.metadata` correctly to Windows
//! SMTC. Pandora.com is the motivating case: SMTC gets the browser tab
//! title ("Today's Hits Radio - Now Playing on Pandora") and the Chrome
//! favicon as thumbnail. The real song info lives only in Chrome's DOM.
//!
//! This module owns:
//! - The `WebPlayerProbe` trait — a small interface every supported
//!   no-Media-Session web player implements.
//! - The `PandoraProbe` impl (first concrete probe).
//! - A polling loop that activates only when a probe's `detects()` matches
//!   the current SMTC snapshot. When no probe matches, zero UIA calls
//!   fire — YouTube / Spotify / iTunes are never touched.
//! - A shared cache (`SharedWebBridge`) the lyrics resolver consults
//!   before falling back to the SMTC snapshot.
//!
//! The cache value is a `WebBridgeTrack` with a `last_seen_unix_ms`
//! timestamp. Resolver treats values older than ~5s as stale.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

use crate::smtc::SharedSnapshot;

#[derive(Clone, Debug, Serialize, Default)]
pub struct WebBridgeTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Identifier of the probe that wrote this entry, e.g. `"pandora-web"`.
    pub source: String,
    /// Unix epoch ms at the moment of the read. Consumers use this to
    /// decide staleness — typically anything older than 5_000ms is treated
    /// as not-present.
    pub last_seen_unix_ms: i64,
}

pub type SharedWebBridge = Arc<RwLock<Option<WebBridgeTrack>>>;

/// A probe for one specific web player that doesn't expose Media Session
/// metadata correctly. Probes are stateless — every method receives all
/// the inputs it needs.
pub trait WebPlayerProbe: Send + Sync {
    /// Short, stable identifier — used as the `source` field of the
    /// produced `WebBridgeTrack` and in logging.
    fn name(&self) -> &'static str;

    /// Fast gate: does the current SMTC snapshot look like our player?
    /// Must be cheap (string ops only) — runs on every snapshot tick.
    fn detects(&self, smtc_title: &str, smtc_app_id: &str) -> bool;

    /// Walk Chrome's UI Automation tree, extract the now-playing widget
    /// content. Returns `Ok(Some(...))` when a complete-enough read
    /// succeeds, `Ok(None)` when the probe ran but couldn't find the
    /// widget, `Err` for unexpected failures.
    fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>>;
}

/// Quick check: does ANY registered probe think the current SMTC snapshot
/// is unreliable? The lyrics resolver uses this to decide whether to
/// surface `Status::Unsupported` when the bridge cache is empty/stale.
pub fn any_probe_detects(smtc_title: &str, smtc_app_id: &str) -> bool {
    PROBES.iter().any(|p| p.detects(smtc_title, smtc_app_id))
}

/// Concrete probe registry. Build-time static — new probes ship as new
/// entries in this slice.
static PROBES: &[&dyn WebPlayerProbe] = &[&PandoraProbe];

/// Enumerate top-level Chrome windows whose title matches `predicate`.
/// Returns the `HWND` of each match. Used by probes to find the right
/// Chromium window when multiple tabs / multiple Chrome windows are open.
///
/// Multi-process Chrome: UIA queries against the top-level window handle
/// reach into whichever renderer process is hosting that window's content,
/// so we don't need to chase the per-tab child processes ourselves.
fn find_chrome_windows<F: Fn(&str) -> bool>(predicate: F) -> Vec<HWND> {
    struct Ctx<'a> {
        predicate: &'a dyn Fn(&str) -> bool,
        hits: Vec<HWND>,
    }

    let mut ctx = Ctx {
        predicate: &predicate,
        hits: Vec::new(),
    };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam was set to a valid &mut Ctx by the EnumWindows
        // caller. The reference outlives the synchronous EnumWindows call.
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };

        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1); // skip hidden, keep enumerating
        }

        let title = read_window_title(hwnd);
        if title.is_empty() || !(ctx.predicate)(&title) {
            return BOOL(1);
        }

        let process_name = read_process_name_for_window(hwnd);
        if process_name.eq_ignore_ascii_case("chrome.exe") {
            ctx.hits.push(hwnd);
        }
        BOOL(1)
    }

    let ctx_ptr: *mut Ctx = &mut ctx;
    let _ = unsafe { EnumWindows(Some(enum_proc), LPARAM(ctx_ptr as isize)) };

    ctx.hits
}

fn read_window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // GetWindowTextW returns the number of characters copied, NOT
    // including the null terminator. A return of 0 means either an
    // empty title or an error — either way we treat as empty.
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    if n <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..n as usize])
}

fn read_process_name_for_window(hwnd: HWND) -> String {
    let mut pid: u32 = 0;
    let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return String::new();
    }
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(_) => return String::new(),
    };
    let mut name_buf = [0u16; 260];
    let n = unsafe { GetModuleBaseNameW(handle, None, &mut name_buf) };
    // Close the handle explicitly to avoid leaking on every window we enumerate.
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(handle) };
    if n == 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&name_buf[..n as usize])
    }
}

// PandoraProbe lives in this same module — see Task 4.
struct PandoraProbe;

impl WebPlayerProbe for PandoraProbe {
    fn name(&self) -> &'static str {
        "pandora-web"
    }

    fn detects(&self, smtc_title: &str, smtc_app_id: &str) -> bool {
        // Chromium-derived browsers (Chrome, Edge, Brave, Opera) all
        // expose UIA trees identically. Match any AUMID that mentions
        // Chrome — covers the common case and a few derivatives.
        // Reject empty app_id outright (idle session / no source).
        if smtc_app_id.is_empty() || !smtc_app_id.contains("Chrome") {
            return false;
        }
        // Pandora's <title> element is always "{station name} - Now
        // Playing on Pandora". Match the suffix exactly — substring
        // matches would false-positive on song titles containing the
        // word "Pandora" (Aerosmith's "Pandora", Greek mythology, etc.).
        smtc_title.ends_with("Now Playing on Pandora")
    }

    fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>> {
        // Filled in in Task 5.
        Ok(None)
    }
}

/// Spawn the bridge worker. The worker watches the SMTC snapshot and,
/// when a probe matches, polls UIA every 2s. Idle (5s tick, zero UIA
/// calls) when no probe matches.
pub fn start(_app: AppHandle, _snapshot: SharedSnapshot, _shared: SharedWebBridge) {
    tauri::async_runtime::spawn(async {
        // Smoke test — log all Chrome windows whose title ends with
        // "Now Playing on Pandora". Removed in Task 6.
        eprintln!("[web_bridge] startup smoke test running");
        let hwnds = find_chrome_windows(|t| t.ends_with("Now Playing on Pandora"));
        eprintln!("[web_bridge] found {} Pandora-titled Chrome windows", hwnds.len());
        for hwnd in &hwnds {
            eprintln!("[web_bridge]   HWND = {hwnd:?}");
        }
    });
}

fn _silence_unused_app_emitter(app: &AppHandle) {
    // Keeps the import live until Task 6 wires the emitter. Will be removed.
    // AppHandle implements Emitter; we reference the trait bound here so the
    // import survives until Task 6 uses it for real.
    fn _assert_emitter<T: Emitter<tauri::Wry>>(_: &T) {}
    _assert_emitter(app);
}

fn _silence_unused_duration() {
    let _ = Duration::from_secs(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `app_id.contains("Chrome")` matches the real Chrome AUMID
    /// (`"Chrome.exe"` on most installs, `"MSEdge.exe"`-based hybrids on
    /// custom builds — we accept any Chromium-derived app since the UIA
    /// tree shape is identical). `app_id` is empty when SMTC didn't
    /// report a source — be tolerant.
    #[test]
    fn pandora_detects_real_chrome_pandora_session() {
        let p = PandoraProbe;
        assert!(p.detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "Chrome.exe",
        ));
        assert!(p.detects(
            "Some Other Station - Now Playing on Pandora",
            "Google.Chrome",
        ));
    }

    #[test]
    fn pandora_rejects_non_chrome_apps() {
        let p = PandoraProbe;
        // Even if a desktop Pandora app set the title to match, we
        // don't activate the probe for non-Chrome sources — they
        // expose SMTC correctly and don't need DOM scraping.
        assert!(!p.detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "Spotify.exe",
        ));
        assert!(!p.detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "",
        ));
    }

    #[test]
    fn pandora_rejects_non_pandora_titles_in_chrome() {
        let p = PandoraProbe;
        // YouTube in Chrome — must NOT match.
        assert!(!p.detects(
            "Rick Astley - Never Gonna Give You Up (Official Music Video)",
            "Chrome.exe",
        ));
        // Spotify Web in Chrome — must NOT match.
        assert!(!p.detects(
            "Bohemian Rhapsody · Queen - Spotify",
            "Chrome.exe",
        ));
        // Empty title in Chrome — must NOT match (idle browser tab).
        assert!(!p.detects("", "Chrome.exe"));
    }

    #[test]
    fn pandora_does_not_false_positive_on_word_pandora_elsewhere() {
        let p = PandoraProbe;
        // A YouTube video about Pandora's Box mythology, or a Spotify
        // album called Pandora. Title doesn't END with the canonical
        // Pandora-tab suffix — must NOT match.
        assert!(!p.detects(
            "Pandora's Box - Greek Mythology Explained",
            "Chrome.exe",
        ));
        assert!(!p.detects(
            "Pandora · Aerosmith - Spotify",
            "Chrome.exe",
        ));
    }

    #[test]
    fn any_probe_detects_aggregates_correctly() {
        assert!(any_probe_detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "Chrome.exe",
        ));
        assert!(!any_probe_detects(
            "Rick Astley - Never Gonna Give You Up",
            "Chrome.exe",
        ));
    }
}
