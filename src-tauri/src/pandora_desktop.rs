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

use anyhow::anyhow;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible};

use crate::web_bridge::{
    read_process_name_for_window, read_window_title, WebBridgeTrack, WebPlayerProbe,
};

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

            let Some((title, artist, album)) =
                extract_track_from_uia_subtree(&automation, &root)
            else {
                continue;
            };

            let now_unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            return Ok(Some(WebBridgeTrack {
                title,
                artist,
                album,
                source: self.name().to_string(),
                last_seen_unix_ms: now_unix_ms,
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
enum PandoraUrlKind {
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
fn classify_pandora_url(url: &str) -> Option<PandoraUrlKind> {
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
