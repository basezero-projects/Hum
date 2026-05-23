//! Artist-info fetch chain: Wikipedia bio, Ticketmaster events,
//! TheAudioDB photo. Disk cache + in-flight dedup.

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
    pub tour_dates: Vec<TourDate>,
    pub fetched_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtistBio {
    pub text: String,
    pub wikipedia_url: String,
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
pub(crate) fn tour_dates_stale(fetched_at_unix_ms: i64, now_unix_ms: i64) -> bool {
    const TWELVE_HOURS_MS: i64 = 12 * 3600 * 1000;
    (now_unix_ms - fetched_at_unix_ms) >= TWELVE_HOURS_MS
}

/// Top-level entry point for callers that don't hold an ArtistInfoCache.
/// Prefer ArtistInfoCache::fetch which adds caching + dedup.
#[allow(dead_code)]
pub async fn fetch_artist_info(artist: &str) -> Result<ArtistInfo> {
    let client = build_artist_info_http_client()?;
    let now = now_unix_ms();
    let (bio, events, photo) = tokio::join!(
        fetch_wikipedia_bio(&client, artist),
        fetch_ticketmaster_events(&client, artist),
        fetch_theaudiodb_photo(&client, artist),
    );
    Ok(ArtistInfo {
        name: artist.to_string(),
        slug: slug_for_artist(artist),
        bio,
        photo_data_url: photo,
        tour_dates: events,
        fetched_at_unix_ms: now,
    })
}

// ── Wikipedia ─────────────────────────────────────────────────────────────

/// Music-relevance keywords used to gate Wikipedia results.
/// The description field (e.g. "American rapper", "English rock band") must
/// contain at least one of these substrings (case-insensitive) for the page
/// to be accepted as a music artist bio.
const MUSIC_KEYWORDS: &[&str] = &[
    "musician", "singer", "rapper", "songwriter", "band", "group",
    "dj", "producer", "composer", "musical", "music", "vocalist",
    "guitarist", "drummer", "bassist", "pianist",
    "rock", "pop", "hip hop", "hip-hop", "country", "jazz", "metal",
    "indie", "electronic", "r&b", "soul", "folk",
];

/// Disambiguator suffixes tried in order when the direct lookup fails the
/// music-relevance gate or returns a non-standard page type.
const WIKIPEDIA_SUFFIXES: &[&str] = &[
    "musician", "singer", "rapper", "band", "rock band", "group",
];

/// Truncate bio text to the last sentence boundary before 1500 chars.
/// Mirrors the existing Last.fm truncation logic.
fn truncate_bio(mut text: String) -> String {
    if text.len() > 1500 {
        let cutoff = text[..1500]
            .rfind(['.', '?', '!'])
            .map(|i| i + 1)
            .unwrap_or(1500);
        text.truncate(cutoff);
        text = text.trim_end().to_string();
        if text.is_empty() {
            // No sentence boundary found — hard truncate with ellipsis.
            text = text[..1500.min(text.len())].to_string();
            text.push('…');
        }
    }
    text
}

/// Return true if the description passes the music-relevance gate.
fn is_music_relevant(description: &str) -> bool {
    let lower = description.to_lowercase();
    MUSIC_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Parse a Wikipedia REST summary JSON body into an `ArtistBio`.
/// Returns `Some` only when type == "standard", extract is non-empty,
/// and the description passes the music-relevance gate.
fn parse_wikipedia_summary(body: &serde_json::Value) -> Option<ArtistBio> {
    let page_type = body.get("type")?.as_str()?;
    if page_type != "standard" {
        return None;
    }
    let extract = body.get("extract")?.as_str()?;
    if extract.is_empty() {
        return None;
    }
    let description = body
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("");
    if !is_music_relevant(description) {
        return None;
    }
    let wikipedia_url = body
        .get("content_urls")
        .and_then(|u| u.get("desktop"))
        .and_then(|d| d.get("page"))
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    if wikipedia_url.is_empty() {
        return None;
    }
    let text = truncate_bio(extract.to_string());
    if text.is_empty() {
        return None;
    }
    Some(ArtistBio { text, wikipedia_url })
}

/// Fetch artist bio from the Wikipedia REST API.
///
/// 1. Tries a direct lookup by artist name.
/// 2. If that fails the music-relevance gate (or returns a non-standard page),
///    retries with disambiguation suffixes: (musician), (singer), (rapper),
///    (band), (rock band), (group).
/// 3. Returns `None` if all attempts fail.
pub(crate) async fn fetch_wikipedia_bio(
    client: &reqwest::Client,
    artist: &str,
) -> Option<ArtistBio> {
    // Direct lookup.
    let encoded = urlencoding::encode(artist);
    let url = format!(
        "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
        encoded
    );
    if let Ok(resp) = client.get(&url).send().await {
        if let Ok(body) = resp.json::<serde_json::Value>().await {
            if let Some(bio) = parse_wikipedia_summary(&body) {
                return Some(bio);
            }
        }
    }

    // Disambiguator suffix fallback.
    for suffix in WIKIPEDIA_SUFFIXES {
        let title = format!("{} ({})", artist, suffix);
        let encoded_title = urlencoding::encode(&title);
        let suffix_url = format!(
            "https://en.wikipedia.org/api/rest_v1/page/summary/{}",
            encoded_title
        );
        if let Ok(resp) = client.get(&suffix_url).send().await {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(bio) = parse_wikipedia_summary(&body) {
                    return Some(bio);
                }
            }
        }
    }

    None
}

// ── Ticketmaster Discovery ─────────────────────────────────────────────────

/// Ticketmaster Discovery API consumer key (SYVR-App, approved 2026-05-22).
/// Free tier: 5 req/sec, 5K req/day. Rate-limit identifier, not an auth secret —
/// embedded in the binary per Ticketmaster's documented intended use.
const TICKETMASTER_API_KEY: &str = "GQbGNt5UBoE0RdMMCDB9IAplTcjEeA6A";
const TICKETMASTER_DISCOVERY_BASE: &str =
    "https://app.ticketmaster.com/discovery/v2/events.json";

/// Impact (impact.com) affiliate URL prefix template. Wes signs up at
/// https://impact.com, joins the Ticketmaster brand, and gets a tracking
/// link template. Until set, ticket URLs route through Ticketmaster
/// directly without affiliate credit. Format expected:
/// `https://{subdomain}.go.impact.com/c/{publisher-id}/{campaign-id}/`
/// then append the URL-encoded target.
const IMPACT_AFFILIATE_PREFIX: Option<&str> = None;

fn wrap_with_impact_affiliate(url: &str) -> String {
    match IMPACT_AFFILIATE_PREFIX {
        Some(prefix) => format!("{}{}", prefix, urlencoding::encode(url)),
        None => url.to_string(),
    }
}

/// Fetch upcoming events for an artist from Ticketmaster Discovery API.
/// Returns events sorted by date ascending, validated against the requested
/// artist name (case-insensitive primary-attraction match). Empty Vec on
/// any failure or no-match.
pub(crate) async fn fetch_ticketmaster_events(
    client: &reqwest::Client,
    artist: &str,
) -> Vec<TourDate> {
    let url = match reqwest::Url::parse_with_params(
        TICKETMASTER_DISCOVERY_BASE,
        &[
            ("apikey", TICKETMASTER_API_KEY),
            ("keyword", artist),
            ("classificationName", "music"),
            ("size", "50"),
            ("sort", "date,asc"),
        ],
    ) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[artist_info] ticketmaster fetch failed: {e}");
            return vec![];
        }
    };

    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[artist_info] ticketmaster JSON parse failed: {e}");
            return vec![];
        }
    };

    let events = match body
        .get("_embedded")
        .and_then(|e| e.get("events"))
        .and_then(|e| e.as_array())
    {
        Some(arr) => arr,
        None => return vec![],
    };

    let mut dates: Vec<TourDate> = events
        .iter()
        .filter_map(|event| parse_ticketmaster_event(event, artist))
        .collect();

    dates.sort_by_key(|d| d.date_unix_ms);
    dates
}

fn parse_ticketmaster_event(event: &serde_json::Value, requested_artist: &str) -> Option<TourDate> {
    // Validate: primary attraction must match requested artist (case-insensitive).
    let primary_attraction = event
        .get("_embedded")
        .and_then(|e| e.get("attractions"))
        .and_then(|a| a.as_array())
        .and_then(|arr| arr.first())
        .and_then(|a| a.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("");

    if !primary_attraction.eq_ignore_ascii_case(requested_artist) {
        return None;
    }

    // Date: combine localDate + localTime, parse via existing helper.
    let start = event.get("dates")?.get("start")?;
    let local_date = start.get("localDate")?.as_str()?;
    let local_time = start
        .get("localTime")
        .and_then(|t| t.as_str())
        .unwrap_or("00:00:00");
    let combined = format!("{}T{}", local_date, local_time);
    let date_unix_ms = parse_iso8601_to_unix_ms(&combined)?;

    // Venue + location.
    let venue = event
        .get("_embedded")
        .and_then(|e| e.get("venues"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first());

    let city = venue
        .and_then(|v| v.get("city"))
        .and_then(|c| c.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let region = venue
        .and_then(|v| v.get("state"))
        .and_then(|s| s.get("stateCode"))
        .and_then(|sc| sc.as_str())
        .unwrap_or("")
        .to_string();
    let country = venue
        .and_then(|v| v.get("country"))
        .and_then(|c| c.get("countryCode"))
        .and_then(|cc| cc.as_str())
        .unwrap_or("")
        .to_string();
    let venue_name = venue
        .and_then(|v| v.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();

    // Ticket URL: wrap with Impact affiliate prefix (no-op until configured).
    let raw_url = event.get("url").and_then(|u| u.as_str()).unwrap_or("");
    let ticket_url = if raw_url.is_empty() {
        None
    } else {
        Some(wrap_with_impact_affiliate(raw_url))
    };

    // Status mapping. Anything but "onsale" treated as SoldOut for UX simplicity.
    let status_code = event
        .get("dates")
        .and_then(|d| d.get("status"))
        .and_then(|s| s.get("code"))
        .and_then(|c| c.as_str())
        .unwrap_or("onsale");
    let status = if status_code.eq_ignore_ascii_case("onsale") {
        TicketStatus::Available
    } else {
        TicketStatus::SoldOut
    };

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

/// Parse ISO8601 datetime string to Unix milliseconds.
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
const THEAUDIODB_BASE: &str = "https://www.theaudiodb.com/api/v1/json/2/search.php";

/// Fetch artist thumbnail from TheAudioDB and return as `data:image/jpeg;base64,...`.
/// Returns None on any failure.
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

// ── HTTP client ────────────────────────────────────────────────────────────

/// Build a reqwest::Client with the User-Agent required by MusicBrainz TOS
/// and a 10s timeout covering all artist-info requests.
pub(crate) fn build_artist_info_http_client() -> Result<reqwest::Client> {
    let client = reqwest::Client::builder()
        .user_agent("hum/0.11.3 (https://github.com/basezero-projects/Hum; itswesl3y@gmail.com)")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    Ok(client)
}

// ── Disk cache ─────────────────────────────────────────────────────────────

/// On-disk structure per artist. Fields are individually timestamped so
/// tour-dates can be refreshed without blowing away the bio/photo.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct CachedArtistData {
    version: u32,
    name: Option<String>,
    slug: Option<String>,
    bio: Option<ArtistBio>,
    bio_fetched_at_unix_ms: Option<i64>,
    photo_data_url: Option<String>,
    photo_fetched_at_unix_ms: Option<i64>,
    tour_dates: Option<Vec<TourDate>>,
    tour_dates_fetched_at_unix_ms: Option<i64>,
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("cache").join("artist"))
}

fn cache_file_path(app: &AppHandle, slug: &str) -> Option<PathBuf> {
    cache_dir(app).map(|d| d.join(format!("{slug}.json")))
}

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
pub struct ArtistInfoCache {
    in_flight: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    app: AppHandle,
}

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
            let new_events = fetch_ticketmaster_events(&client, artist).await;
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

        // Parallel fetch: bio, events, photo.
        let (bio_result, events_result, photo_result) = tokio::join!(
            fetch_wikipedia_bio(&client, artist),
            fetch_ticketmaster_events(&client, artist),
            fetch_theaudiodb_photo(&client, artist),
        );

        let data = CachedArtistData {
            version: 1,
            name: Some(artist.to_string()),
            slug: Some(slug.clone()),
            bio: bio_result,
            bio_fetched_at_unix_ms: Some(now),
            photo_data_url: photo_result,
            photo_fetched_at_unix_ms: Some(now),
            tour_dates: Some(events_result),
            tour_dates_fetched_at_unix_ms: Some(now),
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
        tour_dates: data.tour_dates.clone().unwrap_or_default(),
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

    // ── Ticketmaster parser tests ──────────────────────────────────────────

    fn make_tm_event(attraction: &str, local_date: &str, local_time: Option<&str>, status: &str, url: &str) -> serde_json::Value {
        serde_json::json!({
            "name": format!("{} at Venue", attraction),
            "url": url,
            "dates": {
                "start": {
                    "localDate": local_date,
                    "localTime": local_time.unwrap_or("20:00:00")
                },
                "status": { "code": status }
            },
            "_embedded": {
                "attractions": [{ "name": attraction }],
                "venues": [{
                    "name": "Mission Ballroom",
                    "city": { "name": "Denver" },
                    "state": { "stateCode": "CO" },
                    "country": { "countryCode": "US", "name": "United States Of America" }
                }]
            }
        })
    }

    #[test]
    fn tm_parse_accepts_case_insensitive_match() {
        let event = make_tm_event("Shaggy", "2026-03-05", Some("20:00:00"), "onsale", "https://www.ticketmaster.com/event/abc");
        let result = parse_ticketmaster_event(&event, "shaggy");
        assert!(result.is_some(), "should match case-insensitively");
    }

    #[test]
    fn tm_parse_rejects_non_matching_artist() {
        let event = make_tm_event("Shaggy", "2026-03-05", Some("20:00:00"), "onsale", "https://www.ticketmaster.com/event/abc");
        let result = parse_ticketmaster_event(&event, "Bob Marley");
        assert!(result.is_none(), "should reject mismatched artist");
    }

    #[test]
    fn tm_parse_missing_local_time_defaults_midnight() {
        // Event without localTime — should default to 00:00:00 and still parse.
        let event = serde_json::json!({
            "name": "Shaggy at Venue",
            "url": "https://www.ticketmaster.com/event/abc",
            "dates": {
                "start": { "localDate": "2026-03-05" },
                "status": { "code": "onsale" }
            },
            "_embedded": {
                "attractions": [{ "name": "Shaggy" }],
                "venues": [{
                    "name": "Mission Ballroom",
                    "city": { "name": "Denver" },
                    "state": { "stateCode": "CO" },
                    "country": { "countryCode": "US" }
                }]
            }
        });
        let result = parse_ticketmaster_event(&event, "Shaggy");
        assert!(result.is_some());
        let tour_date = result.unwrap();
        // 2026-03-05T00:00:00 UTC → verify date is parseable (non-zero ms).
        assert!(tour_date.date_unix_ms > 0);
    }

    #[test]
    fn tm_parse_missing_venue_returns_empty_strings() {
        // Event with no _embedded.venues — should still return Some with empty location.
        let event = serde_json::json!({
            "name": "Shaggy at Venue",
            "url": "https://www.ticketmaster.com/event/abc",
            "dates": {
                "start": { "localDate": "2026-03-05", "localTime": "20:00:00" },
                "status": { "code": "onsale" }
            },
            "_embedded": {
                "attractions": [{ "name": "Shaggy" }],
                "venues": []
            }
        });
        let result = parse_ticketmaster_event(&event, "Shaggy");
        assert!(result.is_some());
        let tour_date = result.unwrap();
        assert_eq!(tour_date.city, "");
        assert_eq!(tour_date.venue, "");
    }

    #[test]
    fn tm_wrap_affiliate_noop_when_none() {
        let url = "https://www.ticketmaster.com/event/abc123";
        assert_eq!(wrap_with_impact_affiliate(url), url);
    }

    #[test]
    fn tm_status_onsale_available() {
        let event = make_tm_event("Shaggy", "2026-03-05", Some("20:00:00"), "onsale", "https://www.ticketmaster.com/event/abc");
        let result = parse_ticketmaster_event(&event, "Shaggy").unwrap();
        assert_eq!(result.status, TicketStatus::Available);
    }

    #[test]
    fn tm_status_cancelled_soldsout() {
        let event = make_tm_event("Shaggy", "2026-03-05", Some("20:00:00"), "cancelled", "https://www.ticketmaster.com/event/abc");
        let result = parse_ticketmaster_event(&event, "Shaggy").unwrap();
        assert_eq!(result.status, TicketStatus::SoldOut);
    }

    #[test]
    fn tm_status_offsale_soldsout() {
        let event = make_tm_event("Shaggy", "2026-03-05", Some("20:00:00"), "offsale", "https://www.ticketmaster.com/event/abc");
        let result = parse_ticketmaster_event(&event, "Shaggy").unwrap();
        assert_eq!(result.status, TicketStatus::SoldOut);
    }

    #[test]
    fn tm_status_postponed_soldsout() {
        let event = make_tm_event("Shaggy", "2026-03-05", Some("20:00:00"), "postponed", "https://www.ticketmaster.com/event/abc");
        let result = parse_ticketmaster_event(&event, "Shaggy").unwrap();
        assert_eq!(result.status, TicketStatus::SoldOut);
    }

    // ── Wikipedia helpers tests ────────────────────────────────────────────

    fn make_wiki_summary(
        page_type: &str,
        extract: &str,
        description: &str,
        desktop_url: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "type": page_type,
            "extract": extract,
            "description": description,
            "content_urls": {
                "desktop": {
                    "page": desktop_url
                }
            }
        })
    }

    #[test]
    fn wiki_parse_successful_direct_hit() {
        let body = make_wiki_summary(
            "standard",
            "Shaggy is a Jamaican-American reggae fusion singer and deejay.",
            "Jamaican-American singer",
            "https://en.wikipedia.org/wiki/Shaggy_(musician)",
        );
        let result = parse_wikipedia_summary(&body);
        assert!(result.is_some(), "should parse a valid music summary");
        let bio = result.unwrap();
        assert_eq!(bio.wikipedia_url, "https://en.wikipedia.org/wiki/Shaggy_(musician)");
        assert!(bio.text.contains("Shaggy"));
    }

    #[test]
    fn wiki_parse_rejects_non_music_description() {
        // Description "American politician" has no music keywords.
        let body = make_wiki_summary(
            "standard",
            "John Doe is an American politician who served in Congress.",
            "American politician",
            "https://en.wikipedia.org/wiki/John_Doe",
        );
        let result = parse_wikipedia_summary(&body);
        assert!(result.is_none(), "should reject non-music description");
    }

    #[test]
    fn wiki_parse_rejects_disambiguation_page() {
        // type == "disambiguation" should always be rejected regardless of description.
        let body = make_wiki_summary(
            "disambiguation",
            "Shaggy may refer to several things.",
            "American singer",
            "https://en.wikipedia.org/wiki/Shaggy",
        );
        let result = parse_wikipedia_summary(&body);
        assert!(result.is_none(), "disambiguation page should be rejected");
    }

    #[test]
    fn wiki_parse_rejects_missing_extract() {
        let body = serde_json::json!({
            "type": "standard",
            "extract": "",
            "description": "American rock band",
            "content_urls": {
                "desktop": { "page": "https://en.wikipedia.org/wiki/Foo" }
            }
        });
        let result = parse_wikipedia_summary(&body);
        assert!(result.is_none(), "empty extract should be rejected");
    }

    #[test]
    fn wiki_suffix_fallback_simulation() {
        // Direct lookup: disambiguation. Suffix "(musician)": standard + music.
        let disambig = make_wiki_summary(
            "disambiguation",
            "Artist may refer to:",
            "English singer",
            "https://en.wikipedia.org/wiki/Artist",
        );
        let suffix_hit = make_wiki_summary(
            "standard",
            "Artist is an English pop singer born in London.",
            "English singer",
            "https://en.wikipedia.org/wiki/Artist_(musician)",
        );

        // Direct = rejected (disambiguation).
        assert!(parse_wikipedia_summary(&disambig).is_none());
        // Suffix hit = accepted.
        let result = parse_wikipedia_summary(&suffix_hit);
        assert!(result.is_some(), "suffix fallback body should be accepted");
        assert_eq!(
            result.unwrap().wikipedia_url,
            "https://en.wikipedia.org/wiki/Artist_(musician)"
        );
    }

    #[test]
    fn wiki_all_attempts_fail_returns_none() {
        // A page with type "no-extract" and irrelevant description — simulate all
        // attempts returning this body. parse_wikipedia_summary should return None.
        let body = make_wiki_summary(
            "no-extract",
            "Some content that cannot be extracted.",
            "city in France",
            "https://en.wikipedia.org/wiki/City",
        );
        // Simulate all attempts (direct + all suffixes) returning the same body.
        for _ in 0..=WIKIPEDIA_SUFFIXES.len() {
            assert!(parse_wikipedia_summary(&body).is_none());
        }
    }

    #[test]
    fn wiki_truncation_at_sentence_boundary() {
        // Build a 2000-char extract with a sentence boundary before 1500 chars.
        let sentence_a = "This artist is a famous musician. "; // 34 chars
        let sentence_b = "B".repeat(1500 - sentence_a.len()); // fills to ~1500
        let sentence_b = format!("{}.", sentence_b); // ends with '.'
        let padding = "C".repeat(500); // push total past 1500
        let full_extract = format!("{}{}{}", sentence_a, sentence_b, padding);
        assert!(full_extract.len() > 1500);

        let truncated = truncate_bio(full_extract.clone());
        // Must be ≤ 1500 chars.
        assert!(truncated.len() <= 1500, "truncated bio should be ≤ 1500 chars");
        // Must end at a sentence boundary (last char is '.').
        assert!(
            truncated.ends_with('.'),
            "truncated bio should end at a sentence boundary"
        );
        // Must not contain the padding characters.
        assert!(!truncated.contains('C'), "truncated bio should not include padding past cutoff");
    }

    #[test]
    fn wiki_is_music_relevant_positive() {
        assert!(is_music_relevant("American rapper"));
        assert!(is_music_relevant("English rock band"));
        assert!(is_music_relevant("Jamaican-American singer"));
        assert!(is_music_relevant("Canadian songwriter"));
        assert!(is_music_relevant("electronic music producer"));
    }

    #[test]
    fn wiki_is_music_relevant_negative() {
        assert!(!is_music_relevant("city in France"));
        assert!(!is_music_relevant("American politician"));
        assert!(!is_music_relevant("fictional character"));
        assert!(!is_music_relevant("German automobile manufacturer"));
    }
}
