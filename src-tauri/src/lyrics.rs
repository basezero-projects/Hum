//! Lyrics fetcher + LRC parser + cache.
//!
//! Listens for `track-changed` Tauri events. When a new track arrives:
//!   1. Cleans the title (strips "(Official Video)", "[Lyrics]", etc.)
//!   2. Builds a cache key (`artist|title|duration_secs`)
//!   3. Looks it up in the in-memory cache
//!   4. Falls back to the persistent store (`tauri-plugin-store`)
//!   5. Falls back to LRCLib `/api/get` (then `/api/search` if 404)
//!   6. Parses the LRC string into `Vec<{ time_ms, text }>`
//!   7. Caches the result, emits `lyrics-loaded` or `lyrics-not-found`
//!
//! Network/5xx errors are NOT cached — only authoritative "not found" is.

use std::collections::HashMap;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener};
use tauri_plugin_store::StoreExt;
use tokio::sync::{mpsc, RwLock};

#[cfg(windows)]
use crate::smtc::SharedSnapshot;

#[cfg(not(windows))]
use crate::smtc::SharedSnapshot;

const STORE_FILE: &str = "lyrics-cache.json";
const USER_AGENT: &str =
    "lyric-overlay/0.1.0 (Windows desktop overlay; https://github.com/syvrstudios/lyric-overlay)";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LyricLine {
    pub time_ms: u32,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CachedLyrics {
    NotFound,
    Instrumental,
    Plain { text: String },
    Synced { lines: Vec<LyricLine> },
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CurrentLyrics {
    pub track_key: String,
    pub status: Status,
    pub source: Option<String>, // "memory" | "store" | "lrclib" | "lrclib-search"
    pub line_count: usize,
    pub lines: Vec<LyricLine>,
    pub plain: Option<String>,
    pub track: TrackEcho,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct TrackEcho {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Idle,
    Fetching,
    Synced,
    Plain,
    Instrumental,
    NotFound,
    Error,
}

pub type SharedLyrics = Arc<RwLock<CurrentLyrics>>;

pub fn start(app: AppHandle, shared: SharedLyrics, snapshot: SharedSnapshot) {
    let (tx, mut rx) = mpsc::unbounded_channel::<()>();

    // Subscribe to track-changed via Tauri's event bus. We only need a wakeup
    // signal — the worker reads the freshest data from the snapshot directly.
    let tx_track = tx.clone();
    app.listen_any("track-changed", move |_event| {
        let _ = tx_track.send(());
    });

    tauri::async_runtime::spawn(async move {
        let client = match build_client() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[lyrics] couldn't build http client: {e:#}");
                return;
            }
        };
        let mem: Arc<RwLock<HashMap<String, CachedLyrics>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let mut last_key = String::new();

        // Wake on startup in case a track was already playing when we started.
        let _ = tx.send(());

        while rx.recv().await.is_some() {
            let snap = { snapshot.read().await.clone() };
            if snap.title.trim().is_empty() {
                continue;
            }
            // Skip when artist is empty — usually means a YouTube video with
            // non-music content; LRCLib will 400 and we'd just spam the log.
            if snap.artist.trim().is_empty() {
                continue;
            }
            let key = cache_key(&snap.artist, &snap.title, snap.duration_ms);
            if key == last_key {
                continue;
            }
            last_key = key.clone();

            let track = TrackEcho {
                title: snap.title.clone(),
                artist: snap.artist.clone(),
                album: snap.album.clone(),
                duration_ms: snap.duration_ms,
            };

            // Mark fetching.
            {
                let mut s = shared.write().await;
                *s = CurrentLyrics {
                    track_key: key.clone(),
                    status: Status::Fetching,
                    source: None,
                    line_count: 0,
                    lines: vec![],
                    plain: None,
                    track: track.clone(),
                };
                emit_state(&app, &s);
            }

            let outcome = resolve_lyrics(&app, &client, &mem, &track, &key).await;
            apply_outcome(&app, &shared, &key, &track, outcome).await;
        }
    });
}

async fn resolve_lyrics(
    app: &AppHandle,
    client: &reqwest::Client,
    mem: &Arc<RwLock<HashMap<String, CachedLyrics>>>,
    track: &TrackEcho,
    key: &str,
) -> Outcome {
    // 1. In-memory
    if let Some(cached) = mem.read().await.get(key).cloned() {
        return Outcome { cached, source: "memory".into(), persist: false };
    }
    // 2. Persistent store
    if let Some(cached) = read_store(app, key) {
        mem.write().await.insert(key.to_string(), cached.clone());
        return Outcome { cached, source: "store".into(), persist: false };
    }
    // 3. Network
    let cleaned_title = clean_title(&track.title);
    match fetch_lrclib(client, &track.artist, &cleaned_title, &track.album, track.duration_ms).await
    {
        Ok((cached, source)) => {
            mem.write().await.insert(key.to_string(), cached.clone());
            Outcome { cached, source, persist: true }
        }
        Err(e) => {
            eprintln!("[lyrics] fetch failed for '{cleaned_title}' / '{}': {e:#}", track.artist);
            Outcome::error()
        }
    }
}

struct Outcome {
    cached: CachedLyrics,
    source: String,
    persist: bool,
}

impl Outcome {
    fn error() -> Self {
        Self {
            cached: CachedLyrics::NotFound,
            source: "error".into(),
            persist: false,
        }
    }
}

async fn apply_outcome(
    app: &AppHandle,
    shared: &SharedLyrics,
    key: &str,
    track: &TrackEcho,
    out: Outcome,
) {
    if out.persist {
        write_store(app, key, &out.cached);
    }
    let mut s = shared.write().await;
    s.track_key = key.to_string();
    s.source = Some(out.source.clone());
    s.track = track.clone();

    match out.cached {
        CachedLyrics::Synced { lines } => {
            s.status = Status::Synced;
            s.line_count = lines.len();
            s.plain = None;
            s.lines = lines;
            let _ = app.emit("lyrics-loaded", &*s);
        }
        CachedLyrics::Plain { text } => {
            s.status = Status::Plain;
            s.line_count = text.lines().count();
            s.plain = Some(text);
            s.lines = vec![];
            let _ = app.emit("lyrics-loaded", &*s);
        }
        CachedLyrics::Instrumental => {
            s.status = Status::Instrumental;
            s.line_count = 0;
            s.plain = None;
            s.lines = vec![];
            let _ = app.emit("lyrics-loaded", &*s);
        }
        CachedLyrics::NotFound => {
            s.status = if out.source == "error" {
                Status::Error
            } else {
                Status::NotFound
            };
            s.line_count = 0;
            s.plain = None;
            s.lines = vec![];
            let _ = app.emit("lyrics-not-found", &*s);
        }
    }
}

fn emit_state(app: &AppHandle, s: &CurrentLyrics) {
    let _ = app.emit("lyrics-state", s);
}

// ─── HTTP client ───────────────────────────────────────────────────────────

fn build_client() -> Result<reqwest::Client> {
    // LRCLib responses can take 8-10s on the wire from this network — give
    // generous headroom so we don't false-fail on cold queries.
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("reqwest::Client::build")
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct LrcRecord {
    #[allow(dead_code)]
    id: Option<u64>,
    #[allow(dead_code)]
    name: Option<String>,
    track_name: Option<String>,
    artist_name: Option<String>,
    #[allow(dead_code)]
    album_name: Option<String>,
    duration: Option<f64>,
    instrumental: Option<bool>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

async fn fetch_lrclib(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
    album: &str,
    duration_ms: u64,
) -> Result<(CachedLyrics, String)> {
    // Race /api/get and /api/search in parallel. LRCLib responses are ~8-10s
    // each from this network, so sequential fetch (get → maybe search) was up
    // to ~20s on misses. Parallel halves the wall-clock to ~10s.
    //
    // Priority on result: /api/get is canonical (exact metadata match), so it
    // wins whenever it returns content. /api/search is the fallback when
    // /api/get 404s or returns empty.
    let (get_res, search_res) = tokio::join!(
        try_get_lrclib(client, artist, title, album, duration_ms),
        try_search_lrclib(client, artist, title),
    );

    if let Ok(Some(rec)) = &get_res {
        let cached = to_cached_ref(rec);
        if !matches!(cached, CachedLyrics::NotFound) {
            return Ok((cached, "lrclib".into()));
        }
    }

    if let Ok(records) = &search_res {
        if let Some(rec) = pick_best(records.clone(), title, artist, duration_ms) {
            let cached = to_cached(rec);
            if !matches!(cached, CachedLyrics::NotFound) {
                return Ok((cached, "lrclib-search".into()));
            }
        }
    }

    // Both completed but had no content → authoritative NotFound.
    if get_res.is_ok() && search_res.is_ok() {
        return Ok((CachedLyrics::NotFound, "lrclib".into()));
    }

    // At least one was a transient error — surface it so we don't cache.
    match (get_res, search_res) {
        (Err(e), Err(_)) => Err(e.context("both /api/get and /api/search failed")),
        (Err(e), _) => Err(e.context("/api/get failed")),
        (_, Err(e)) => Err(e.context("/api/search failed")),
        _ => Ok((CachedLyrics::NotFound, "lrclib".into())),
    }
}

/// Returns Ok(Some(rec)) on a 200 hit, Ok(None) on any 4xx, Err on 5xx/network.
async fn try_get_lrclib(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
    album: &str,
    duration_ms: u64,
) -> Result<Option<LrcRecord>> {
    let dur_secs = (duration_ms / 1000).to_string();
    let mut params: Vec<(&str, &str)> = vec![
        ("artist_name", artist),
        ("track_name", title),
        ("duration", &dur_secs),
    ];
    if !album.trim().is_empty() {
        params.push(("album_name", album));
    }
    let url = reqwest::Url::parse_with_params("https://lrclib.net/api/get", &params)
        .context("build /api/get url")?;

    let resp = client.get(url).send().await.context("GET /api/get")?;
    let status = resp.status();
    if status.is_success() {
        let body = resp.text().await.context("read /api/get body")?;
        let rec: LrcRecord = serde_json::from_str(&body).context("parse /api/get json")?;
        return Ok(Some(rec));
    }
    if status.is_client_error() {
        return Ok(None);
    }
    anyhow::bail!("/api/get returned {status}");
}

/// Returns Ok(records) (possibly empty) on a 2xx, Err on 4xx/5xx/network.
async fn try_search_lrclib(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
) -> Result<Vec<LrcRecord>> {
    let url = reqwest::Url::parse_with_params(
        "https://lrclib.net/api/search",
        &[("track_name", title), ("artist_name", artist)],
    )
    .context("build /api/search url")?;

    let resp = client
        .get(url)
        .send()
        .await
        .context("GET /api/search")?
        .error_for_status()
        .context("/api/search status")?;

    let body = resp.text().await.context("read /api/search body")?;
    let records: Vec<LrcRecord> =
        serde_json::from_str(&body).context("parse /api/search json")?;
    Ok(records)
}

fn pick_best(
    records: Vec<LrcRecord>,
    title: &str,
    _artist: &str,
    requested_duration_ms: u64,
) -> Option<LrcRecord> {
    // Filter by:
    //   1. Title substring match (case-insensitive, bidirectional) — avoids
    //      picking entirely unrelated tracks that happened to surface in search.
    //   2. Duration within ±5s of the requested track — covers/remixes of the
    //      same name usually have very different lengths. This was the Duka/
    //      Toxic risk: Ashnikko's 163s Toxic shouldn't get picked when a 203s
    //      Toxic was requested.
    let requested_secs = requested_duration_ms as i64 / 1000;
    let title_l = title.to_lowercase();
    let tolerance_secs: i64 = 5;

    let mut candidates: Vec<_> = records
        .into_iter()
        .filter(|r| {
            let rec_title = r.track_name.as_deref().unwrap_or("").to_lowercase();
            let title_match =
                rec_title.contains(&title_l) || title_l.contains(&rec_title) || rec_title == title_l;
            if !title_match {
                return false;
            }
            // Skip duration filter only when we have no requested duration to
            // compare against (shouldn't happen in practice — SMTC and iTunes
            // both provide it).
            if requested_secs == 0 {
                return true;
            }
            let r_secs = r.duration.unwrap_or(0.0) as i64;
            (r_secs - requested_secs).abs() <= tolerance_secs
        })
        .collect();

    candidates.sort_by(|a, b| {
        // Prefer records with synced lyrics.
        let ra = a.synced_lyrics.is_some();
        let rb = b.synced_lyrics.is_some();
        rb.cmp(&ra)
    });
    candidates.into_iter().next()
}

fn to_cached_ref(rec: &LrcRecord) -> CachedLyrics {
    // Convenience: clone the bits we need without consuming the record.
    if rec.instrumental.unwrap_or(false) {
        return CachedLyrics::Instrumental;
    }
    if let Some(s) = rec.synced_lyrics.as_deref() {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return CachedLyrics::Synced { lines };
        }
    }
    if let Some(p) = rec.plain_lyrics.as_ref() {
        if !p.trim().is_empty() {
            return CachedLyrics::Plain { text: p.clone() };
        }
    }
    CachedLyrics::NotFound
}

fn to_cached(rec: LrcRecord) -> CachedLyrics {
    if rec.instrumental.unwrap_or(false) {
        return CachedLyrics::Instrumental;
    }
    if let Some(s) = rec.synced_lyrics.as_deref() {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return CachedLyrics::Synced { lines };
        }
    }
    if let Some(p) = rec.plain_lyrics {
        if !p.trim().is_empty() {
            return CachedLyrics::Plain { text: p };
        }
    }
    let _ = (rec.duration, rec.artist_name, rec.track_name);
    CachedLyrics::NotFound
}

// ─── Title cleaner ─────────────────────────────────────────────────────────

fn cleaner() -> &'static Regex {
    static C: OnceLock<Regex> = OnceLock::new();
    C.get_or_init(|| {
        // (?ix) = case-insensitive + ignore whitespace inside the pattern.
        Regex::new(
            r"(?ix)
              \s*[\[\(]\s*
              (?:
                  official\s+(?:music\s+|lyric\s+|hd\s+)?video |
                  music\s+video |
                  lyric\s+video |
                  lyrics? |
                  audio |
                  visualizer |
                  feat\.?\s.* |
                  ft\.?\s.* |
                  featuring\s.* |
                  with\s.* |
                  remaster(?:ed)?(?:\s\d{2,4})? |
                  \d{2,4}\s+remaster(?:ed)? |
                  re-?recorded(?:\s\d{2,4})? |
                  live(?:\s+(?:at|from|in)\s+.*)? |
                  acoustic |
                  unplugged |
                  demo |
                  single\s+version |
                  album\s+version |
                  radio\s+(?:edit|version|mix) |
                  extended\s+(?:mix|version) |
                  original\s+(?:mix|version) |
                  edit |
                  bonus\s+track |
                  \d{1,2}k |
                  hd | uhd | mv
              )
              \s*[\]\)]
            ",
        )
        .unwrap()
    })
}

pub fn clean_title(title: &str) -> String {
    let cleaned = cleaner().replace_all(title, "").to_string();
    cleaned.trim().to_string()
}

// ─── LRC parser ────────────────────────────────────────────────────────────

fn ts_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]").unwrap())
}

pub fn parse_lrc(s: &str) -> Vec<LyricLine> {
    let re = ts_re();
    let mut lines: Vec<LyricLine> = Vec::new();
    for raw in s.lines() {
        let mut rest = raw.trim_start();
        let mut times: Vec<u32> = Vec::new();
        while let Some(cap) = re.captures(rest) {
            let mm: u32 = cap[1].parse().unwrap_or(0);
            let ss: u32 = cap[2].parse().unwrap_or(0);
            let frac_ms: u32 = cap.get(3).map_or(0, |m| {
                let s = m.as_str();
                let n: u32 = s.parse().unwrap_or(0);
                match s.len() {
                    1 => n * 100,
                    2 => n * 10,
                    _ => n,
                }
            });
            times.push(mm.saturating_mul(60_000).saturating_add(ss * 1_000).saturating_add(frac_ms));
            let consumed = cap[0].len();
            rest = &rest[consumed..];
        }
        if times.is_empty() {
            continue; // metadata tag like [ti:..] or non-timestamped line
        }
        let text = rest.trim().to_string();
        for t in times {
            lines.push(LyricLine { time_ms: t, text: text.clone() });
        }
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

// ─── Cache key ─────────────────────────────────────────────────────────────

fn cache_key(artist: &str, title: &str, duration_ms: u64) -> String {
    let dur_secs = duration_ms / 1000;
    format!("{}|{}|{}", normalize(artist), normalize(title), dur_secs)
}

fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

// ─── Persistent store (tauri-plugin-store) ─────────────────────────────────

fn read_store(app: &AppHandle, key: &str) -> Option<CachedLyrics> {
    let store = app.store(STORE_FILE).ok()?;
    let v = store.get(key)?;
    serde_json::from_value(v).ok()
}

fn write_store(app: &AppHandle, key: &str, cached: &CachedLyrics) {
    let Ok(store) = app.store(STORE_FILE) else { return };
    let Ok(v) = serde_json::to_value(cached) else { return };
    store.set(key, v);
    let _ = store.save();
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_titles() {
        assert_eq!(clean_title("Apocalypse (Official Video)"), "Apocalypse");
        assert_eq!(clean_title("Apocalypse [Lyrics]"), "Apocalypse");
        assert_eq!(clean_title("Hey Jude (Remastered 2009)"), "Hey Jude");
        assert_eq!(clean_title("Sweet Caroline (feat. Someone)"), "Sweet Caroline");
        assert_eq!(clean_title("Test Song [HD] (4K)"), "Test Song");
        assert_eq!(clean_title("Track Name (Live at Wembley)"), "Track Name");
        assert_eq!(clean_title("Plain Title"), "Plain Title");
    }

    #[test]
    fn parses_basic_lrc() {
        let s = "[ti:Hello]\n[ar:World]\n[00:01.50]Line one\n[00:03.25]Line two\n";
        let lines = parse_lrc(s);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time_ms, 1_500);
        assert_eq!(lines[0].text, "Line one");
        assert_eq!(lines[1].time_ms, 3_250);
    }

    #[test]
    fn parses_multi_timestamp_lrc() {
        let s = "[00:01.00][01:01.00]Repeated line\n";
        let lines = parse_lrc(s);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time_ms, 1_000);
        assert_eq!(lines[1].time_ms, 61_000);
        assert_eq!(lines[0].text, lines[1].text);
    }

    #[test]
    fn parses_three_digit_fraction_lrc() {
        let s = "[00:01.123]Millisecond precision\n";
        let lines = parse_lrc(s);
        assert_eq!(lines[0].time_ms, 1_123);
    }

    #[test]
    fn parses_no_fraction_lrc() {
        let s = "[00:05]Five seconds in\n";
        let lines = parse_lrc(s);
        assert_eq!(lines[0].time_ms, 5_000);
    }
}
