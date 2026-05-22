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

// ── Last.fm ────────────────────────────────────────────────────────────────

/// Register a free API account at https://www.last.fm/api before public release.
/// Embedding a static key is the documented intended use (rate-limit identifier,
/// not an auth secret).
#[allow(dead_code)]
const LASTFM_API_KEY: &str = "PLACEHOLDER_REPLACE_BEFORE_LAUNCH";
#[allow(dead_code)]
const LASTFM_BASE: &str = "https://ws.audioscrobbler.com/2.0/";

/// Fetch artist bio from Last.fm artist.getInfo.
/// Returns None on artist-not-found (error 6), network failure, or missing fields.
#[allow(dead_code)]
pub(crate) async fn fetch_lastfm_bio(
    client: &reqwest::Client,
    artist: &str,
) -> Option<ArtistBio> {
    let url = reqwest::Url::parse_with_params(
        LASTFM_BASE,
        &[
            ("method", "artist.getInfo"),
            ("artist", artist),
            ("api_key", LASTFM_API_KEY),
            ("format", "json"),
        ],
    )
    .ok()?;

    let resp = client.get(url).send().await.ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;

    // Error 6 = artist not found; error 26 = suspended key. Both → None.
    if body.get("error").is_some() {
        eprintln!(
            "[artist_info] lastfm getInfo error: {}",
            body.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
        );
        return None;
    }

    let artist_obj = body.get("artist")?;
    let raw_bio = artist_obj
        .get("bio")?
        .get("summary")?
        .as_str()
        .unwrap_or("")
        .to_string();
    let lastfm_url = artist_obj
        .get("url")?
        .as_str()
        .unwrap_or("")
        .to_string();

    if lastfm_url.is_empty() {
        return None;
    }

    let mut bio_text = strip_html(&raw_bio);

    // Truncate to last sentence boundary before 1500 chars.
    if bio_text.len() > 1500 {
        // Find last period, ?, or ! before position 1500.
        let cutoff = bio_text[..1500]
            .rfind(['.', '?', '!'])
            .map(|i| i + 1)
            .unwrap_or(1500);
        bio_text.truncate(cutoff);
        bio_text = bio_text.trim_end().to_string();
    }

    if bio_text.is_empty() {
        return None;
    }

    Some(ArtistBio { text: bio_text, lastfm_url })
}

/// Fetch similar artists from Last.fm artist.getSimilar (top 8).
/// Returns an empty Vec on any failure.
#[allow(dead_code)]
pub(crate) async fn fetch_lastfm_similar(
    client: &reqwest::Client,
    artist: &str,
) -> Vec<String> {
    let url = match reqwest::Url::parse_with_params(
        LASTFM_BASE,
        &[
            ("method", "artist.getSimilar"),
            ("artist", artist),
            ("api_key", LASTFM_API_KEY),
            ("limit", "8"),
            ("format", "json"),
        ],
    ) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(_) => return vec![],
    };

    if body.get("error").is_some() {
        return vec![];
    }

    body.get("similarartists")
        .and_then(|sa| sa.get("artist"))
        .and_then(|arr| arr.as_array())
        .map(|artists| {
            artists
                .iter()
                .filter_map(|a| a.get("name")?.as_str().map(|s| s.to_string()))
                .take(8)
                .collect()
        })
        .unwrap_or_default()
}

// ── Bandsintown ────────────────────────────────────────────────────────────

/// Placeholder. Wes registers at https://bandsintown.com/partners before
/// public release and replaces this with the live partner app_id.
#[allow(dead_code)]
const BANDSINTOWN_APP_ID: &str = "hum-dev";

/// Fetch upcoming events from Bandsintown.
/// Returns events sorted by date ascending. Returns empty Vec on any failure.
#[allow(dead_code)]
pub(crate) async fn fetch_bandsintown_events(
    client: &reqwest::Client,
    artist: &str,
) -> Vec<TourDate> {
    // Artist name goes in the URL path, not as a query param.
    let encoded = urlencoding::encode(artist);
    let url_str = format!(
        "https://rest.bandsintown.com/artists/{}/events?app_id={}",
        encoded, BANDSINTOWN_APP_ID
    );
    let url = match reqwest::Url::parse(&url_str) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[artist_info] bandsintown fetch failed: {e}");
            return vec![];
        }
    };

    let events: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[artist_info] bandsintown JSON parse failed: {e}");
            return vec![];
        }
    };

    let arr = match events.as_array() {
        Some(a) => a,
        None => return vec![],
    };

    let mut dates: Vec<TourDate> = arr
        .iter()
        .filter_map(parse_bandsintown_event)
        .collect();

    // Sort ascending by date.
    dates.sort_by_key(|d| d.date_unix_ms);
    dates
}

fn parse_bandsintown_event(event: &serde_json::Value) -> Option<TourDate> {
    // datetime: ISO8601, e.g. "2026-03-05T20:00:00" (no timezone offset).
    let datetime_str = event.get("datetime")?.as_str()?;
    let date_unix_ms = parse_iso8601_to_unix_ms(datetime_str)?;

    let venue = event.get("venue")?;
    let city = venue
        .get("city")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let region = venue
        .get("region")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let country = venue
        .get("country")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let venue_name = venue
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Find the first "Tickets" offer.
    let mut ticket_url: Option<String> = None;
    let mut status = TicketStatus::Available;
    if let Some(offers) = event.get("offers").and_then(|o| o.as_array()) {
        for offer in offers {
            let offer_type = offer.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if offer_type.eq_ignore_ascii_case("Tickets") {
                if let Some(url) = offer.get("url").and_then(|u| u.as_str()) {
                    if !url.is_empty() {
                        ticket_url = Some(url.to_string());
                    }
                }
                let offer_status = offer
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("available");
                status = if offer_status.eq_ignore_ascii_case("available") {
                    TicketStatus::Available
                } else {
                    TicketStatus::SoldOut
                };
                break;
            }
        }
    }

    Some(TourDate {
        date_unix_ms,
        city,
        region,
        country,
        venue: venue_name,
        ticket_url,
        status,
    })
}

/// Parse Bandsintown ISO8601 datetime string to Unix milliseconds.
/// Input format: "2026-03-05T20:00:00" (no timezone; treat as UTC for sorting purposes).
fn parse_iso8601_to_unix_ms(s: &str) -> Option<i64> {
    // Split at 'T' and parse manually: "2026-03-05" and "20:00:00".
    let (date_part, time_part) = s.split_once('T')?;
    let mut date_parts = date_part.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time_part.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let min: i64 = time_parts.next()?.parse().ok()?;
    let sec_str = time_parts.next().unwrap_or("0");
    let sec: i64 = sec_str.split('.').next()?.parse().ok()?;

    // Days from epoch (1970-01-01). Use the proleptic Gregorian formula.
    // This is accurate for dates in the 2020s–2030s range we actually see.
    let days = days_from_epoch(year, month, day)?;
    let secs = days * 86400 + hour * 3600 + min * 60 + sec;
    Some(secs * 1000)
}

fn days_from_epoch(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days in each month (non-leap).
    let days_in_month = [0i64, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mut days: i64 = (year - 1970) * 365 + (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400;
    for m in 1..month {
        days += days_in_month[m as usize];
        if m == 2 && is_leap { days += 1; }
    }
    days += day - 1;
    Some(days)
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
