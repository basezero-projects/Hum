//! Artist-info fetch chain: Wikipedia bio and TheAudioDB
//! photo. Disk cache + in-flight dedup.

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
    pub fetched_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtistBio {
    pub text: String,
    pub wikipedia_url: String,
}

// ── Pure helpers ───────────────────────────────────────────────────────────

/// Derive a URL-safe slug from an artist name.
/// Lowercase, collapse whitespace to single dash, strip everything else
/// except ASCII alphanumerics and dashes. Common Latin diacritics mapped
/// to ASCII before stripping.
pub(crate) fn slug_for_artist(name: &str) -> String {
    // Diacritic → ASCII mapping for common Latin Extended chars.
    let mapped: String = name
        .chars()
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => {
                'a'
            }
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => {
                'o'
            }
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
        })
        .collect();

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

/// Top-level entry point for callers that don't hold an ArtistInfoCache.
/// Prefer ArtistInfoCache::fetch which adds caching + dedup.
#[allow(dead_code)]
pub async fn fetch_artist_info(artist: &str) -> Result<ArtistInfo> {
    let client = build_artist_info_http_client()?;
    let now = now_unix_ms();
    let (bio, photo) = tokio::join!(
        fetch_wikipedia_bio(&client, artist),
        fetch_theaudiodb_photo(&client, artist),
    );
    Ok(ArtistInfo {
        name: artist.to_string(),
        slug: slug_for_artist(artist),
        bio,
        photo_data_url: photo,
        fetched_at_unix_ms: now,
    })
}

// ── Wikipedia ─────────────────────────────────────────────────────────────

/// Music-relevance keywords used to gate Wikipedia results.
/// The description field (e.g. "American rapper", "English rock band") must
/// contain at least one of these substrings (case-insensitive) for the page
/// to be accepted as a music artist bio.
const MUSIC_KEYWORDS: &[&str] = &[
    "musician",
    "singer",
    "rapper",
    "songwriter",
    "band",
    "group",
    "dj",
    "producer",
    "composer",
    "musical",
    "music",
    "vocalist",
    "guitarist",
    "drummer",
    "bassist",
    "pianist",
    "rock",
    "pop",
    "hip hop",
    "hip-hop",
    "country",
    "jazz",
    "metal",
    "indie",
    "electronic",
    "r&b",
    "soul",
    "folk",
];

/// Disambiguator suffixes tried in order when the direct lookup fails the
/// music-relevance gate or returns a non-standard page type.
const WIKIPEDIA_SUFFIXES: &[&str] = &["musician", "singer", "rapper", "band", "rock band", "group"];

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
    Some(ArtistBio {
        text,
        wikipedia_url,
    })
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

    // SSRF guard: strArtistThumb is an arbitrary string from the TheAudioDB
    // JSON. Only follow it if it's an https URL on theaudiodb.com — a
    // malicious or MITM'd response could otherwise point this fetch at an
    // internal/LAN address (router admin pages, link-local metadata, etc.).
    match reqwest::Url::parse(thumb_url) {
        Ok(u) => {
            let ok = u.scheme() == "https"
                && u.host_str().is_some_and(|h| {
                    let h = h.to_ascii_lowercase();
                    h == "theaudiodb.com" || h.ends_with(".theaudiodb.com")
                });
            if !ok {
                eprintln!("[artist_info] theaudiodb thumb URL rejected: {thumb_url}");
                return None;
            }
        }
        Err(_) => return None,
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

/// On-disk structure per artist. Fields are individually timestamped.
/// Older cache files may carry a `tour_dates` field; serde ignores it.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct CachedArtistData {
    version: u32,
    name: Option<String>,
    slug: Option<String>,
    bio: Option<ArtistBio>,
    bio_fetched_at_unix_ms: Option<i64>,
    photo_data_url: Option<String>,
    photo_fetched_at_unix_ms: Option<i64>,
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("cache").join("artist"))
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

/// Cap on the per-artist JSON files retained under `cache/artist/`.
/// 500 artists × ~2KB typical = ~1MB. Heavy listeners exceed this over years
/// — eviction is oldest-mtime first (effectively LRU since tour-dates refresh
/// rewrites mtime every 12h for any artist still being listened to).
const MAX_ARTIST_CACHE_FILES: usize = 500;

/// Prune the artist disk cache to `MAX_ARTIST_CACHE_FILES` by deleting the
/// oldest-mtime files. Intended to run once at startup off the main thread.
/// Failures are logged and ignored — sweep is best-effort, never fatal.
pub async fn sweep_disk_cache(app: &AppHandle) {
    let Some(dir) = cache_dir(app) else { return };
    let mut read_dir = match tokio::fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let mut files: Vec<(PathBuf, SystemTime)> = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let mtime = match entry.metadata().await {
            Ok(m) => m.modified().unwrap_or(UNIX_EPOCH),
            Err(_) => continue,
        };
        files.push((path, mtime));
    }
    if files.len() <= MAX_ARTIST_CACHE_FILES {
        return;
    }
    files.sort_by_key(|(_, mtime)| *mtime);
    let to_remove = files.len() - MAX_ARTIST_CACHE_FILES;
    let total = files.len();
    let mut removed = 0usize;
    for (path, _) in files.iter().take(to_remove) {
        if tokio::fs::remove_file(path).await.is_ok() {
            removed += 1;
        }
    }
    eprintln!("[artist_info] disk cache sweep: removed {removed} oldest of {total}");
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
            let notify = match notify {
                // We waited on another fetch; return the cache result.
                None => return build_artist_info_from_cache(&cached, artist, &slug),
                Some(n) => n,
            };
            {
                let mut map = self.in_flight.lock().await;
                map.remove(&slug);
            }
            notify.notify_waiters();
            return build_artist_info_from_cache(&cached, artist, &slug);
        }

        // Cache miss — full fetch.
        // Guard: if notify is None we were a waiter, not the original fetcher.
        // The original fetch already ran but its cache write must have failed
        // (disk full, permission error, etc.). Return an error so the panel
        // shows the "Couldn't load artist info / Retry" state rather than
        // panicking on the unwrap below.
        let Some(notify) = notify else {
            return Err(anyhow::anyhow!(
                "upstream fetch failed; cache empty after wait"
            ));
        };
        let client = build_artist_info_http_client()?;

        // Parallel fetch: bio, photo.
        let (bio_result, photo_result) = tokio::join!(
            fetch_wikipedia_bio(&client, artist),
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
        fetched_at_unix_ms: data.bio_fetched_at_unix_ms.unwrap_or(0),
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
        assert_eq!(
            bio.wikipedia_url,
            "https://en.wikipedia.org/wiki/Shaggy_(musician)"
        );
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
        assert!(
            truncated.len() <= 1500,
            "truncated bio should be ≤ 1500 chars"
        );
        // Must end at a sentence boundary (last char is '.').
        assert!(
            truncated.ends_with('.'),
            "truncated bio should end at a sentence boundary"
        );
        // Must not contain the padding characters.
        assert!(
            !truncated.contains('C'),
            "truncated bio should not include padding past cutoff"
        );
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
