//! Pandora desktop bridge (Microsoft Store / Chromium-shelled `Pandora.exe`).
//!
//! Pandora's Microsoft Store app is a Chromium-shelled desktop client — same
//! window class (`Chrome_WidgetWin_1`) and renderer (`Chrome_RenderWidgetHostHWND`)
//! as Chrome itself, but a distinct executable identity. Crucially, **it
//! does not publish to Windows SMTC**: opening Pandora and playing a track
//! leaves SMTC stuck on whatever app last published (often iTunes), so the
//! lyrics overlay shows the wrong song.
//!
//! Fix: detect the Pandora.exe process via Win32 window enumeration and
//! extract the now-playing track + artist directly from the renderer's UI
//! Automation tree. Selectors were discovered by `cargo run --bin dump_uia`
//! during the v0.11.2 dev-tool slice.
//!
//! ## UIA selector strategy
//!
//! The now-playing block in Pandora's React shell renders the track, artist,
//! and album as Hyperlink elements with absolute Pandora URLs:
//!
//! ```text
//! [Hyperlink]  Name="Country Grammar (Hot Shit)"
//!              Value="https://www.pandora.com/artist/nelly/country-grammar-deluxe-edition/country-grammar-hot-shit/TRk97VdtdjmnjgX"
//! [Hyperlink]  Name="Nelly"
//!              Value="https://www.pandora.com/artist/nelly/AR6tvkckhd75l2J"
//! [Hyperlink]  Name="Country Grammar (Deluxe Edition)"
//!              Value="https://www.pandora.com/artist/nelly/country-grammar-deluxe-edition/AL7Klp5tl6hztxJ"
//! ```
//!
//! Path semantics are part of Pandora's public product surface (these are
//! deep-link URLs end users navigate to), so they're more durable than CSS
//! class substrings. We classify by the **last path segment**:
//!
//! - Starts with `TR` → track (use Name as title)
//! - Starts with `AR` → artist
//! - Starts with `AL` → album (optional)
//!
//! Take the FIRST hit of each kind encountered in a top-down DFS — Pandora
//! renders the now-playing block before any "Recently Played" / artist-bio /
//! album-details Hyperlinks in document order.

use std::sync::{Mutex, OnceLock};

use anyhow::anyhow;
use regex::Regex;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
};

use crate::smtc::PlaybackState;
use crate::web_bridge::{
    read_process_name_for_window, read_window_title, WebBridgeTrack, WebPlayerProbe,
};

/// Per-track playback state tracker. Pandora doesn't expose the seek bar to
/// UI Automation, so we can't read the *real* current position — instead
/// we estimate by counting wall-clock ms while the play state is "Playing"
/// and freezing the count while "Paused". Wrong when Hum joins mid-song
/// (we'd report position 0 instead of wherever the user actually is), but
/// correct for: tracks that play through from the start, track changes
/// while Hum is open, and pause/resume cycles within a track.
struct TrackPlayState {
    /// `format!("{title}|{artist}")` of the active track.
    key: String,
    /// Total ms played BEFORE the current play period. Frozen while paused.
    cumulative_ms: u64,
    /// Unix epoch ms when the current play period started — or, while
    /// paused, when the pause started (so it can be advanced forward on
    /// resume without retroactively crediting paused time).
    period_start_unix_ms: i64,
    /// The play state at the end of the last poll.
    state: PlaybackState,
}

static TRACK_PLAY: Mutex<Option<TrackPlayState>> = Mutex::new(None);

/// Update the per-track state machine and return the position + state the
/// bridge should report this poll. See `TrackPlayState` doc for the
/// estimation rationale.
///
/// State transitions (`prev -> curr`):
/// - new track key       → reset, position = 0
/// - Playing  → Playing  → position = cumulative + (now - period_start), no state mutation
/// - Playing  → Paused   → cumulative += elapsed; period_start = now; freeze
/// - Paused   → Playing  → period_start = now; resume advancing
/// - Paused   → Paused   → position = cumulative (frozen)
fn update_track_state(
    track_key: String,
    detected_state: PlaybackState,
    now_unix_ms: i64,
) -> (u64, PlaybackState) {
    let mut guard = TRACK_PLAY.lock().expect("TRACK_PLAY mutex poisoned");

    let needs_reset = match guard.as_ref() {
        Some(s) => s.key != track_key,
        None => true,
    };
    if needs_reset {
        *guard = Some(TrackPlayState {
            key: track_key,
            cumulative_ms: 0,
            period_start_unix_ms: now_unix_ms,
            state: detected_state,
        });
        // Position 0 regardless of detected_state; cumulative starts fresh.
        return (0, detected_state);
    }

    let s = guard.as_mut().expect("guard is Some after reset branch");
    match (s.state, detected_state) {
        (PlaybackState::Playing, PlaybackState::Playing) => {
            let elapsed = (now_unix_ms - s.period_start_unix_ms).max(0) as u64;
            (s.cumulative_ms + elapsed, PlaybackState::Playing)
        }
        (PlaybackState::Playing, PlaybackState::Paused) => {
            let elapsed = (now_unix_ms - s.period_start_unix_ms).max(0) as u64;
            s.cumulative_ms = s.cumulative_ms.saturating_add(elapsed);
            s.state = PlaybackState::Paused;
            s.period_start_unix_ms = now_unix_ms;
            (s.cumulative_ms, PlaybackState::Paused)
        }
        (PlaybackState::Paused, PlaybackState::Playing) => {
            s.state = PlaybackState::Playing;
            s.period_start_unix_ms = now_unix_ms;
            (s.cumulative_ms, PlaybackState::Playing)
        }
        (PlaybackState::Paused, PlaybackState::Paused) => {
            (s.cumulative_ms, PlaybackState::Paused)
        }
        // Any other detected_state (Unknown/Stopped/Changing/Closed/Opened)
        // is treated as "no change to the timer" but the reported state
        // tracks the detection so suppressing logic can act on it.
        _ => {
            let reported_position = match s.state {
                PlaybackState::Playing => {
                    let elapsed = (now_unix_ms - s.period_start_unix_ms).max(0) as u64;
                    s.cumulative_ms.saturating_add(elapsed)
                }
                _ => s.cumulative_ms,
            };
            (reported_position, s.state)
        }
    }
}

pub struct PandoraDesktopProbe;

impl WebPlayerProbe for PandoraDesktopProbe {
    fn name(&self) -> &'static str {
        "pandora-desktop"
    }

    /// Pandora desktop is detected by process enumeration, NOT SMTC: the
    /// app doesn't publish to SMTC at all, so the snapshot's title/app_id
    /// reflect *whichever other app last published* (often iTunes), not
    /// Pandora. The trait's `smtc_title` / `smtc_app_id` arguments are
    /// intentionally ignored here.
    fn detects(&self, _smtc_title: &str, _smtc_app_id: &str) -> bool {
        !find_pandora_desktop_windows().is_empty()
    }

    fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>> {
        use uiautomation::UIAutomation;

        let hwnds = find_pandora_desktop_windows();
        if hwnds.is_empty() {
            return Ok(None);
        }

        let automation = UIAutomation::new()
            .map_err(|e| anyhow!("UIAutomation::new failed: {e:?}"))?;

        // Try each matching window — first one that yields a usable read wins.
        for hwnd in hwnds {
            // Fresh-anchor through the HWND. This is what wakes the Chromium
            // accessibility tree; walking from `get_root_element()` instead
            // returns a 14-node shell with the renderer subtree empty.
            let root = match automation.element_from_handle((hwnd.0 as isize).into()) {
                Ok(elem) => elem,
                Err(_) => continue,
            };

            // Collect all Pandora URLs + countdown text in one DFS pass.
            let (urls, countdown) = collect_pandora_uia_data(&automation, &root);

            // Classify: ad or normal track?
            let state_result = classify_pandora_state(&urls, countdown.as_deref());

            // Read play/pause: WASAPI peak meter first (canonical, works
            // independent of Pandora's UIA hygiene), then fall back to
            // UIA pattern reads if no audio session is found for this
            // PID. Default to Playing if neither produces a verdict.
            let pid = pid_for_window(hwnd);
            let detected_state =
                detect_playback_state_with_audio(&automation, &root, pid)
                    .unwrap_or(PlaybackState::Playing);

            let now_unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            if state_result.is_ad {
                // Simple approach: duration = countdown_seconds (what the
                // countdown shows right now). This means duration decreases
                // over the ad — position stays 0 (progress bar at 0%).
                // The AD BREAK badge still fires correctly. Documented in
                // commit message. Full initial-duration caching deferred to
                // a future task if Wes wants accurate progress.
                let dur_ms = state_result
                    .countdown_seconds
                    .map(|s| s * 1_000)
                    .unwrap_or(30_000); // 30s fallback when countdown unreadable
                return Ok(Some(WebBridgeTrack {
                    title: String::new(),
                    artist: String::new(),
                    album: String::new(),
                    source: "pandora-desktop".into(),
                    last_seen_unix_ms: now_unix_ms,
                    position_ms: Some(0),
                    state: Some(detected_state),
                    is_ad: true,
                    duration_ms: Some(dur_ms),
                }));
            }

            // Normal track path — use the URL-extracted title/artist/album.
            let Some((title, artist, album)) =
                extract_track_from_uia_subtree(&automation, &root)
            else {
                continue;
            };

            let track_key = format!("{title}|{artist}");
            let (position_ms, reported_state) =
                update_track_state(track_key, detected_state, now_unix_ms);

            return Ok(Some(WebBridgeTrack {
                title,
                artist,
                album,
                source: self.name().to_string(),
                last_seen_unix_ms: now_unix_ms,
                position_ms: Some(position_ms),
                state: Some(reported_state),
                is_ad: false,
                duration_ms: None,
            }));
        }

        Ok(None)
    }
}

/// Walk the UIA subtree rooted at `root` looking for the now-playing
/// Hyperlinks (`Name` + `Value`) Pandora's renderer exposes. Returns
/// `Some((title, artist, album))` once a track and artist are both found
/// (album is best-effort — empty string when not present).
///
/// Detect Pandora's play/pause state. Tries the canonical signal first
/// (WASAPI audio session peak meter — works regardless of Pandora's UIA
/// hygiene), falls back to UIA pattern reads if WASAPI fails to find a
/// session for the PID.
///
/// `pid_hint` is the process ID of the Pandora window we're probing,
/// used to scope the WASAPI session lookup. When `None`, only UIA is
/// consulted.
fn detect_playback_state_with_audio(
    automation: &uiautomation::UIAutomation,
    root: &uiautomation::UIElement,
    pid_hint: Option<u32>,
) -> Option<PlaybackState> {
    // WASAPI first: peak meter is 0 when the session is silent (paused
    // or muted), nonzero when audio is actively flowing. This is the
    // signal iTunes/Spotify implicitly produce through SMTC; we're
    // synthesizing the equivalent for an app that doesn't publish.
    if let Some(pid) = pid_hint {
        if let Some(silent) = is_process_audio_silent(pid) {
            return Some(if silent {
                PlaybackState::Paused
            } else {
                PlaybackState::Playing
            });
        }
    }
    detect_playback_state_via_uia(automation, root)
}

/// Walk WASAPI's audio sessions for the default render endpoint and check
/// the peak meter of the session belonging to `pid`. Returns:
/// - `Some(true)`  → session found, peak < threshold (paused/muted)
/// - `Some(false)` → session found, peak >= threshold (actively playing)
/// - `None`        → no session found for this pid, or COM failure
fn is_process_audio_silent(pid: u32) -> Option<bool> {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioSessionControl2, IAudioSessionEnumerator,
        IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    // Anything below this peak amplitude (linear 0.0..1.0) we treat as
    // silent. Pandora paused leaves the session in "Inactive" state with
    // peak 0; even Windows mixer dithering well below this floor counts
    // as silence in practice.
    const SILENCE_FLOOR: f32 = 0.0001;

    unsafe {
        // CoInitializeEx is idempotent per-thread; ignore RPC_E_CHANGED_MODE
        // (someone else already initialized this thread in a compatible
        // apartment).
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        let session_mgr: IAudioSessionManager2 =
            device.Activate(CLSCTX_ALL, None).ok()?;
        let sessions: IAudioSessionEnumerator = session_mgr.GetSessionEnumerator().ok()?;

        let count = sessions.GetCount().ok()?;
        for i in 0..count {
            let session = match sessions.GetSession(i) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let session2: IAudioSessionControl2 = match session.cast() {
                Ok(s) => s,
                Err(_) => continue,
            };
            let session_pid = match session2.GetProcessId() {
                Ok(p) => p,
                Err(_) => continue,
            };
            if session_pid != pid {
                continue;
            }
            let meter: IAudioMeterInformation = match session.cast() {
                Ok(m) => m,
                Err(_) => continue,
            };
            let peak = meter.GetPeakValue().ok()?;
            return Some(peak < SILENCE_FLOOR);
        }
        None
    }
}

/// Detect Pandora's play/pause state by walking the UIA tree for the
/// playback control bar's Play button. Strategy:
///
/// 1. Find a `Button` element whose `Name` is exactly `"Play"` or `"Pause"`.
///    Pandora's React shell sometimes labels it statically as `"Play"`
///    regardless of state, so step 2 is the more reliable signal.
/// 2. Try to read the `TogglePattern` from that button. Pandora's button
///    sets `aria-pressed`, which UIA maps to `ToggleState::On` when
///    *playing* (button is "engaged") and `Off` when *paused*. Returns
///    `Some(Playing)` for `On`, `Some(Paused)` for `Off`.
/// 3. If TogglePattern isn't available, fall back to interpreting the
///    button's `Name`: `"Pause"` => Playing (clicking would pause),
///    `"Play"` => Paused (clicking would play). When Name is static this
///    is wrong, but it's the best we have without the pattern.
///
/// Returns `None` if no Play/Pause button can be found at all (we'll then
/// default to `Playing` in the caller — same behavior as v0.11.4-v0.11.6).
fn detect_playback_state_via_uia(
    automation: &uiautomation::UIAutomation,
    root: &uiautomation::UIElement,
) -> Option<PlaybackState> {
    use uiautomation::patterns::UITogglePattern;
    use uiautomation::types::ToggleState;

    const MAX_NODES: usize = 10_000;
    let walker = automation.get_control_view_walker().ok()?;

    let mut stack: Vec<uiautomation::UIElement> = vec![root.clone()];
    let mut visited = 0_usize;
    let mut name_only_hit: Option<PlaybackState> = None;

    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > MAX_NODES {
            break;
        }

        if let Ok(name) = node.get_name() {
            let trimmed = name.trim();
            if trimmed.eq_ignore_ascii_case("Play") || trimmed.eq_ignore_ascii_case("Pause") {
                // Prefer the TogglePattern signal — Name is often static.
                if let Ok(toggle) = node.get_pattern::<UITogglePattern>() {
                    if let Ok(state) = toggle.get_toggle_state() {
                        return Some(match state {
                            ToggleState::On => PlaybackState::Playing,
                            ToggleState::Off => PlaybackState::Paused,
                            ToggleState::Indeterminate => PlaybackState::Unknown,
                        });
                    }
                }
                // No toggle — fall back to interpreting the Name as the
                // *action* the button performs. Record but keep walking
                // in case another button has the pattern.
                if name_only_hit.is_none() {
                    name_only_hit = Some(if trimmed.eq_ignore_ascii_case("Pause") {
                        PlaybackState::Playing
                    } else {
                        PlaybackState::Paused
                    });
                }
            }
        }

        if let Ok(first) = walker.get_first_child(&node) {
            let mut cur = Some(first);
            let mut kids: Vec<uiautomation::UIElement> = Vec::new();
            while let Some(c) = cur {
                kids.push(c.clone());
                cur = walker.get_next_sibling(&c).ok();
            }
            for c in kids.into_iter().rev() {
                stack.push(c);
            }
        }
    }

    name_only_hit
}

/// Walk the UIA subtree and collect:
/// 1. All Hyperlink `Value` URLs (for track/artist/album classification).
/// 2. The first text node whose `Name` matches `^\d+:\d{2}$` (countdown
///    widget shown during ads, e.g. "0:23").
///
/// Returns `(urls, countdown_text)`. The caller passes these to
/// `classify_pandora_state` to decide ad vs. normal-track.
fn collect_pandora_uia_data(
    automation: &uiautomation::UIAutomation,
    root: &uiautomation::UIElement,
) -> (Vec<String>, Option<String>) {
    static COUNTDOWN_RE: OnceLock<Regex> = OnceLock::new();
    let countdown_re = COUNTDOWN_RE.get_or_init(|| {
        Regex::new(r"^\d+:\d{2}$").expect("countdown regex is valid")
    });

    const MAX_NODES: usize = 10_000;
    let walker = match automation.get_control_view_walker() {
        Ok(w) => w,
        Err(_) => return (Vec::new(), None),
    };

    let mut urls: Vec<String> = Vec::new();
    let mut countdown: Option<String> = None;

    let mut stack: Vec<uiautomation::UIElement> = vec![root.clone()];
    let mut visited = 0_usize;

    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > MAX_NODES {
            eprintln!(
                "[pandora_desktop] collect_uia_data hit MAX_NODES={MAX_NODES}"
            );
            break;
        }

        // Collect Hyperlink URLs (track/artist/album classification).
        if let Some(value) = read_value_pattern(&node) {
            if classify_pandora_url(&value).is_some() {
                urls.push(value);
            }
        }

        // Collect countdown text (ad timer: "0:23", "1:05", etc.).
        // Only capture the first match.
        if countdown.is_none() {
            if let Ok(name) = node.get_name() {
                if countdown_re.is_match(name.trim()) {
                    countdown = Some(name.trim().to_string());
                }
            }
        }

        // Enqueue children in reverse (left-to-right DFS).
        if let Ok(first) = walker.get_first_child(&node) {
            let mut cur = Some(first);
            let mut kids: Vec<uiautomation::UIElement> = Vec::new();
            while let Some(c) = cur {
                kids.push(c.clone());
                cur = walker.get_next_sibling(&c).ok();
            }
            for c in kids.into_iter().rev() {
                stack.push(c);
            }
        }
    }

    (urls, countdown)
}

/// Pulled out as a free function so the URL-classification logic (the
/// stable part) can be unit-tested without UIA.
fn extract_track_from_uia_subtree(
    automation: &uiautomation::UIAutomation,
    root: &uiautomation::UIElement,
) -> Option<(String, String, String)> {
    const MAX_NODES: usize = 10_000;
    let walker = automation.get_control_view_walker().ok()?;

    let mut track: Option<String> = None;
    let mut artist: Option<String> = None;
    let mut album: Option<String> = None;

    // DFS preorder. Push children in reverse so we pop them in document
    // order — guarantees the FIRST Hyperlink of each kind in document
    // order wins, which is the now-playing block.
    let mut stack: Vec<uiautomation::UIElement> = vec![root.clone()];
    let mut visited = 0_usize;

    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > MAX_NODES {
            eprintln!(
                "[pandora_desktop] tree walk hit MAX_NODES={MAX_NODES} before finding track"
            );
            break;
        }

        if let (Ok(name), Some(value)) = (node.get_name(), read_value_pattern(&node)) {
            if let Some(kind) = classify_pandora_url(&value) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    match kind {
                        PandoraUrlKind::Track => {
                            if track.is_none() {
                                track = Some(trimmed.to_string());
                            }
                        }
                        PandoraUrlKind::Artist => {
                            if artist.is_none() {
                                artist = Some(trimmed.to_string());
                            }
                        }
                        PandoraUrlKind::Album => {
                            if album.is_none() {
                                album = Some(trimmed.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Early-out the moment we have track + artist. Album is optional;
        // we won't make extra walk passes for it.
        if track.is_some() && artist.is_some() {
            break;
        }

        if let Ok(first) = walker.get_first_child(&node) {
            let mut cur = Some(first);
            let mut kids: Vec<uiautomation::UIElement> = Vec::new();
            while let Some(c) = cur {
                kids.push(c.clone());
                cur = walker.get_next_sibling(&c).ok();
            }
            for c in kids.into_iter().rev() {
                stack.push(c);
            }
        }
    }

    let track = track?;
    let artist = artist?;
    Some((track, artist, album.unwrap_or_default()))
}

pub(crate) struct PandoraStateResult {
    pub is_ad: bool,
    /// Seconds remaining in the ad, if the countdown widget was readable.
    /// None when the countdown text couldn't be parsed.
    pub countdown_seconds: Option<u64>,
}

/// Given the URLs found in the player region + optionally the countdown
/// widget text, classify as ad or normal track.
///
/// - At least one URL matching `classify_pandora_url(…)` returning
///   `Some(PandoraUrlKind::Track)` → normal track. Otherwise → ad.
/// - Countdown text parsing: `M:SS` format. Returns total seconds when
///   parseable, None otherwise. Ad classification is independent of
///   countdown parseability.
pub(crate) fn classify_pandora_state(
    urls: &[String],
    countdown_text: Option<&str>,
) -> PandoraStateResult {
    let has_track = urls
        .iter()
        .any(|u| matches!(classify_pandora_url(u), Some(PandoraUrlKind::Track)));
    let countdown_seconds = countdown_text.and_then(parse_countdown_to_seconds);
    PandoraStateResult {
        is_ad: !has_track,
        countdown_seconds,
    }
}

fn parse_countdown_to_seconds(text: &str) -> Option<u64> {
    let text = text.trim();
    let (mins, secs) = text.split_once(':')?;
    let mins: u64 = mins.parse().ok()?;
    let secs: u64 = secs.parse().ok()?;
    if secs >= 60 {
        return None;
    }
    Some(mins * 60 + secs)
}

fn read_value_pattern(elem: &uiautomation::UIElement) -> Option<String> {
    use uiautomation::patterns::UIValuePattern;
    let p = elem.get_pattern::<UIValuePattern>().ok()?;
    let v = p.get_value().ok()?;
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PandoraUrlKind {
    Track,
    Artist,
    Album,
}

/// Classify a Pandora URL by its last path segment's two-character prefix.
///
/// Pandora's deep-link URL shape (stable, part of the public product surface):
/// - `https://www.pandora.com/artist/{slug}/{album-slug}/{track-slug}/TR{id}` → track
/// - `https://www.pandora.com/artist/{slug}/AR{id}` → artist
/// - `https://www.pandora.com/artist/{slug}/{album-slug}/AL{id}` → album
///
/// Anything else (search URLs, station URLs, lyrics URLs, account URLs)
/// returns `None`.
pub(crate) fn classify_pandora_url(url: &str) -> Option<PandoraUrlKind> {
    const PREFIX: &str = "https://www.pandora.com/artist/";
    if !url.starts_with(PREFIX) {
        return None;
    }
    // Reject the "See All Lyrics" link: it shares the /TR{id} suffix with
    // the real track URL, but its Name property is the string "See All
    // Lyrics" — adopting it as the song title would be wrong. Path shape:
    //   /artist/lyrics/{artist}/{album}/{track-slug}/TR{id}
    // vs the real track at:
    //   /artist/{artist}/{album}/{track-slug}/TR{id}
    if url.starts_with("https://www.pandora.com/artist/lyrics/") {
        return None;
    }
    // Last non-empty path segment.
    let last_seg = url.rsplit('/').find(|s| !s.is_empty())?;
    if last_seg.len() < 3 {
        return None;
    }
    let prefix = &last_seg[..2];
    // The rest of the segment must be a non-empty ID (alphanumeric — Pandora
    // uses base-62-ish IDs like TRk97VdtdjmnjgX or AR6tvkckhd75l2J).
    let id = &last_seg[2..];
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    match prefix {
        "TR" => Some(PandoraUrlKind::Track),
        "AR" => Some(PandoraUrlKind::Artist),
        "AL" => Some(PandoraUrlKind::Album),
        _ => None,
    }
}

/// Read the process ID owning `hwnd`. Returns `None` if the call fails or
/// PID is zero. Used to scope WASAPI session lookups to the Pandora
/// window we're probing.
fn pid_for_window(hwnd: HWND) -> Option<u32> {
    let mut pid: u32 = 0;
    let _ = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        None
    } else {
        Some(pid)
    }
}

/// Enumerate visible top-level windows belonging to `Pandora.exe`. Empty
/// vector when the app isn't running. Filters by process file name (not
/// window title) so we still match if the app starts on the Browse / My
/// Collection tabs (titles that don't end with "Now Playing on Pandora").
fn find_pandora_desktop_windows() -> Vec<HWND> {
    struct Ctx {
        hits: Vec<HWND>,
    }

    let mut ctx = Ctx { hits: Vec::new() };

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: lparam was set to a valid &mut Ctx by EnumWindows; the
        // reference outlives this synchronous call.
        let ctx = unsafe { &mut *(lparam.0 as *mut Ctx) };

        if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return BOOL(1);
        }

        let process_name = read_process_name_for_window(hwnd);
        if process_name.eq_ignore_ascii_case("Pandora.exe") {
            // Belt-and-braces: the Microsoft Store Pandora app uses one
            // top-level window with a non-empty title once content loads;
            // skip placeholder splash windows that haven't drawn yet.
            let title = read_window_title(hwnd);
            if !title.is_empty() {
                ctx.hits.push(hwnd);
            }
        }
        BOOL(1)
    }

    let ctx_ptr: *mut Ctx = &mut ctx;
    let _ = unsafe { EnumWindows(Some(enum_proc), LPARAM(ctx_ptr as isize)) };

    ctx.hits
}

#[cfg(test)]
mod ad_detection_tests {
    use super::*;

    /// Classify ad detection logic in isolation from UIA. We test the
    /// classifier that takes an enumerated set of Hyperlink URLs + a
    /// possibly-empty countdown text and decides ad-ness.
    #[test]
    fn empty_url_set_with_pandora_window_present_is_ad() {
        let urls: Vec<String> = vec![];
        let countdown = Some("0:23".to_string());
        let result = classify_pandora_state(&urls, countdown.as_deref());
        assert!(result.is_ad, "no /TR URLs + countdown present → ad");
        assert_eq!(result.countdown_seconds, Some(23));
    }

    #[test]
    fn url_set_with_TR_link_is_not_ad() {
        let urls = vec!["https://www.pandora.com/artist/x/y/TR123abc".into()];
        let result = classify_pandora_state(&urls, None);
        assert!(!result.is_ad);
    }

    #[test]
    fn countdown_parses_minutes_seconds() {
        let urls: Vec<String> = vec![];
        let result = classify_pandora_state(&urls, Some("1:05"));
        assert!(result.is_ad);
        assert_eq!(result.countdown_seconds, Some(65));
    }

    #[test]
    fn countdown_parses_zero_seconds() {
        let urls: Vec<String> = vec![];
        let result = classify_pandora_state(&urls, Some("0:00"));
        assert!(result.is_ad);
        assert_eq!(result.countdown_seconds, Some(0));
    }

    #[test]
    fn malformed_countdown_returns_none() {
        let urls: Vec<String> = vec![];
        let result = classify_pandora_state(&urls, Some("not a countdown"));
        assert!(result.is_ad, "no /TR URLs is still an ad signal");
        assert_eq!(result.countdown_seconds, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_track_url() {
        let url = "https://www.pandora.com/artist/nelly/country-grammar-deluxe-edition/country-grammar-hot-shit/TRk97VdtdjmnjgX";
        assert_eq!(classify_pandora_url(url), Some(PandoraUrlKind::Track));
    }

    #[test]
    fn classify_artist_url() {
        let url = "https://www.pandora.com/artist/nelly/AR6tvkckhd75l2J";
        assert_eq!(classify_pandora_url(url), Some(PandoraUrlKind::Artist));
    }

    #[test]
    fn classify_album_url() {
        let url =
            "https://www.pandora.com/artist/nelly/country-grammar-deluxe-edition/AL7Klp5tl6hztxJ";
        assert_eq!(classify_pandora_url(url), Some(PandoraUrlKind::Album));
    }

    #[test]
    fn classify_rejects_wrong_prefix() {
        // No /artist/ root.
        assert_eq!(
            classify_pandora_url("https://www.pandora.com/station/play/12345"),
            None,
        );
        assert_eq!(
            classify_pandora_url("https://www.pandora.com/upgrade"),
            None,
        );
    }

    #[test]
    fn classify_rejects_non_pandora_host() {
        assert_eq!(
            classify_pandora_url(
                "https://www.spotify.com/artist/nelly/AR6tvkckhd75l2J"
            ),
            None,
        );
    }

    #[test]
    fn classify_rejects_unknown_two_char_prefix() {
        // ST, US, etc. — not one of the three known kinds.
        assert_eq!(
            classify_pandora_url(
                "https://www.pandora.com/artist/nelly/ST123abc"
            ),
            None,
        );
    }

    #[test]
    fn classify_rejects_empty_id() {
        assert_eq!(
            classify_pandora_url("https://www.pandora.com/artist/nelly/AR"),
            None,
        );
        assert_eq!(
            classify_pandora_url("https://www.pandora.com/artist/nelly/TR"),
            None,
        );
    }

    #[test]
    fn classify_rejects_non_alphanumeric_id() {
        // Hyphenated IDs aren't a thing in Pandora's URL space; reject.
        assert_eq!(
            classify_pandora_url(
                "https://www.pandora.com/artist/nelly/AR-not-a-real-id"
            ),
            None,
        );
    }

    #[test]
    fn classify_rejects_see_all_lyrics_url() {
        // The "See All Lyrics" link has the same /TR{id} suffix as the
        // real track URL but Name="See All Lyrics" — picking it up would
        // poison the bridge with a non-song title.
        let url = "https://www.pandora.com/artist/lyrics/justin-bieber-artist/my-world/1-time/TR9tK7b3cdjvnc6";
        assert_eq!(classify_pandora_url(url), None);
    }

    #[test]
    fn classify_trailing_slash_ignored() {
        // rsplit then skip-empties — trailing slash should not break
        // detection.
        let url = "https://www.pandora.com/artist/nelly/AR6tvkckhd75l2J/";
        assert_eq!(classify_pandora_url(url), Some(PandoraUrlKind::Artist));
    }
}
