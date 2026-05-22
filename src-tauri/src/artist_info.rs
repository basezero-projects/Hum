//! Artist-info fetch chain: Last.fm bio + similar, Bandsintown events,
//! TheAudioDB photo, MusicBrainz mbid fallback. Disk cache + in-flight dedup.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, Notify};

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

/// Top-level entry point for callers that don't hold an ArtistInfoCache.
/// Prefer ArtistInfoCache::fetch which adds caching + dedup.
#[allow(dead_code)]
pub async fn fetch_artist_info(artist: &str) -> Result<ArtistInfo> {
    let client = build_artist_info_http_client()?;
    let now = now_unix_ms();
    let (bio, similar, events, photo) = tokio::join!(
        fetch_lastfm_bio(&client, artist),
        fetch_lastfm_similar(&client, artist),
        fetch_bandsintown_events(&client, artist),
        fetch_theaudiodb_photo(&client, artist),
    );
    Ok(ArtistInfo {
        name: artist.to_string(),
        slug: slug_for_artist(artist),
        bio,
        photo_data_url: photo,
        similar_artists: similar,
        tour_dates: events,
        mbid: None,
        fetched_at_unix_ms: now,
    })
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

// ── TheAudioDB ─────────────────────────────────────────────────────────────

/// TheAudioDB free tier uses the public test key "2".
/// Documented at https://www.theaudiodb.com/api_guide.php
#[allow(dead_code)]
const THEAUDIODB_BASE: &str = "https://www.theaudiodb.com/api/v1/json/2/search.php";

/// Fetch artist thumbnail from TheAudioDB and return as `data:image/jpeg;base64,...`.
/// Returns None on any failure.
#[allow(dead_code)]
pub(crate) async fn fetch_theaudiodb_photo(
    client: &reqwest::Client,
    artist: &str,
) -> Option<String> {
    use base64::Engine;

    let url = reqwest::Url::parse_with_params(THEAUDIODB_BASE, &[("s", artist)]).ok()?;

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[artist_info] theaudiodb search failed: {e}");
            return None;
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[artist_info] theaudiodb JSON parse failed: {e}");
            return None;
        }
    };

    let thumb_url = body
        .get("artists")?
        .as_array()?
        .first()?
        .get("strArtistThumb")?
        .as_str()?;

    if thumb_url.is_empty() {
        return None;
    }

    let bytes = match client.get(thumb_url).send().await {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[artist_info] theaudiodb image bytes failed: {e}");
                return None;
            }
        },
        Err(e) => {
            eprintln!("[artist_info] theaudiodb image fetch failed: {e}");
            return None;
        }
    };

    if bytes.is_empty() {
        return None;
    }

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:image/jpeg;base64,{b64}"))
}

// ── MusicBrainz ────────────────────────────────────────────────────────────

/// Resolve a MusicBrainz artist ID (mbid) by name.
/// Only invoked on the Last.fm fallback path (error 6 + Bandsintown hit).
/// MusicBrainz TOS requires a User-Agent with contact info; the HTTP client
/// built in Task 6 sets that header. Returns None on any failure.
#[allow(dead_code)]
pub(crate) async fn resolve_mbid_musicbrainz(
    client: &reqwest::Client,
    artist: &str,
) -> Option<String> {
    let query = format!("artist:{}", urlencoding::encode(artist));
    let url = reqwest::Url::parse_with_params(
        "https://musicbrainz.org/ws/2/artist",
        &[("query", query.as_str()), ("limit", "1"), ("fmt", "json")],
    )
    .ok()?;

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[artist_info] musicbrainz fetch failed: {e}");
            return None;
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[artist_info] musicbrainz JSON parse failed: {e}");
            return None;
        }
    };

    body.get("artists")?
        .as_array()?
        .first()?
        .get("id")?
        .as_str()
        .map(|s| s.to_string())
}

// ── HTTP client ────────────────────────────────────────────────────────────

/// Build a reqwest::Client with the User-Agent required by MusicBrainz TOS
/// and a 10s timeout covering all artist-info requests.
pub(crate) fn build_artist_info_http_client() -> Result<reqwest::Client> {
    let client = reqwest::Client::builder()
        .user_agent("hum/0.11.0 (https://github.com/basezero-projects/Hum; itswesl3y@gmail.com)")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    Ok(client)
}

// ── Disk cache ─────────────────────────────────────────────────────────────

/// On-disk structure per artist. Fields are individually timestamped so
/// tour-dates can be refreshed without blowing away the bio/photo.
#[allow(dead_code)]
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct CachedArtistData {
    version: u32,
    name: Option<String>,
    slug: Option<String>,
    bio: Option<ArtistBio>,
    bio_fetched_at_unix_ms: Option<i64>,
    photo_data_url: Option<String>,
    photo_fetched_at_unix_ms: Option<i64>,
    similar_artists: Option<Vec<String>>,
    similar_fetched_at_unix_ms: Option<i64>,
    tour_dates: Option<Vec<TourDate>>,
    tour_dates_fetched_at_unix_ms: Option<i64>,
    mbid: Option<String>,
}

#[allow(dead_code)]
fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[allow(dead_code)]
fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("cache").join("artist"))
}

#[allow(dead_code)]
fn cache_file_path(app: &AppHandle, slug: &str) -> Option<PathBuf> {
    cache_dir(app).map(|d| d.join(format!("{slug}.json")))
}

#[allow(dead_code)]
async fn read_cache_file(app: &AppHandle, slug: &str) -> Option<CachedArtistData> {
    let path = cache_file_path(app, slug)?;
    let bytes = tokio::fs::read(&path).await.ok()?;
    let data: CachedArtistData = match serde_json::from_slice(&bytes) {
        Ok(d) => d,
        Err(e) => {
            // Corrupted cache file. Per spec: delete the file, treat as a
            // miss, log so it shows up if it ever happens in real-world use.
            eprintln!(
                "[artist_info] cache file {:?} corrupted ({}); deleting",
                path, e
            );
            let _ = tokio::fs::remove_file(&path).await;
            return None;
        }
    };
    if data.version != 1 {
        // Version mismatch — delete and treat as miss so the new version's
        // shape lands on next fetch.
        let _ = tokio::fs::remove_file(&path).await;
        return None;
    }
    Some(data)
}

#[allow(dead_code)]
async fn write_cache_file(app: &AppHandle, data: &CachedArtistData) -> Result<()> {
    let slug = data.slug.as_deref().unwrap_or("unknown");
    let path = cache_file_path(app, slug)
        .ok_or_else(|| anyhow::anyhow!("could not resolve cache path"))?;
    // Ensure directory exists.
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_vec_pretty(data)?;
    tokio::fs::write(&path, &json).await?;
    Ok(())
}

// ── In-flight dedup + managed state ────────────────────────────────────────

/// Tauri managed state for the artist-info fetch chain.
/// `in_flight` maps artist slug → a `Notify` that fires when the pending
/// fetch for that slug completes. A second caller for the same slug waits
/// on the Notify instead of firing a duplicate request.
#[allow(dead_code)]
pub struct ArtistInfoCache {
    in_flight: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    app: AppHandle,
}

#[allow(dead_code)]
impl ArtistInfoCache {
    pub fn new(app: AppHandle) -> Self {
        Self {
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            app,
        }
    }

    /// Fetch artist info. Returns cached data immediately when fresh.
    /// Re-fetches only tour dates when they are stale (≥12h).
    /// De-duplicates concurrent requests for the same slug via Notify.
    pub async fn fetch(&self, artist: &str) -> Result<ArtistInfo> {
        let slug = slug_for_artist(artist);
        if slug.is_empty() {
            return Err(anyhow::anyhow!("artist name produced empty slug"));
        }

        // Check if another task is already fetching this slug.
        let notify = {
            let mut map = self.in_flight.lock().await;
            if let Some(existing) = map.get(&slug) {
                let notify = existing.clone();
                drop(map);
                // Wait for the other fetch to complete, then read from cache.
                notify.notified().await;
                // Fall through to cache read below.
                None
            } else {
                let notify = Arc::new(Notify::new());
                map.insert(slug.clone(), notify.clone());
                Some(notify)
            }
        };

        // Always read cache after acquiring or waiting.
        let now = now_unix_ms();
        if let Some(cached) = read_cache_file(&self.app, &slug).await {
            let tour_stale = cached
                .tour_dates_fetched_at_unix_ms
                .map(|t| tour_dates_stale(t, now))
                .unwrap_or(true);

            if notify.is_none() {
                // We waited on another fetch; return the cache result.
                return build_artist_info_from_cache(&cached, artist, &slug);
            }

            if !tour_stale {
                // Fully fresh — release the in-flight slot and return.
                let notify = notify.unwrap();
                {
                    let mut map = self.in_flight.lock().await;
                    map.remove(&slug);
                }
                notify.notify_waiters();
                return build_artist_info_from_cache(&cached, artist, &slug);
            }

            // Tour dates stale — refetch only events; keep everything else.
            let client = build_artist_info_http_client()?;
            let new_events = fetch_bandsintown_events(&client, artist).await;
            let mut updated = cached.clone();
            updated.tour_dates = Some(new_events);
            updated.tour_dates_fetched_at_unix_ms = Some(now);
            let _ = write_cache_file(&self.app, &updated).await;
            let notify = notify.unwrap();
            {
                let mut map = self.in_flight.lock().await;
                map.remove(&slug);
            }
            notify.notify_waiters();
            return build_artist_info_from_cache(&updated, artist, &slug);
        }

        // Cache miss — full fetch.
        // Guard: if notify is None we were a waiter, not the original fetcher.
        // The original fetch already ran but its cache write must have failed
        // (disk full, permission error, etc.). Return an error so the panel
        // shows the "Couldn't load artist info / Retry" state rather than
        // panicking on the unwrap below.
        let Some(notify) = notify else {
            return Err(anyhow::anyhow!("upstream fetch failed; cache empty after wait"));
        };
        let client = build_artist_info_http_client()?;

        // Parallel fetch: bio, similar, events, photo.
        let (bio_result, similar_result, events_result, photo_result) = tokio::join!(
            fetch_lastfm_bio(&client, artist),
            fetch_lastfm_similar(&client, artist),
            fetch_bandsintown_events(&client, artist),
            fetch_theaudiodb_photo(&client, artist),
        );

        // MusicBrainz fallback: only if Last.fm bio failed AND Bandsintown
        // returned something (so we know the artist actually exists).
        let (final_bio, mbid) = if bio_result.is_none() && !events_result.is_empty() {
            let mbid_opt = resolve_mbid_musicbrainz(&client, artist).await;
            if let Some(ref mbid_str) = mbid_opt {
                // Retry Last.fm with the mbid.
                let bio_retry = fetch_lastfm_bio_by_mbid(&client, mbid_str).await;
                (bio_retry, mbid_opt)
            } else {
                (None, None)
            }
        } else {
            (bio_result, None)
        };

        let data = CachedArtistData {
            version: 1,
            name: Some(artist.to_string()),
            slug: Some(slug.clone()),
            bio: final_bio,
            bio_fetched_at_unix_ms: Some(now),
            photo_data_url: photo_result,
            photo_fetched_at_unix_ms: Some(now),
            similar_artists: Some(similar_result),
            similar_fetched_at_unix_ms: Some(now),
            tour_dates: Some(events_result),
            tour_dates_fetched_at_unix_ms: Some(now),
            mbid,
        };

        let _ = write_cache_file(&self.app, &data).await;

        {
            let mut map = self.in_flight.lock().await;
            map.remove(&slug);
        }
        notify.notify_waiters();

        build_artist_info_from_cache(&data, artist, &slug)
    }

    /// Wipe the entire artist cache directory.
    pub async fn clear(&self) -> Result<()> {
        if let Some(dir) = cache_dir(&self.app) {
            if dir.exists() {
                tokio::fs::remove_dir_all(&dir).await?;
            }
        }
        Ok(())
    }
}

/// Retry Last.fm artist.getInfo using an mbid instead of artist name.
#[allow(dead_code)]
async fn fetch_lastfm_bio_by_mbid(
    client: &reqwest::Client,
    mbid: &str,
) -> Option<ArtistBio> {
    let url = reqwest::Url::parse_with_params(
        LASTFM_BASE,
        &[
            ("method", "artist.getInfo"),
            ("mbid", mbid),
            ("api_key", LASTFM_API_KEY),
            ("format", "json"),
        ],
    )
    .ok()?;
    let resp = client.get(url).send().await.ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    if body.get("error").is_some() {
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
    if lastfm_url.is_empty() { return None; }
    let mut bio_text = strip_html(&raw_bio);
    if bio_text.len() > 1500 {
        let cutoff = bio_text[..1500].rfind(['.', '?', '!']).map(|i| i + 1).unwrap_or(1500);
        bio_text.truncate(cutoff);
        bio_text = bio_text.trim_end().to_string();
    }
    if bio_text.is_empty() { return None; }
    Some(ArtistBio { text: bio_text, lastfm_url })
}

#[allow(dead_code)]
fn build_artist_info_from_cache(
    data: &CachedArtistData,
    artist: &str,
    slug: &str,
) -> Result<ArtistInfo> {
    Ok(ArtistInfo {
        name: data.name.clone().unwrap_or_else(|| artist.to_string()),
        slug: data.slug.clone().unwrap_or_else(|| slug.to_string()),
        bio: data.bio.clone(),
        photo_data_url: data.photo_data_url.clone(),
        similar_artists: data.similar_artists.clone().unwrap_or_default(),
        tour_dates: data.tour_dates.clone().unwrap_or_default(),
        mbid: data.mbid.clone(),
        fetched_at_unix_ms: data
            .bio_fetched_at_unix_ms
            .or(data.tour_dates_fetched_at_unix_ms)
            .unwrap_or(0),
    })
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_artist_info(
    artist: String,
    cache: tauri::State<'_, ArtistInfoCache>,
) -> Result<ArtistInfo, String> {
    cache.fetch(&artist).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_artist_info_cache(
    cache: tauri::State<'_, ArtistInfoCache>,
) -> Result<(), String> {
    cache.clear().await.map_err(|e| e.to_string())
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
