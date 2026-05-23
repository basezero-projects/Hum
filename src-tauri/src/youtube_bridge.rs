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
pub(crate) fn classify_youtube_state(
    texts: &[String],
    timer_text: Option<&str>,
) -> YouTubeAdState {
    let markers = ["Sponsored", "Ad ·", "Advertisement", "Skip Ad", "Skip in"];
    let is_ad = texts.iter().any(|t| {
        markers.iter().any(|m| t.contains(m))
    });

    if !is_ad {
        return YouTubeAdState { is_ad: false, position_ms: None, duration_ms: None };
    }

    let (position_ms, duration_ms) = timer_text.map(parse_youtube_timer).unwrap_or((None, None));
    YouTubeAdState { is_ad, position_ms, duration_ms }
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

    fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>> {
        // 1. Find a Chrome window whose title contains "YouTube".
        // 2. Re-anchor through element_from_handle to wake the accessibility tree.
        // 3. DFS for Text nodes that contain ad markers; also capture timer-shaped text.
        // 4. Classify via classify_youtube_state; if is_ad, return ad WebBridgeTrack.
        // 5. Return Ok(None) for non-ad state — normal YouTube SMTC metadata stays.

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

        // Not an ad — return None so we don't override SMTC's normal
        // YouTube metadata with empty strings.
        Ok(None)
    }
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

    let automation = UIAutomation::new()
        .map_err(|e| anyhow!("UIAutomation::new failed: {e:?}"))?;
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
