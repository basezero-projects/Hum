#![allow(dead_code)] // Removed in Task 8 when lyrics.rs starts consulting the bridge.
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

/// Recognized Chromium-derived browser process names. UIA tree structure
/// is identical across these, so any of them hosting a Pandora tab is a
/// valid target for the probe. Match is case-insensitive. Keep this
/// aligned with the `app_id`-side check inside `PandoraProbe::detects` —
/// the two gates need to agree on what counts as "a Chromium browser."
const CHROMIUM_PROCESS_NAMES: &[&str] = &[
    "chrome.exe",
    "msedge.exe",
    "brave.exe",
    "opera.exe",
    "vivaldi.exe",
];

fn is_chromium_process(name: &str) -> bool {
    CHROMIUM_PROCESS_NAMES
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n))
}

/// Enumerate top-level Chromium-browser windows whose title matches `predicate`.
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
        if is_chromium_process(&process_name) {
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
        // Chromium-derived browsers (Chrome, Edge, Brave, Opera,
        // Vivaldi) all expose UIA trees identically. SMTC's AUMID for
        // these usually contains "Chrome" (Chrome.exe itself, Chromium
        // forks that report the upstream identifier, etc.) — broad
        // substring match catches the common cases. The window-side
        // gate (`find_chrome_windows` + `is_chromium_process`) is the
        // narrower check that actually filters to known Chromium
        // process names; both gates need to keep agreeing on what
        // "Chromium" means.
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
pub fn start(app: AppHandle, snapshot: SharedSnapshot, shared: SharedWebBridge) {
    tauri::async_runtime::spawn(async move {
        eprintln!("[web_bridge] worker starting");
        let mut last_emitted_title = String::new();

        loop {
            let (title, app_id) = {
                let snap = snapshot.read().await;
                let id = snap.source_app_id.clone().unwrap_or_default();
                (snap.title.clone(), id)
            };

            let active_probe: Option<&'static dyn WebPlayerProbe> = PROBES
                .iter()
                .find(|p| p.detects(&title, &app_id))
                .copied();

            match active_probe {
                Some(probe) => {
                    let name = probe.name();
                    let read_result = tokio::task::spawn_blocking(move || probe.read())
                        .await;
                    match read_result {
                        Ok(Ok(Some(track))) => {
                            let new_title = track.title.clone();
                            {
                                let mut w = shared.write().await;
                                *w = Some(track);
                            }
                            if new_title != last_emitted_title {
                                eprintln!(
                                    "[web_bridge] probe={name} read title={new_title:?}, emitting web-bridge-updated"
                                );
                                last_emitted_title = new_title;
                                // Dedicated event so SMTC's `track-changed`
                                // semantics (payload = full CurrentTrack)
                                // stay clean. lyrics::start subscribes to
                                // both events; web-bridge-updated is just
                                // a wake signal with `()` payload.
                                let _ = app.emit("web-bridge-updated", ());
                            }
                        }
                        Ok(Ok(None)) => {
                            // Probe ran, found nothing — leave existing cache
                            // alone. Resolver staleness check (5s) handles
                            // expiration if subsequent reads also fail.
                        }
                        Ok(Err(e)) => {
                            eprintln!("[web_bridge] probe={name} read error: {e:#}");
                        }
                        Err(join_err) => {
                            eprintln!("[web_bridge] probe={name} spawn_blocking failed: {join_err:#}");
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                None => {
                    // No probe matches the current SMTC snapshot. Idle —
                    // zero UIA calls fire. Wake periodically to re-check.
                    if !last_emitted_title.is_empty() {
                        // Just transitioned out of an active probe; clear the
                        // stale cache so the resolver doesn't keep using
                        // last-known Pandora data after the user switched
                        // tabs to YouTube.
                        *shared.write().await = None;
                        last_emitted_title.clear();
                        eprintln!("[web_bridge] no probe matches, cache cleared");
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    });
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
