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

use std::sync::{Arc, OnceLock};

use regex::Regex;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

use crate::smtc::SharedSnapshot;

/// Unix epoch milliseconds at the current moment. Inline helper used by both
/// the web and desktop probes — keeps the call sites readable.
fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

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
    /// Playback position when the probe last read. `None` when the probe
    /// cannot determine position (e.g. the Chrome-side `PandoraProbe`
    /// trusts SMTC's position because Chrome itself publishes timeline
    /// data to SMTC). When `Some`, the lyrics resolver and
    /// `get_current_track` blend this in over SMTC's stale position.
    /// Interpretation matches `CurrentTrack.position_ms`: ms elapsed from
    /// the track start, paired with `last_seen_unix_ms` for client-side
    /// interpolation.
    #[serde(default)]
    pub position_ms: Option<u64>,
    /// Playback state — `Playing` or `Paused`. `None` when the probe can't
    /// determine state (Chrome `PandoraProbe` relies on SMTC for this).
    /// When `Some`, the frontend's wall-clock interpolation only advances
    /// position while `Playing`, so a paused Pandora desktop session
    /// freezes the lyrics where they are instead of scrolling forward.
    #[serde(default)]
    pub state: Option<crate::smtc::PlaybackState>,
    /// Set by probes that can detect an ad break (Pandora desktop /
    /// Pandora web). When true, `blend_bridge_into_snapshot` flips
    /// `snap.ad_active = true`. `position_ms` / `duration_ms` still
    /// reflect ad timing when present.
    #[serde(default)]
    pub is_ad: bool,
    /// Ad (or probe-supplied) track duration in ms. When `Some`, overrides
    /// the SMTC-reported `snap.duration_ms`. Used by Pandora desktop ads
    /// to surface the countdown-derived duration for the progress bar.
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

pub type SharedWebBridge = Arc<RwLock<Option<WebBridgeTrack>>>;

/// Freshness window for bridge data, in ms. Bridge entries older than this
/// are no longer used to override snapshot fields in `blend_bridge_into_snapshot`.
pub const BRIDGE_FRESHNESS_MS: i64 = 5_000;

/// Authority window for bridge data, in ms. A position-supplying bridge
/// entry within this window keeps SMTC's parallel emits *suppressed*, so
/// the frontend doesn't flicker back to whatever app SMTC happens to be
/// publishing during a single-poll miss (ad breaks transiently collapse
/// Pandora's now-playing Hyperlinks). When the bridge worker observes
/// its probe is truly gone (Pandora.exe closed), it clears the cache to
/// `None`, which trips `bridge_is_authoritative` immediately regardless
/// of this window.
pub const BRIDGE_AUTHORITY_MS: i64 = 60_000;

/// Mutate `snap` in place so its `title`/`artist`/`album` reflect a fresh
/// bridge entry (if one exists), and — when the bridge probe supplied
/// `position_ms` — its `position_ms`/`last_update_unix_ms`/`state` reflect
/// bridge timing too. The lyrics resolver does the title/artist/album
/// override already, but the snapshot itself (read by the
/// `get_current_track` command and emitted in `timeline-changed` events)
/// needs the same treatment so the *frontend* sees bridge-accurate
/// playback position for non-SMTC apps like Pandora desktop.
pub async fn blend_bridge_into_snapshot(
    snap: &mut crate::smtc::CurrentTrack,
    bridge: &SharedWebBridge,
) {
    let bridge_track = { bridge.read().await.clone() };
    let Some(bt) = bridge_track else { return };

    let now_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    if now_unix_ms - bt.last_seen_unix_ms >= BRIDGE_FRESHNESS_MS {
        return;
    }
    // Ad-break path: empty title is expected (no track to display).
    // Set the ad flag and timing fields, then return — don't overwrite
    // title/artist/album with empty strings (the snapshot retains whatever
    // the last real track was, which is fine since the overlay shows the
    // promo card, not metadata, during ads).
    if bt.is_ad {
        snap.ad_active = true;
        if let Some(pos) = bt.position_ms {
            snap.position_ms = pos;
            snap.last_update_unix_ms = bt.last_seen_unix_ms;
        }
        if let Some(dur) = bt.duration_ms {
            snap.duration_ms = dur;
        }
        if let Some(s) = bt.state {
            snap.state = s;
        }
        return;
    }

    if bt.title.trim().is_empty() {
        return;
    }

    // Explicit clear: the bridge has a real (non-ad) track, so the
    // previously-set ad flag must drop. Without this, transitioning from
    // a Pandora ad to the next song would leave `ad_active = true` on the
    // snapshot — the overlay would keep showing the promo card and the
    // AD BREAK chip even though the bridge already reported the new song.
    snap.ad_active = false;

    snap.title = bt.title;
    snap.artist = bt.artist;
    snap.album = bt.album;
    if let Some(pos) = bt.position_ms {
        snap.position_ms = pos;
        snap.last_update_unix_ms = bt.last_seen_unix_ms;
        // The probe's reported state drives the snapshot's state. Frontend
        // interpolation only advances while Playing; when the probe reports
        // Paused, lyrics freeze where they are instead of scrolling forward.
        if let Some(s) = bt.state {
            snap.state = s;
        }
    }
    if let Some(dur) = bt.duration_ms {
        snap.duration_ms = dur;
    }
}

/// Returns `true` when a position-supplying bridge probe is currently
/// authoritative over the snapshot — meaning SMTC's parallel emits should
/// be suppressed so they don't flicker the frontend back to whatever app
/// SMTC happens to be publishing.
///
/// "Authoritative" = the cache has an entry whose `position_ms` is `Some`
/// (the probe is actively supplying position) AND whose `last_seen_unix_ms`
/// is within `BRIDGE_AUTHORITY_MS`. Uses a much longer window than
/// `blend_bridge_into_snapshot` so a single missed poll doesn't drop
/// authority mid-song.
pub async fn bridge_is_authoritative(bridge: &SharedWebBridge) -> bool {
    let bridge_track = { bridge.read().await.clone() };
    let Some(bt) = bridge_track else { return false };
    if bt.position_ms.is_none() {
        // Bridge entry exists but the probe didn't supply position (e.g.
        // the Chrome-side PandoraProbe relies on SMTC for position).
        // SMTC's own emits stay the authoritative source in that case.
        return false;
    }
    // A paused bridge probe yields authority so another SMTC source can
    // take over. Without this, pausing Pandora.exe and starting Spotify
    // would leave Hum stuck on the (paused) Pandora track because SMTC's
    // Spotify emits would still be suppressed.
    if matches!(bt.state, Some(crate::smtc::PlaybackState::Paused)) {
        return false;
    }
    let now_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    (now_unix_ms - bt.last_seen_unix_ms) < BRIDGE_AUTHORITY_MS
}

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
/// entries in this slice. Order matters: the poll loop picks the first
/// probe whose `detects()` returns true, so SMTC-gated probes (the cheap
/// path) come before process-enumeration probes.
static PROBES: &[&dyn WebPlayerProbe] = &[
    &PandoraProbe,
    &crate::pandora_desktop::PandoraDesktopProbe,
    &crate::youtube_bridge::YouTubeProbe,
];

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
///
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

/// Find the first visible Chromium window whose title contains `needle`
/// (case-sensitive substring match). Used by YouTubeProbe to locate the
/// YouTube tab's window handle without a full tab enumeration.
///
/// Returns `None` when no matching Chromium window is visible. The caller
/// must check whether the returned window is actually a YouTube tab — the
/// title match is a fast heuristic; the UIA walk in the probe confirms.
pub(crate) fn find_chromium_window_with_title_substring(needle: &str) -> Option<HWND> {
    find_chrome_windows(|title| title.contains(needle)).into_iter().next()
}

pub(crate) fn read_window_title(hwnd: HWND) -> String {
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

pub(crate) fn read_process_name_for_window(hwnd: HWND) -> String {
    let mut pid: u32 = 0;
    let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return String::new();
    }
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(h) => h,
        Err(_) => return String::new(),
    };
    // QueryFullProcessImageNameW works with PROCESS_QUERY_LIMITED_INFORMATION,
    // whereas GetModuleBaseNameW requires PROCESS_VM_READ (which we don't
    // have for most processes on a non-elevated session). Returns the full
    // path; we extract the file name basename below.
    let mut path_buf = [0u16; 1024];
    let mut size: u32 = path_buf.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(path_buf.as_mut_ptr()),
            &mut size,
        )
    };
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(handle) };
    if result.is_err() || size == 0 {
        return String::new();
    }
    let full_path = String::from_utf16_lossy(&path_buf[..size as usize]);
    // Extract basename — last path component after \ or /.
    full_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_string()
}

// ─── Pandora UIA selector reference ────────────────────────────────────────
//
// Discovered via inspect.exe against pandora.com Chrome window on 2026-05-21.
// Pandora's CSS Modules use a `{Module}__{slot}__{name}` convention with
// inconsistent leading-letter case across slots (`nowPlayingTopInfo` vs
// `NowPlayingTopInfo`). The matcher below is case-insensitive on the
// distinguishing `__current__<role>Name` suffix to absorb that, and uses
// substring match because Pandora's ClassName is a space-separated multi-
// class list where the first token is role-specific and subsequent tokens
// (`nowPlayingTopInfo__current__link` shared across all three roles, plus
// ImageLoader__shadow / __intrinsic for art elements) are noise we ignore.
//
// Track title:
//   LocalizedControlType = "link"
//   ClassName substring  = "__current__trackName"
//   Name                 = song title (sometimes duplicated visible+aria-label)
//
// Artist:
//   LocalizedControlType = "link"
//   ClassName substring  = "__current__artistName"
//   Name                 = artist name(s), e.g. "Kane Brown, Swae Lee & Khalid"
//
// Album:
//   LocalizedControlType = "link"
//   ClassName substring  = "__current__albumName"
//   Name                 = album, e.g. "Different Man"
//
// Selector strategy: depth-first walk of the Chrome window's root
// IUIAutomationElement subtree via the control-view tree walker, filter
// by lowercased ClassName containing the role's substring, return the
// matching node's Name property. Stable across Pandora's React rebuilds
// because the substring is derived from CSS Module source slot names.

/// Combined single-pass DFS over the Chrome UIA tree for a Pandora.com tab.
///
/// Collects in one walk:
/// - `title_raw`  — `Name` of the first element whose class contains
///   `__current__trackName`
/// - `artist_raw` — class `__current__artistName`
/// - `album_raw`  — class `__current__albumName`
/// - `urls`       — all Hyperlink `Value` URLs (for ad/track classification)
/// - `countdown`  — first text node matching `M:SS` (ad countdown timer)
///
/// Replaces the previous three separate `find_text_by_class_substr` calls
/// with a single O(n) walk — three DFS passes was flagged as a performance
/// hazard in BUGS.md.
struct PandoraWebData {
    title_raw: String,
    artist_raw: String,
    album_raw: String,
    urls: Vec<String>,
    countdown: Option<String>,
}

fn collect_pandora_web_data(
    automation: &uiautomation::UIAutomation,
    root: &uiautomation::UIElement,
) -> PandoraWebData {
    use uiautomation::patterns::UIValuePattern;

    static COUNTDOWN_RE: OnceLock<Regex> = OnceLock::new();
    let countdown_re = COUNTDOWN_RE.get_or_init(|| {
        Regex::new(r"^\d+:\d{2}$").expect("countdown regex is valid")
    });

    const MAX_NODES: usize = 10_000;
    let walker = match automation.get_control_view_walker() {
        Ok(w) => w,
        Err(_) => {
            return PandoraWebData {
                title_raw: String::new(),
                artist_raw: String::new(),
                album_raw: String::new(),
                urls: Vec::new(),
                countdown: None,
            }
        }
    };

    let mut title_raw = String::new();
    let mut artist_raw = String::new();
    let mut album_raw = String::new();
    let mut urls: Vec<String> = Vec::new();
    let mut countdown: Option<String> = None;

    let mut stack: Vec<uiautomation::UIElement> = vec![root.clone()];
    let mut visited = 0_usize;

    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > MAX_NODES {
            eprintln!(
                "[web_bridge] PandoraProbe: collect_pandora_web_data hit MAX_NODES={MAX_NODES}"
            );
            break;
        }

        // Class-based text reads (track name / artist / album).
        if let Ok(class) = node.get_classname() {
            let class_lower = class.to_lowercase();
            if class_lower.contains("__current__trackname") && title_raw.is_empty() {
                if let Ok(name) = node.get_name() {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        title_raw = trimmed.to_string();
                    }
                }
            } else if class_lower.contains("__current__artistname") && artist_raw.is_empty() {
                if let Ok(name) = node.get_name() {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        artist_raw = trimmed.to_string();
                    }
                }
            } else if class_lower.contains("__current__albumname") && album_raw.is_empty() {
                if let Ok(name) = node.get_name() {
                    let trimmed = name.trim();
                    if !trimmed.is_empty() {
                        album_raw = trimmed.to_string();
                    }
                }
            }
        }

        // URL collection via ValuePattern (Hyperlinks used for ad detection).
        // Shared with the desktop probe — same Pandora URL semantics.
        if let Ok(vp) = node.get_pattern::<UIValuePattern>() {
            if let Ok(value) = vp.get_value() {
                if crate::pandora_desktop::classify_pandora_url(&value).is_some() {
                    urls.push(value);
                }
            }
        }

        // Countdown text (ad timer, e.g. "0:23").
        if countdown.is_none() {
            if let Ok(name) = node.get_name() {
                if countdown_re.is_match(name.trim()) {
                    countdown = Some(name.trim().to_string());
                }
            }
        }

        // Enqueue children in reverse for left-to-right DFS.
        let mut children: Vec<uiautomation::UIElement> = Vec::new();
        if let Ok(first) = walker.get_first_child(&node) {
            let mut cur = Some(first);
            while let Some(c) = cur {
                children.push(c.clone());
                cur = walker.get_next_sibling(&c).ok();
            }
        }
        for c in children.into_iter().rev() {
            stack.push(c);
        }
    }

    PandoraWebData { title_raw, artist_raw, album_raw, urls, countdown }
}

/// Pandora's track-title `Name` property sometimes contains the visible
/// text concatenated with the aria-label, producing strings like
/// `"Song Title Song Title"`. When the string is `"{half} {half}"` —
/// i.e. two copies separated by exactly one space — collapse to a single
/// copy. Defensive: only dedupes when the two halves match exactly, so
/// legitimate titles that happen to contain repeated words are preserved.
fn dedupe_doubled(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // "ABC ABC" has the form left + " " + right where left == right.
    // The space falls at index N in a string of length 2N+1.
    let len = trimmed.len();
    if len >= 3 && !len.is_multiple_of(2) {
        // Odd total length — the midpoint space is at (len - 1) / 2.
        let mid = (len - 1) / 2;
        if trimmed.as_bytes().get(mid) == Some(&b' ') {
            let left = &trimmed[..mid];
            let right = &trimmed[mid + 1..];
            if !left.is_empty() && left == right {
                return left.to_string();
            }
        }
    }
    trimmed.to_string()
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
        use uiautomation::UIAutomation;

        let automation = UIAutomation::new()
            .map_err(|e| anyhow::anyhow!("UIAutomation::new failed: {e:?}"))?;

        // Chrome only exposes the ACTIVE tab's title via the OS window
        // text (GetWindowTextW). If Pandora is in a background tab, the
        // window's title is whatever tab the user is currently looking at
        // — Pandora is invisible to the title-filter approach. Enumerate
        // ALL Chromium windows and let the UIA read step decide which one
        // contains Pandora content. NOTE: Chrome's accessibility tree only
        // includes the ACTIVE tab's DOM — backgrounded tabs are absent.
        // So this still requires Pandora to be the active tab in some
        // Chromium window. Documented limitation; the only known workaround
        // is a browser extension, which Wes explicitly ruled out.
        let hwnds = find_chrome_windows(|_| true);
        if hwnds.is_empty() {
            return Ok(None);
        }

        // Try each matching window — first one that yields a clean read wins.
        for hwnd in hwnds {
            // HWND is windows@0.58; uiautomation uses windows@0.62 — the
            // types are distinct crate versions so From<HWND> doesn't cross.
            // Bridge via the raw isize handle value, which Handle: From<isize>.
            let root = match automation.element_from_handle((hwnd.0 as isize).into()) {
                Ok(elem) => elem,
                Err(_) => continue,
            };

            // Single combined DFS: collects title/artist/album (class-based),
            // Pandora hyperlink URLs (for ad classification), and countdown
            // text (ad timer). Replaces the prior three separate walks.
            let data = collect_pandora_web_data(&automation, &root);

            // Ad classification — check URLs first.
            let ad_state = crate::pandora_desktop::classify_pandora_state(
                &data.urls,
                data.countdown.as_deref(),
            );

            if ad_state.is_ad {
                // Mirror Task 7's simpler position_ms approach: position = 0,
                // duration = current countdown value (shrinks over the ad).
                // Accurate initial-duration tracking would require caching
                // per-ad-key across polls — deferred, same rationale as Task 7.
                let dur_ms = ad_state
                    .countdown_seconds
                    .map(|s| s * 1_000)
                    .unwrap_or(30_000); // 30s fallback when countdown unreadable
                return Ok(Some(WebBridgeTrack {
                    title: String::new(),
                    artist: String::new(),
                    album: String::new(),
                    source: "pandora-web".into(),
                    last_seen_unix_ms: now_unix_ms(),
                    position_ms: Some(0),
                    // pandora-web doesn't track its own playback state;
                    // SMTC handles that via Chrome's MediaSession.
                    state: None,
                    is_ad: true,
                    duration_ms: Some(dur_ms),
                }));
            }

            // Normal track path — use the class-based title/artist/album.
            let title = dedupe_doubled(&data.title_raw);
            let artist = dedupe_doubled(&data.artist_raw);
            let album = dedupe_doubled(&data.album_raw);

            if title.is_empty() {
                continue;
            }

            return Ok(Some(WebBridgeTrack {
                title,
                artist,
                album,
                source: self.name().to_string(),
                last_seen_unix_ms: now_unix_ms(),
                // Pandora-in-Chrome leaves position + state to SMTC —
                // Chrome itself publishes timeline data, so SMTC's
                // position/state are already correct for this case.
                position_ms: None,
                state: None,
                is_ad: false,
                duration_ms: None,
            }));
        }

        Ok(None)
    }
}

/// Spawn the bridge worker. The worker watches the SMTC snapshot and,
/// when a probe matches, polls UIA every 2s. Idle (5s tick, zero UIA
/// calls) when no probe matches.
pub fn start(app: AppHandle, snapshot: SharedSnapshot, shared: SharedWebBridge) {
    // HTTP client for iTunes Search art lookups. Created once and cloned
    // per-track so we get connection reuse across requests.
    let http_client = match reqwest::Client::builder()
        .user_agent(format!("hum/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[web_bridge] failed to build http client for art lookup: {e}");
            return;
        }
    };

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
                            let art_title = track.title.clone();
                            let art_artist = track.artist.clone();
                            let has_bridge_position = track.position_ms.is_some();
                            {
                                let mut w = shared.write().await;
                                *w = Some(track);
                            }
                            // Probes that supply their own position (e.g.
                            // Pandora desktop, where SMTC publishes either
                            // nothing or unrelated app data) need the
                            // overlay to re-read from the snapshot every
                            // tick so lyrics-line interpolation tracks the
                            // bridge's position instead of SMTC's stale
                            // timeline. Emit a synthesized timeline-changed
                            // event after blending; the frontend already
                            // wires this to applyTrack().
                            //
                            // BUT — yield to SMTC when SMTC has a real
                            // actively-playing session (Spotify / iTunes /
                            // etc.). Their published position is real;
                            // ours is estimated. Emitting our timeline-
                            // changed in that case would flicker the
                            // frontend between two sources every 2s.
                            let smtc_actively_playing = {
                                let s = snapshot.read().await;
                                matches!(s.state, crate::smtc::PlaybackState::Playing)
                                    && !s.title.trim().is_empty()
                            };
                            if has_bridge_position && !smtc_actively_playing {
                                let mut blended = { snapshot.read().await.clone() };
                                blend_bridge_into_snapshot(&mut blended, &shared).await;
                                // Sync ad_active back to the SHARED snapshot
                                // — the lyrics resolver reads the shared
                                // snapshot to decide ad-short-circuit, not
                                // the emit payload. Without this write, the
                                // frontend's `track` state (from the emit
                                // payload) would correctly carry ad_active
                                // = true while the resolver still sees the
                                // stale false on shared, leading to the
                                // AD BREAK chip firing but the lyric area
                                // showing "fetching" / "no lyrics" instead
                                // of the SYVR promo card. Same root cause
                                // as the smtc::emit_blended sync — same fix.
                                {
                                    let mut s = snapshot.write().await;
                                    s.ad_active = blended.ad_active;
                                }
                                let _ = app.emit("timeline-changed", &blended);
                            }
                            if new_title != last_emitted_title {
                                eprintln!(
                                    "[web_bridge] probe={name} read title={new_title:?}, emitting web-bridge-updated"
                                );
                                last_emitted_title = new_title;
                                let _ = app.emit("web-bridge-updated", ());

                                // Kick off art fetch in the background so it
                                // doesn't slow the 2s polling cadence.
                                let app_for_art = app.clone();
                                let client_for_art = http_client.clone();
                                tauri::async_runtime::spawn(async move {
                                    let Some(data_url) = crate::smtc::fetch_art_via_itunes(
                                        &client_for_art,
                                        &art_artist,
                                        &art_title,
                                    )
                                    .await
                                    else {
                                        eprintln!(
                                            "[web_bridge] art lookup miss for {art_artist:?} - {art_title:?}"
                                        );
                                        return;
                                    };
                                    let payload = crate::smtc::AlbumArtPayload {
                                        title: art_title,
                                        artist: art_artist,
                                        data_url,
                                    };
                                    // Mirror smtc::spawn_art_fetch's cache-then-emit
                                    // pattern so get_current_album_art (called by
                                    // the frontend on mount) returns this payload
                                    // for the bridge-keyed title, not whatever
                                    // SMTC's spawn_art_fetch last wrote.
                                    use tauri::Manager;
                                    let art_state =
                                        app_for_art.state::<crate::smtc::SharedAlbumArt>();
                                    *art_state.inner().write().await = Some(payload.clone());
                                    let _ = app_for_art.emit("album-art-loaded", &payload);
                                });
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
    fn pandora_probe_detects_via_smtc_only() {
        // PandoraProbe is the SMTC-gated probe — tested in isolation so the
        // assertions don't depend on whether a desktop Pandora.exe is
        // currently running (which would flip PandoraDesktopProbe).
        let p = PandoraProbe;
        assert!(p.detects(
            "Today's Hits Radio - Now Playing on Pandora",
            "Chrome.exe",
        ));
        assert!(!p.detects(
            "Rick Astley - Never Gonna Give You Up",
            "Chrome.exe",
        ));
    }

    #[test]
    fn dedupe_doubled_handles_exact_doubling() {
        assert_eq!(dedupe_doubled("Be Like That Be Like That"), "Be Like That");
        assert_eq!(dedupe_doubled("Different Man Different Man"), "Different Man");
    }

    #[test]
    fn dedupe_doubled_preserves_non_doubled_strings() {
        assert_eq!(dedupe_doubled("Be Like That"), "Be Like That");
        assert_eq!(dedupe_doubled("Be Like That (Alex Waldin Remix)"), "Be Like That (Alex Waldin Remix)");
        assert_eq!(dedupe_doubled("Kane Brown, Swae Lee & Khalid"), "Kane Brown, Swae Lee & Khalid");
        // Two different halves that happen to be even-length total — don't trim.
        assert_eq!(dedupe_doubled("Hello World"), "Hello World");
    }

    #[test]
    fn dedupe_doubled_trims_whitespace() {
        assert_eq!(dedupe_doubled("  Song  "), "Song");
        assert_eq!(dedupe_doubled(""), "");
        assert_eq!(dedupe_doubled("   "), "");
    }
}
