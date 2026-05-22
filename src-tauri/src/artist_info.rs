//! Artist-info fetch chain: Last.fm bio + similar, Bandsintown events,
//! TheAudioDB photo, MusicBrainz mbid fallback. Disk cache + in-flight dedup.

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtistInfo {
    pub name: String,
    pub slug: String,
    pub bio: Option<ArtistBio>,
    pub photo_data_url: Option<String>,
    pub similar_artists: Vec<String>,
    pub tour_dates: Vec<TourDate>,
    pub mbid: Option<String>,
    pub fetched_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtistBio {
    pub text: String,
    pub lastfm_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TourDate {
    pub date_unix_ms: i64,
    pub city: String,
    pub region: String,
    pub country: String,
    pub venue: String,
    pub ticket_url: Option<String>,
    pub status: TicketStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Available,
    SoldOut,
}

// ── Pure helpers ───────────────────────────────────────────────────────────

/// Derive a URL-safe slug from an artist name.
/// Lowercase, collapse whitespace to single dash, strip everything else
/// except ASCII alphanumerics and dashes. Common Latin diacritics mapped
/// to ASCII before stripping.
#[allow(dead_code)]
pub(crate) fn slug_for_artist(name: &str) -> String {
    // Diacritic → ASCII mapping for common Latin Extended chars.
    let mapped: String = name.chars().map(|c| match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' | 'Ÿ' => 'y',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        'ß' => 's',
        'æ' | 'Æ' => 'a',
        'œ' | 'Œ' => 'o',
        'ð' | 'Ð' => 'd',
        'þ' | 'Þ' => 't',
        other => other,
    }).collect();

    let lower = mapped.to_lowercase();

    // Replace whitespace runs with a single dash, keep alphanumerics.
    let mut result = String::new();
    let mut last_was_dash = false;
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
            last_was_dash = false;
        } else if (c.is_whitespace() || c == '-') && !last_was_dash && !result.is_empty() {
            result.push('-');
            last_was_dash = true;
        }
        // All other chars (punctuation, non-ASCII) are dropped.
    }
    // Trim trailing dash.
    result.trim_end_matches('-').to_string()
}

/// True if the tour-dates entry is stale (older than 12 hours).
#[allow(dead_code)]
pub(crate) fn tour_dates_stale(fetched_at_unix_ms: i64, now_unix_ms: i64) -> bool {
    const TWELVE_HOURS_MS: i64 = 12 * 3600 * 1000;
    (now_unix_ms - fetched_at_unix_ms) >= TWELVE_HOURS_MS
}

/// Strip HTML tags from a string, keeping tag content.
/// Handles `<a href="...">text</a>`, `<b>text</b>`, etc.
#[allow(dead_code)]
pub(crate) fn strip_html(s: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("strip_html regex"));
    // Decode common HTML entities after stripping tags.
    let stripped = re.replace_all(s, "");
    stripped
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Stub — filled in by Task 6.
#[allow(dead_code)]
pub async fn fetch_artist_info(_artist: &str) -> Result<ArtistInfo> {
    todo!("implemented in Task 6")
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_simple() {
        assert_eq!(slug_for_artist("Shaggy"), "shaggy");
    }

    #[test]
    fn slug_diacritics() {
        assert_eq!(slug_for_artist("Mötley Crüe"), "motley-crue");
    }

    #[test]
    fn slug_punctuation_stripped() {
        assert_eq!(slug_for_artist("AC/DC"), "acdc");
    }

    #[test]
    fn slug_punctuation_pnk() {
        assert_eq!(slug_for_artist("  P!nk  "), "pnk");
    }

    #[test]
    fn slug_empty() {
        assert_eq!(slug_for_artist(""), "");
    }

    #[test]
    fn slug_multi_word() {
        assert_eq!(slug_for_artist("The Rolling Stones"), "the-rolling-stones");
    }

    #[test]
    fn slug_leading_trailing_dash() {
        // Leading/trailing non-alphanum should not produce leading/trailing dash.
        assert_eq!(slug_for_artist("---test---"), "test");
    }

    #[test]
    fn tour_dates_fresh() {
        // 0 hours ago — not stale.
        assert!(!tour_dates_stale(1_000_000, 1_000_000));
    }

    #[test]
    fn tour_dates_eleven_hours() {
        let now = 1_000_000_000i64;
        let fetched = now - (11 * 3600 * 1000);
        assert!(!tour_dates_stale(fetched, now));
    }

    #[test]
    fn tour_dates_thirteen_hours() {
        let now = 1_000_000_000i64;
        let fetched = now - (13 * 3600 * 1000);
        assert!(tour_dates_stale(fetched, now));
    }

    #[test]
    fn tour_dates_exactly_twelve_hours() {
        let now = 1_000_000_000i64;
        let fetched = now - (12 * 3600 * 1000);
        // Exactly at the boundary → stale (>=).
        assert!(tour_dates_stale(fetched, now));
    }

    #[test]
    fn strip_html_plain() {
        assert_eq!(strip_html("plain"), "plain");
    }

    #[test]
    fn strip_html_anchor() {
        assert_eq!(strip_html("<a href='x'>link</a>"), "link");
    }

    #[test]
    fn strip_html_bold() {
        assert_eq!(strip_html("text with <b>bold</b>"), "text with bold");
    }

    #[test]
    fn strip_html_entities() {
        assert_eq!(strip_html("rock &amp; roll"), "rock & roll");
    }

    #[test]
    fn strip_html_empty() {
        assert_eq!(strip_html(""), "");
    }
}
