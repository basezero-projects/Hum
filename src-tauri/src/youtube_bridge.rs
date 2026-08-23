//! YouTube ad-break detection via Chrome UIA tree scraping.
//!
//! YouTube's normal track metadata flows through SMTC (Chrome publishes
//! it via the MediaSession API). Hum doesn't need to scrape non-ad
//! metadata. This probe runs only for ad detection.
//!
//! Detection strategy: when the SMTC source is a Chromium browser and
//! there is a non-empty SMTC title, walk the Chrome window's UIA tree
//! looking for ad-marker text nodes ("Sponsored", "Ad ·", "Skip Ad",
//! etc.) and an optional M:SS or M:SS / M:SS timer string. If markers
//! are found, we return an ad-shaped WebBridgeTrack and let the overlay
//! render the SYVR promo card. If no markers are found we return
//! Ok(None) so normal SMTC-sourced YouTube metadata is untouched.

use std::sync::OnceLock;

use anyhow::anyhow;
use regex::Regex;

use crate::web_bridge::{WebBridgeTrack, WebPlayerProbe};

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct YouTubeAdState {
    pub is_ad: bool,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}

/// Classify a YouTube ad state given (a) any text nodes found in the
/// player region and (b) any timer text found ("0:05 / 0:30" or "0:30").
///
/// Ad markers (any of these → is_ad true):
/// - "Sponsored"
/// - "Ad ·"
/// - "Advertisement"
/// - "Skip Ad"  // also matches "Skip in", "Skip Ad in"
pub(crate) fn classify_youtube_state(texts: &[String], timer_text: Option<&str>) -> YouTubeAdState {
    let markers = ["Sponsored", "Ad ·", "Advertisement", "Skip Ad", "Skip in"];
    let is_ad = texts.iter().any(|t| markers.iter().any(|m| t.contains(m)));

    if !is_ad {
        return YouTubeAdState {
            is_ad: false,
            position_ms: None,
            duration_ms: None,
        };
    }

    let (position_ms, duration_ms) = timer_text.map(parse_youtube_timer).unwrap_or((None, None));
    YouTubeAdState {
        is_ad,
        position_ms,
        duration_ms,
    }
}

/// Parse YouTube's timer in the format "M:SS / M:SS" (e.g. "0:05 / 0:30").
/// Returns (position_ms, duration_ms). If only one M:SS is present, treats
/// it as duration with position None.
fn parse_youtube_timer(text: &str) -> (Option<u64>, Option<u64>) {
    let text = text.trim();
    if let Some((left, right)) = text.split_once(" / ") {
        return (parse_mss_to_ms(left), parse_mss_to_ms(right));
    }
    (None, parse_mss_to_ms(text))
}

fn parse_mss_to_ms(text: &str) -> Option<u64> {
    let text = text.trim();
    let (mins, secs) = text.split_once(':')?;
    let mins: u64 = mins.parse().ok()?;
    let secs: u64 = secs.parse().ok()?;
    if secs >= 60 {
        return None;
    }
    Some((mins * 60 + secs) * 1000)
}

// ─── Probe implementation ────────────────────────────────────────────────────

pub(crate) struct YouTubeProbe;

impl WebPlayerProbe for YouTubeProbe {
    fn name(&self) -> &'static str {
        "youtube-web"
    }

    // YouTube's SMTC title is the real (decorated) song, not a placeholder,
    // so a missing/stale bridge read must NOT short-circuit the lyrics
    // resolver to Unsupported — `clean_title` + `strip_youtube_noise` can
    // still resolve it from the raw title. See the trait default's docs.
    fn smtc_unreliable_without_bridge(&self) -> bool {
        false
    }

    fn detects(&self, smtc_title: &str, smtc_app_id: &str) -> bool {
        // YouTube comes through SMTC via Chrome. Cheap gate: the SMTC
        // source is a Chromium browser AND the title is non-empty.
        // We don't have a stronger SMTC signal than "is this Chrome?" —
        // the probe's read() does the real check (finding a YouTube window).
        let app = smtc_app_id.to_lowercase();
        let is_chromium = app.contains("chrome")
            || app.contains("msedge")
            || app.contains("edge")
            || app.contains("brave")
            || app.contains("opera")
            || app.contains("vivaldi");
        if !is_chromium {
            return false;
        }
        // Heuristic: YouTube publishes via MediaSession to SMTC, so the
        // title is the video name. Just gate on "Chromium is the source"
        // and a non-empty title — the actual ad detection is in read().
        !smtc_title.trim().is_empty()
    }

    fn read(&self, smtc_title: &str, smtc_artist: &str) -> anyhow::Result<Option<WebBridgeTrack>> {
        // 1. Find a Chrome window whose title contains "YouTube".
        // 2. Re-anchor through element_from_handle to wake the accessibility tree.
        // 3. DFS for Text nodes that contain ad markers; also capture timer-shaped text.
        // 4. Classify via classify_youtube_state; if is_ad, return ad WebBridgeTrack.
        // 5. Otherwise normalize SMTC's channel-as-artist + decorated video
        //    title into a real (artist, title) so the overlay, lyrics
        //    resolver, and album-art lookup all get clean metadata.

        let hwnd = match crate::web_bridge::find_chromium_window_with_title_substring("YouTube") {
            Some(h) => h,
            None => return Ok(None),
        };

        let (texts, timer_text) = walk_for_ad_markers_and_timer(hwnd)?;
        let state = classify_youtube_state(&texts, timer_text.as_deref());

        if state.is_ad {
            let now_unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            return Ok(Some(WebBridgeTrack {
                page_url: None,
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                source: "youtube-web".into(),
                last_seen_unix_ms: now_unix_ms,
                position_ms: state.position_ms,
                state: None, // YouTube state continues through SMTC
                is_ad: true,
                duration_ms: state.duration_ms.or(Some(30_000)),
            }));
        }

        // Not an ad — normalize the SMTC metadata. YouTube (non-Music)
        // publishes the channel name as the artist ("7clouds") and the full
        // decorated video title as the title ("Fleetwood Mac - Dreams
        // (Lyrics)"). Recover the real artist + song so lyrics resolve via
        // LRCLib /api/get and art via iTunes/Deezer instead of failing on
        // the channel name.
        //
        // Gate on the active YouTube window actually showing this track:
        // the cheap `detects()` matches ANY Chromium tab, so without this a
        // Spotify-Web session playing alongside an open YouTube tab would
        // get its metadata rewritten as if it were a YouTube video.
        if smtc_title.trim().is_empty()
            || !crate::web_bridge::youtube_window_shows_track(smtc_title)
        {
            return Ok(None);
        }

        let (artist, title) = parse_youtube_metadata(smtc_title, smtc_artist);
        if title.trim().is_empty() {
            return Ok(None);
        }

        let now_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        // position_ms / duration_ms / state left None: SMTC owns the
        // timeline for YouTube. blend_bridge_into_snapshot only overwrites
        // those fields when they're Some, so SMTC's real position survives
        // while this supplies just the cleaned title/artist.
        // Diagnostics only, and only on the real-track path: an ad has no page
        // worth recording, and the resolver never reads this. Costs one
        // targeted UIA lookup against a window we have already located.
        let page_url = crate::web_bridge::read_chromium_address_bar(hwnd);

        Ok(Some(WebBridgeTrack {
            page_url,
            title,
            artist,
            album: String::new(),
            source: "youtube-web".into(),
            last_seen_unix_ms: now_unix_ms,
            position_ms: None,
            state: None,
            is_ad: false,
            duration_ms: None,
        }))
    }
}

/// Normalize YouTube's SMTC metadata into a real (artist, title).
///
/// YouTube (the video site, not YouTube Music) publishes via the
/// MediaSession API with `artist` = the uploading channel and `title` =
/// the full video title, which by uploader convention is usually
/// `"Real Artist - Real Song (Lyrics)"` / `"... (Official Video)"` etc.
///
/// Strategy:
/// 1. Run the title through `lyrics::clean_title` to drop the uploader
///    chrome — `(Lyrics)`, `[Official Music Video]`, ` | Lyric Video`,
///    trailing bare `Lyrics`, `.mp4` extensions, quote-bait excerpts.
/// 2. Strip a trailing ` ft./feat./featuring X` credit (clean_title leaves
///    bare feat tags alone; an exact LRCLib /api/get match can't have it).
/// 3. If the cleaned title splits on the FIRST ` - ` into two non-trivial
///    halves, treat them as `Real Artist - Real Song` and discard the
///    channel name. Otherwise keep the channel as the artist (stripping a
///    trailing ` - Topic`, YouTube's auto-generated art-track convention
///    where the channel IS the real artist) and the cleaned title as-is.
pub(crate) fn parse_youtube_metadata(smtc_title: &str, smtc_artist: &str) -> (String, String) {
    let cleaned = strip_trailing_feat(&crate::lyrics::clean_title(smtc_title));

    // Channel fallback artist — drop YouTube's " - Topic" auto-channel
    // suffix so "Fleetwood Mac - Topic" surfaces as "Fleetwood Mac".
    let channel = smtc_artist.trim();
    let channel = channel.strip_suffix(" - Topic").unwrap_or(channel).trim();

    if let Some((prefix, suffix)) = cleaned.split_once(" - ") {
        let real_artist = prefix.trim();
        let real_title = suffix.trim();
        // Guard against degenerate splits ("A - B", empty halves) eating a
        // title that merely happens to contain " - ".
        if real_artist.chars().filter(|c| !c.is_whitespace()).count() >= 2 && !real_title.is_empty()
        {
            return (real_artist.to_string(), real_title.to_string());
        }
    }

    let title = if cleaned.trim().is_empty() {
        smtc_title.trim().to_string()
    } else {
        cleaned
    };
    (channel.to_string(), title)
}

/// Strip a trailing ` ft.` / ` feat.` / ` featuring X` credit. Mirrors the
/// feat handling in `lyrics::strip_youtube_noise` but kept local so the
/// normalizer doesn't depend on a private lyrics helper.
fn strip_trailing_feat(title: &str) -> String {
    static FEAT_RE: OnceLock<Regex> = OnceLock::new();
    let feat_re =
        FEAT_RE.get_or_init(|| Regex::new(r"(?i)\s+(?:feat\.?|ft\.?|featuring)\s+.+$").unwrap());
    feat_re.replace(title, "").trim().to_string()
}

/// Walk the Chrome UIA tree anchored at `hwnd` collecting:
/// - All text node names (for ad marker matching; capped at 200 to avoid
///   memory-bombing on YouTube's verbose accessibility tree)
/// - The first M:SS or M:SS / M:SS timer string (ad position / duration)
///
/// Uses the same tree-walker API as `pandora_desktop.rs` — get_control_view_walker,
/// get_first_child, get_next_sibling — so the patterns stay consistent.
fn walk_for_ad_markers_and_timer(
    hwnd: windows::Win32::Foundation::HWND,
) -> anyhow::Result<(Vec<String>, Option<String>)> {
    use uiautomation::UIAutomation;

    static TIMER_RE: OnceLock<Regex> = OnceLock::new();
    let timer_re = TIMER_RE.get_or_init(|| {
        Regex::new(r"^\d+:\d{2}( / \d+:\d{2})?$").expect("youtube timer regex is valid")
    });

    const MAX_NODES: usize = 10_000;

    let automation = UIAutomation::new().map_err(|e| anyhow!("UIAutomation::new failed: {e:?}"))?;
    // Re-anchor via element_from_handle to wake the Chromium accessibility
    // tree — the same pattern used by PandoraProbe and PandoraDesktopProbe.
    let root = automation
        .element_from_handle((hwnd.0 as isize).into())
        .map_err(|e| anyhow!("element_from_handle failed: {e:?}"))?;

    let walker = automation
        .get_control_view_walker()
        .map_err(|e| anyhow!("get_control_view_walker failed: {e:?}"))?;

    let mut texts: Vec<String> = Vec::new();
    let mut timer: Option<String> = None;

    let mut stack: Vec<uiautomation::UIElement> = vec![root];
    let mut visited = 0_usize;

    while let Some(node) = stack.pop() {
        visited += 1;
        if visited > MAX_NODES {
            eprintln!("[youtube_bridge] walk hit MAX_NODES={MAX_NODES}");
            break;
        }

        if let Ok(name) = node.get_name() {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                // Check for timer shape first (M:SS or M:SS / M:SS).
                if timer.is_none() && timer_re.is_match(trimmed) {
                    timer = Some(trimmed.to_string());
                }
                // Cap the text collection to 200 entries — plenty for ad-marker
                // matching while keeping a lid on YouTube's verbose tree.
                if texts.len() < 200 {
                    texts.push(trimmed.to_string());
                }
            }
        }

        // Enqueue children in reverse for left-to-right DFS — same pattern
        // as pandora_desktop.rs::collect_pandora_uia_data.
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

    Ok((texts, timer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_markers_not_ad() {
        let texts = vec!["Some other text".to_string(), "Music video".to_string()];
        let r = classify_youtube_state(&texts, None);
        assert!(!r.is_ad);
    }

    #[test]
    fn parse_artist_dash_song_with_lyrics_tag() {
        // The motivating case: 7clouds lyric video.
        let (artist, title) = parse_youtube_metadata("Fleetwood Mac - Dreams (Lyrics)", "7clouds");
        assert_eq!(artist, "Fleetwood Mac");
        assert_eq!(title, "Dreams");
    }

    #[test]
    fn parse_channel_artist_discarded() {
        let (artist, title) =
            parse_youtube_metadata("Kelly Clarkson - Since U Been Gone", "RockHype");
        assert_eq!(artist, "Kelly Clarkson");
        assert_eq!(title, "Since U Been Gone");
    }

    #[test]
    fn parse_strips_official_video_and_feat() {
        let (artist, title) =
            parse_youtube_metadata("T-Pain - Bartender (Official HD Video) ft. Akon", "T Pain");
        assert_eq!(artist, "T-Pain");
        assert_eq!(title, "Bartender");
    }

    #[test]
    fn parse_no_dash_keeps_channel_and_strips_topic() {
        // YouTube auto-generated "Topic" channel: the channel IS the artist.
        let (artist, title) = parse_youtube_metadata("Dreams", "Fleetwood Mac - Topic");
        assert_eq!(artist, "Fleetwood Mac");
        assert_eq!(title, "Dreams");
    }

    #[test]
    fn parse_no_dash_plain_song_keeps_channel() {
        let (artist, title) = parse_youtube_metadata("Some Song", "SomeChannel");
        assert_eq!(artist, "SomeChannel");
        assert_eq!(title, "Some Song");
    }

    #[test]
    fn parse_degenerate_split_not_taken() {
        // Single-char prefix must not be treated as an artist.
        let (_artist, title) = parse_youtube_metadata("A - B", "Chan");
        assert_eq!(title, "A - B");
    }

    #[test]
    fn youtube_does_not_short_circuit_lyrics() {
        // YouTube's SMTC title is a real song — a stale bridge must NOT
        // mark it unreliable (which would emit `unsupported-source` and
        // skip the resolver). This guards the v0.13.43 fix.
        use crate::web_bridge::WebPlayerProbe;
        assert!(!YouTubeProbe.smtc_unreliable_without_bridge());
    }

    #[test]
    fn sponsored_text_is_ad() {
        let texts = vec!["Sponsored".to_string()];
        let r = classify_youtube_state(&texts, None);
        assert!(r.is_ad);
    }

    #[test]
    fn skip_ad_text_is_ad() {
        let texts = vec!["Skip Ad in 3".to_string()];
        let r = classify_youtube_state(&texts, None);
        assert!(r.is_ad);
    }

    #[test]
    fn ad_bullet_text_is_ad() {
        let texts = vec!["Ad · 0:30".to_string()];
        let r = classify_youtube_state(&texts, None);
        assert!(r.is_ad);
    }

    #[test]
    fn timer_parses_both_sides() {
        let texts = vec!["Sponsored".to_string()];
        let r = classify_youtube_state(&texts, Some("0:05 / 0:30"));
        assert!(r.is_ad);
        assert_eq!(r.position_ms, Some(5_000));
        assert_eq!(r.duration_ms, Some(30_000));
    }

    #[test]
    fn timer_with_only_duration() {
        let texts = vec!["Advertisement".to_string()];
        let r = classify_youtube_state(&texts, Some("0:15"));
        assert!(r.is_ad);
        assert_eq!(r.position_ms, None);
        assert_eq!(r.duration_ms, Some(15_000));
    }
}
