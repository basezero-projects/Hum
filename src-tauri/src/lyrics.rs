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
    "hum/0.11.8 (Windows desktop overlay; https://github.com/basezero-projects/Hum)";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WordSpan {
    pub time_ms: u32,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LyricLine {
    pub time_ms: u32,
    pub text: String,
    /// Word-level timing inside this line (when the source provides enhanced
    /// LRC like SimpMusic's `richSyncLyrics`). None for line-level-only sources
    /// like LRCLib. Frontend uses this for karaoke-style highlighting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<WordSpan>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CachedLyrics {
    NotFound,
    /// The source publishes audio but doesn't expose track metadata in any
    /// form Hum can read (e.g. Pandora web with no UIA selector match).
    /// Renders as a clear "source-specific reason" message rather than
    /// the generic "no lyrics for <garbage tab title>" output.
    Unsupported,
    Instrumental,
    Plain {
        text: String,
    },
    Synced {
        lines: Vec<LyricLine>,
        /// Optional translation lines (one-to-one with `lines` when present).
        /// Only NetEase provides this in practice (Chinese translations).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        translation: Option<Vec<LyricLine>>,
    },
}

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CurrentLyrics {
    pub track_key: String,
    pub status: Status,
    /// "memory" | "store" | "lrclib" | "lrclib-search" | "simpmusic" | "netease" | "all-sources" | "error"
    pub source: Option<String>,
    pub line_count: usize,
    pub lines: Vec<LyricLine>,
    pub plain: Option<String>,
    /// Per-line translations (when available — NetEase Chinese tlyric).
    pub translation: Option<Vec<LyricLine>>,
    /// Per-source failure strings, populated only when `status == Error`. Each
    /// entry is prefixed with the source name (`"lrclib: ..."`, `"simpmusic:
    /// ..."`, `"netease: ..."`) so the dev console can show what went wrong.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
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
    Unsupported,
    Error,
}

pub type SharedLyrics = Arc<RwLock<CurrentLyrics>>;

pub fn start(
    app: AppHandle,
    shared: SharedLyrics,
    snapshot: SharedSnapshot,
    #[cfg(windows)] web_bridge: crate::web_bridge::SharedWebBridge,
) {
    let (tx, mut rx) = mpsc::unbounded_channel::<()>();

    // Subscribe to track-changed via Tauri's event bus. We only need a wakeup
    // signal — the worker reads the freshest data from the snapshot directly.
    let tx_track = tx.clone();
    app.listen_any("track-changed", move |_event| {
        let _ = tx_track.send(());
    });

    // Bridge probes (web_bridge.rs) emit web-bridge-updated when they
    // read a new track from Chrome's UIA tree. Wake the resolver loop
    // through the same channel — the bridge-cache consultation below
    // picks up the fresh values.
    #[cfg(windows)]
    {
        let tx_bridge = tx.clone();
        app.listen_any("web-bridge-updated", move |_event| {
            let _ = tx_bridge.send(());
        });
    }

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

            // Consult the web-player bridge. If a probe wrote real track info
            // within the staleness window (5s), use that. Otherwise fall back
            // to SMTC's snapshot. Pandora.com is the motivating case — SMTC
            // sees only the browser tab title; the bridge fills in the real
            // song via UIA.
            #[cfg(windows)]
            let (effective_title, effective_artist, effective_album, bridge_fresh, unreliable_no_bridge) = {
                let bridge_track = {
                    let b = web_bridge.read().await;
                    b.clone()
                };
                let now_unix_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let fresh = bridge_track
                    .as_ref()
                    .is_some_and(|t| now_unix_ms - t.last_seen_unix_ms < 5_000 && !t.title.trim().is_empty());

                let (title, artist, album) = if fresh {
                    let t = bridge_track.as_ref().unwrap();
                    (t.title.clone(), t.artist.clone(), t.album.clone())
                } else {
                    (snap.title.clone(), snap.artist.clone(), snap.album.clone())
                };

                // If SMTC's title matches a known-unreliable-source probe AND
                // we don't have fresh bridge data, surface Unsupported instead
                // of running the resolver against the garbage SMTC title.
                // Pandora web with the UIA probe broken / not-yet-read is the
                // motivating case — we'd otherwise look up the browser tab
                // title as if it were a song and get noise back.
                let unreliable = !fresh
                    && crate::web_bridge::any_probe_detects(
                        &snap.title,
                        snap.source_app_id.as_deref().unwrap_or(""),
                    );

                (title, artist, album, fresh, unreliable)
            };

            #[cfg(not(windows))]
            let (effective_title, effective_artist, effective_album, bridge_fresh, unreliable_no_bridge) = (
                snap.title.clone(),
                snap.artist.clone(),
                snap.album.clone(),
                false,
                false,
            );

            let _ = bridge_fresh; // consumed via unreliable_no_bridge path only

            if effective_title.trim().is_empty() {
                continue;
            }

            let key = cache_key(&effective_artist, &effective_title, snap.duration_ms);
            if key == last_key {
                continue;
            }
            last_key = key.clone();

            let track = TrackEcho {
                title: effective_title.clone(),
                artist: effective_artist.clone(),
                album: effective_album.clone(),
                duration_ms: snap.duration_ms,
            };

            if unreliable_no_bridge {
                // Short-circuit: emit Unsupported, do NOT hit any network source.
                // The resolver's normal LRCLib / SimpMusic / NetEase chain would
                // burn an HTTP round trip on a non-song query and return NotFound
                // anyway. Skipping it saves the round trip and renders the
                // honest "Pandora web — track info unavailable" message.
                apply_outcome(
                    &app,
                    &shared,
                    &key,
                    &track,
                    Outcome {
                        cached: CachedLyrics::Unsupported,
                        source: "unsupported-source".into(),
                        persist: false,
                        errors: Vec::new(),
                    },
                )
                .await;
                continue;
            }

            // Mark fetching. The `errors: vec![]` reset prevents stale errors
            // from a previous track's resolution from leaking into the dev
            // console while this one is still in flight.
            {
                let mut s = shared.write().await;
                *s = CurrentLyrics {
                    track_key: key.clone(),
                    status: Status::Fetching,
                    source: None,
                    line_count: 0,
                    lines: vec![],
                    plain: None,
                    translation: None,
                    errors: vec![],
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
        return Outcome {
            cached,
            source: "memory".into(),
            persist: false,
            errors: Vec::new(),
        };
    }
    // 2. Persistent store
    if let Some(cached) = read_store(app, key) {
        mem.write().await.insert(key.to_string(), cached.clone());
        return Outcome {
            cached,
            source: "store".into(),
            persist: false,
            errors: Vec::new(),
        };
    }

    // 3. Network — try sources in priority order. LRCLib first (largest +
    // best metadata match), then SimpMusic (often has rich/word-level), then
    // NetEase (broad coverage incl. translations). A source returning
    // NotFound proceeds to the next; a transient error also proceeds (we
    // still want a chance at a hit), but is recorded so the dev console can
    // surface it.
    //
    // Title noise like "(Official Video)" and "[Lyrics]" is stripped via
    // `clean_title`. Artist noise from YouTube — " - Topic" suffixes on auto-
    // generated channels, " VEVO", " - Official Artist Channel" — is stripped
    // via `clean_artist`. Without that, LRCLib's exact match never hits and
    // /api/search returns 400 on the noisy params, which used to surface as
    // "error fetching lyrics" instead of a clean NotFound.
    let cleaned_title = clean_title(&track.title);
    let cleaned_artist = clean_artist(&track.artist);

    // Mashups / bootlegs / fan edits don't exist on any canonical lyric
    // source (LRCLib, SimpMusic, NetEase) — only their constituent songs
    // do. Falling through to those sources means we end up matching a
    // single song's lyrics against the mashup audio, producing
    // confidently-wrong out-of-sync output (the "Twista x Wetter (SW
    // Mashup)" case Wes hit returned Twista's "Wetter" lyrics, which
    // drift several minutes off the actual mashup playback). No lyrics
    // beats wrong lyrics. Detection is intentionally conservative —
    // only the explicit fan-creation keywords, not heuristic " x " /
    // " vs " separators (which appear in legit song titles like
    // "Romeo x Juliet").
    if looks_like_mashup(&track.title) {
        return Outcome {
            cached: CachedLyrics::NotFound,
            source: "mashup-skip".into(),
            persist: false,
            errors: Vec::new(),
        };
    }

    let mut errors: Vec<String> = Vec::new();
    // Did at least one source authoritatively reply "no match" (vs erroring)?
    // If yes, we treat the overall result as NotFound even when other sources
    // errored — a peer's network blip doesn't downgrade an authoritative miss
    // to a generic "fetch failed." Only when *every* source errored is this
    // a real fetch failure that warrants `Status::Error`.
    let mut any_clean_notfound = false;

    match fetch_lrclib(client, &cleaned_artist, &cleaned_title, &track.album, track.duration_ms)
        .await
    {
        Ok((cached, source)) if !matches!(cached, CachedLyrics::NotFound) => {
            mem.write().await.insert(key.to_string(), cached.clone());
            return Outcome { cached, source, persist: true, errors: Vec::new() };
        }
        Ok(_) => {
            any_clean_notfound = true;
        }
        Err(e) => {
            eprintln!(
                "[lyrics] lrclib failed for '{cleaned_title}' / '{cleaned_artist}': {e:#}"
            );
            errors.push(format!("lrclib: {e:#}"));
        }
    }

    match fetch_simpmusic(client, &cleaned_artist, &cleaned_title, track.duration_ms).await {
        Ok((cached, source)) if !matches!(cached, CachedLyrics::NotFound) => {
            mem.write().await.insert(key.to_string(), cached.clone());
            return Outcome { cached, source, persist: true, errors: Vec::new() };
        }
        Ok(_) => {
            any_clean_notfound = true;
        }
        Err(e) => {
            eprintln!(
                "[lyrics] simpmusic failed for '{cleaned_title}' / '{cleaned_artist}': {e:#}"
            );
            errors.push(format!("simpmusic: {e:#}"));
        }
    }

    match fetch_netease(client, &cleaned_artist, &cleaned_title, track.duration_ms).await {
        Ok((cached, source)) if !matches!(cached, CachedLyrics::NotFound) => {
            mem.write().await.insert(key.to_string(), cached.clone());
            return Outcome { cached, source, persist: true, errors: Vec::new() };
        }
        Ok(_) => {
            any_clean_notfound = true;
        }
        Err(e) => {
            eprintln!(
                "[lyrics] netease failed for '{cleaned_title}' / '{cleaned_artist}': {e:#}"
            );
            errors.push(format!("netease: {e:#}"));
        }
    }

    if any_clean_notfound {
        // At least one authoritative miss — show NotFound. Errors (if any)
        // still pass through to `CurrentLyrics.errors` so the dev console can
        // surface the peer timeout for debugging, but the user-facing status
        // is the clean miss, not a generic "error fetching lyrics."
        //
        // Don't cache NotFound in memory either. Combined with the symmetric
        // disk-cache change in v0.10.15 (read_store discards NotFound,
        // write_store skips NotFound), this means every track change re-runs
        // the resolver against an unfindable track. The algorithm is still
        // evolving — every recent release added new YouTube-noise patterns,
        // punctuation normalization, or duration tweaks — and caching
        // NotFound was masking those improvements within a session. Cost:
        // ~1-2s of parallel API calls per replay of an unfindable track,
        // which runs in the background and doesn't block the overlay UI.
        Outcome {
            cached: CachedLyrics::NotFound,
            source: "all-sources".into(),
            persist: false,
            errors,
        }
    } else {
        // Every source errored — a true fetch failure. Don't cache; surface
        // as Status::Error so the user knows to wait it out.
        Outcome::error(errors)
    }
}

struct Outcome {
    cached: CachedLyrics,
    source: String,
    persist: bool,
    /// Per-source failures collected during this resolution. Only populated on
    /// the error branch; flows into `CurrentLyrics::errors` so the dev console
    /// can show the actual reqwest/anyhow chain instead of "(network)".
    errors: Vec<String>,
}

impl Outcome {
    fn error(errors: Vec<String>) -> Self {
        Self {
            cached: CachedLyrics::NotFound,
            source: "error".into(),
            persist: false,
            errors,
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
    s.errors = out.errors;
    s.track = track.clone();

    match out.cached {
        CachedLyrics::Synced { lines, translation } => {
            s.status = Status::Synced;
            s.line_count = lines.len();
            s.plain = None;
            s.lines = lines;
            s.translation = translation;
            let _ = app.emit("lyrics-loaded", &*s);
        }
        CachedLyrics::Plain { text } => {
            s.status = Status::Plain;
            s.line_count = text.lines().count();
            s.plain = Some(text);
            s.lines = vec![];
            s.translation = None;
            let _ = app.emit("lyrics-loaded", &*s);
        }
        CachedLyrics::Instrumental => {
            s.status = Status::Instrumental;
            s.line_count = 0;
            s.plain = None;
            s.lines = vec![];
            s.translation = None;
            let _ = app.emit("lyrics-loaded", &*s);
        }
        CachedLyrics::Unsupported => {
            s.status = Status::Unsupported;
            s.line_count = 0;
            s.plain = None;
            s.lines = vec![];
            s.translation = None;
            let _ = app.emit("lyrics-not-found", &*s);
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
            s.translation = None;
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
    // generous headroom so we don't false-fail on cold queries. NetEase needs
    // a cookie jar for its NMTID handshake; the jar is harmless for the other
    // hosts (they don't set cookies).
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        .cookie_store(true)
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
        try_search_lrclib_once(client, title),
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

    // Aggressive retry: when the first-pass search either returned zero
    // records OR returned records that all failed pick_best (wrong title
    // shape, wrong duration), try again with the YouTube-noise-stripped
    // title. This catches the "G Eazy & Halsey - Him & I (Lyrics)" case
    // where LRCLib returned 3 unsynced "G-Eazy & Halsey - Him & I (Official
    // Video)" records that failed pick_best's substring check (hyphen vs
    // space in "G-Eazy" vs "G Eazy"), while the stripped query "Him & I"
    // returns the canonical synced record. The retry runs only when the
    // first pass didn't yield a usable record AND there's something to
    // strip — keeps the API call cost at +0 in the happy path.
    let stripped = strip_youtube_noise(title);
    if !stripped.is_empty() && stripped != title {
        if let Ok(records) = try_search_lrclib_once(client, &stripped).await {
            if let Some(rec) = pick_best(records, &stripped, artist, duration_ms) {
                let cached = to_cached(rec);
                if !matches!(cached, CachedLyrics::NotFound) {
                    return Ok((cached, "lrclib-search".into()));
                }
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
///
/// `/api/get` requires exact-match artist + title to be useful — when artist
/// is blank (common on YouTube auto-generated Topic videos), skip the call
/// entirely; `/api/search` (which `fetch_lrclib` races in parallel) picks up
/// the slack via title-only search.
async fn try_get_lrclib(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
    album: &str,
    duration_ms: u64,
) -> Result<Option<LrcRecord>> {
    if artist.trim().is_empty() {
        return Ok(None);
    }
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

/// Single LRCLib `/api/search` call. Returns Ok(records) (possibly empty)
/// on a 2xx OR 4xx (4xx means "your query didn't match anything I can
/// parse" — that's an authoritative miss, not a transient error). Err
/// only on 5xx / network / parse failures.
///
/// Title-only search. We don't pass `artist_name` — LRCLib applies that
/// as a strict filter and SMTC-reported artists routinely diverge from
/// LRCLib's canonical form ("TPainVEVO" → cleans to "TPain"; LRCLib has
/// "T-Pain"). `pick_best`'s bidirectional title-substring filter + ±5s
/// duration filter handles disambiguation downstream.
///
/// The aggressive retry (call this again with `strip_youtube_noise(title)`
/// to drop the leading `"Artist - "` prefix and trailing `" ft. X"`) is
/// the caller's responsibility — `fetch_lrclib` does it when the first
/// pass + pick_best didn't yield a usable record. Was previously a
/// `try_search_lrclib` wrapper here that did the retry on empty-records
/// only; moved to `fetch_lrclib` so it also fires when records came back
/// but pick_best filtered them all out.
async fn try_search_lrclib_once(
    client: &reqwest::Client,
    title: &str,
) -> Result<Vec<LrcRecord>> {
    let url = reqwest::Url::parse_with_params(
        "https://lrclib.net/api/search",
        &[("track_name", title)],
    )
    .context("build /api/search url")?;

    let resp = client.get(url).send().await.context("GET /api/search")?;
    let status = resp.status();
    if status.is_client_error() {
        return Ok(Vec::new());
    }
    if !status.is_success() {
        anyhow::bail!("/api/search returned {status}");
    }
    let body = resp.text().await.context("read /api/search body")?;
    let records: Vec<LrcRecord> =
        serde_json::from_str(&body).context("parse /api/search json")?;
    Ok(records)
}

/// Aggressive YouTube-noise stripper, applied only as retry-on-miss fallback
/// for LRCLib /api/search. NOT applied to the title shown in the dev console
/// or used for /api/get (which already requires exact metadata match).
///
/// Operations, in order:
/// 1. Strip trailing ` ft. X` / ` feat. X` / ` featuring X` (case-insensitive)
///    that survived `clean_title` because it wasn't inside parens/brackets.
/// 2. Strip leading `Word(s) - ` when the title contains ` - ` AND the
///    candidate post-strip still has ≥2 non-whitespace chars (avoids
///    eating the whole title for short fragments like `"A - B"`).
///
/// Edge case: titles with legit embedded ` - ` like `"Born In The U.S.A. -
/// 1984 Remaster"` would strip to just `"1984 Remaster"` here, which won't
/// find lyrics either. Net result: NotFound, same as the baseline. The
/// retry only runs when the baseline already returned zero, so the false-
/// positive cost is "we still don't find lyrics" — never worse than the
/// status quo. The gain is YouTube uploader conventions like
/// `"T-Pain - Bartender ft. Akon"` → `"Bartender"` now resolve correctly.
fn strip_youtube_noise(title: &str) -> String {
    static FEAT_RE: OnceLock<Regex> = OnceLock::new();
    let feat_re = FEAT_RE.get_or_init(|| {
        Regex::new(r"(?i)\s+(?:feat\.?|ft\.?|featuring)\s+.+$").unwrap()
    });

    let mut s = feat_re.replace(title, "").to_string();

    if let Some(idx) = s.find(" - ") {
        let candidate = s[idx + 3..].trim().to_string();
        if candidate.chars().filter(|c| !c.is_whitespace()).count() >= 2 {
            s = candidate;
        }
    }

    s.trim().to_string()
}

/// Detect fan mashups / bootlegs / DJ edits that don't exist on canonical
/// lyric sources. Conservative: only flags titles containing explicit
/// fan-creation keywords. " x " / " vs " / " versus " are NOT included
/// because they appear in plenty of legit released tracks ("Romeo x
/// Juliet", "Spy vs Spy", "Smith Vs Mills"). False negatives — letting
/// an ambiguous title through to the normal resolver — are acceptable
/// since the scoring threshold there will reject weak matches. False
/// positives — refusing to resolve a real song — are not, since the
/// user gets nothing instead of correct lyrics.
fn looks_like_mashup(title: &str) -> bool {
    let lower = title.to_lowercase();
    // Exact-substring checks (no regex needed). Keep this list short and
    // unambiguous — anything that's basically only used by fan uploaders.
    lower.contains("mashup")
        || lower.contains("bootleg")
        || lower.contains("fan edit")
        || lower.contains("flip edit")
        || lower.contains("dj edit")
}

/// Lowercase + collapse common Unicode punctuation that LRCLib uploaders use
/// inconsistently into ASCII equivalents. Two different uploads of the same
/// song routinely use different apostrophe flavors (`'` ASCII vs `'` U+2019
/// vs `'` U+2018), different quote flavors, or hyphen vs en-dash. Without
/// this, the substring match in `pick_best` rejects records that are
/// otherwise correct — e.g. a YouTube-bridged title with `Can't` (ASCII)
/// would miss a LRCLib record uploaded as `Can't` (curly).
fn normalize_for_match(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            // Apostrophes: curly right (’), curly left (‘), prime (′), reversed prime (‵)
            '\u{2019}' | '\u{2018}' | '\u{2032}' | '\u{2035}' => '\'',
            // Double quotes: curly left (“), curly right (”), double prime (″)
            '\u{201C}' | '\u{201D}' | '\u{2033}' => '"',
            // Dashes: en-dash (–), em-dash (—), figure dash (‒), horizontal bar (―)
            '\u{2013}' | '\u{2014}' | '\u{2012}' | '\u{2015}' => '-',
            // Non-breaking space → regular space
            '\u{00A0}' => ' ',
            _ => c,
        })
        .collect::<String>()
        .to_lowercase()
}

/// Pick the best LRCLib record for the user's track using a weighted
/// scoring system instead of cascading hard filters. Every previous
/// approach (substring + ±N-second duration) had an "all-or-nothing"
/// failure mode: one weak signal rejected the candidate entirely. Real
/// LRCLib data is too noisy for that — YouTube lyric uploads add 5-15s
/// of intro/outro padding, different uploaders use different artist
/// capitalizations, some records carbon-copy the YouTube title verbatim
/// (with the "Artist - " prefix and "(Lyrics)" suffix) while the canonical
/// upload uses the clean studio title. The scoring system uses each
/// signal as evidence and lets a strong match on one signal compensate
/// for a weak match on another.
///
/// Score components (max ~165, threshold 80):
///   - **Title (0-100)**: exact match = 100, one-contains-other = 60-90
///     based on length ratio, weak word-token overlap = 20-50, no overlap = -1
///     (filtered out — saves cycles since title is the dominant signal).
///   - **Duration (-50 to +30)**: 0-5s diff = 30, 6-10s = 22, 11-20s = 12,
///     21-30s = 4, 31+s = -50 (strong negative, reflects "this is probably
///     a different recording entirely"). Zero requested duration = neutral 15.
///   - **Artist (0-20)**: only when user-provided artist is non-empty.
///     Exact = 20, substring = 10, otherwise 0.
///   - **Synced bonus (0 or +20)**: synced records preferred over plain.
///
/// Threshold 80 means a record needs strong evidence on at least two
/// signals to be returned. Concrete cases:
///   - Exact title + same duration + synced = 100 + 30 + 20 = 150 → picked.
///   - Exact title + 8s duration diff + synced = 100 + 22 + 20 = 142 → picked.
///     (This is the "The Script" / "Fleetwood Mac" lyric-video padding case.)
///   - Exact title + 40s duration diff + synced = 100 + (-50) + 20 = 70 →
///     filtered. (This is the "Ashnikko Toxic vs Britney Toxic" disambiguation
///     — when only the wrong-duration record exists, return None and let
///     the strip-and-retry path try a cleaner query.)
///   - Partial title match (rec has the user title as substring) + 5s diff
///     + synced = 80 + 30 + 20 = 130 → picked. (Carbon-copy LRCLib uploads
///       of the YouTube video title.)
///
/// Substring/contains check uses `normalize_for_match` (lowercase + collapse
/// Unicode punctuation flavors) so curly-vs-ASCII apostrophe mismatches
/// don't artificially lower the score.
fn pick_best(
    records: Vec<LrcRecord>,
    title: &str,
    artist: &str,
    requested_duration_ms: u64,
) -> Option<LrcRecord> {
    let title_l = normalize_for_match(title);
    let artist_l = normalize_for_match(artist);
    let requested_secs = requested_duration_ms as i64 / 1000;

    const THRESHOLD: i64 = 80;

    let mut scored: Vec<(i64, LrcRecord)> = records
        .into_iter()
        .map(|r| {
            let rec_title = normalize_for_match(r.track_name.as_deref().unwrap_or(""));
            let rec_artist = normalize_for_match(r.artist_name.as_deref().unwrap_or(""));

            // --- Title score -----------------------------------------------
            let title_score: i64 = if rec_title.is_empty() {
                -1
            } else if rec_title == title_l {
                100
            } else if rec_title.contains(&title_l) || title_l.contains(&rec_title) {
                // Bidirectional substring. Score by length-ratio: when sizes
                // are close, the substring carries almost all the title's
                // meaning. When sizes are far apart, the longer side has a
                // lot of extra noise — still a hit, but weaker.
                let shorter = rec_title.len().min(title_l.len()) as f64;
                let longer = rec_title.len().max(title_l.len()) as f64;
                let ratio = if longer > 0.0 { shorter / longer } else { 1.0 };
                (60.0 + 30.0 * ratio) as i64
            } else {
                // Last-chance partial overlap: count shared whitespace-
                // separated word tokens (after normalization). Catches
                // cases like "Foo Bar" vs "Bar Foo" word-reorderings or
                // tracks where SMTC reports a different cleanup than the
                // LRCLib uploader did. Score 0-50 based on overlap fraction.
                let user_tokens: std::collections::HashSet<&str> =
                    title_l.split_whitespace().filter(|t| t.len() > 1).collect();
                let rec_tokens: std::collections::HashSet<&str> =
                    rec_title.split_whitespace().filter(|t| t.len() > 1).collect();
                if user_tokens.is_empty() || rec_tokens.is_empty() {
                    -1
                } else {
                    let shared = user_tokens.intersection(&rec_tokens).count();
                    let min_set = user_tokens.len().min(rec_tokens.len());
                    if shared == 0 {
                        -1
                    } else {
                        let frac = shared as f64 / min_set as f64;
                        (20.0 + 30.0 * frac) as i64
                    }
                }
            };

            if title_score < 0 {
                return (-1_000, r);
            }

            // --- Duration score --------------------------------------------
            let rec_secs = r.duration.unwrap_or(0.0) as i64;
            let duration_score: i64 = if requested_secs == 0 || rec_secs == 0 {
                15 // neutral — no signal either way
            } else {
                let diff = (rec_secs - requested_secs).abs();
                match diff {
                    0..=5 => 30,
                    6..=10 => 22,
                    11..=20 => 12,
                    21..=30 => 4,
                    _ => -50, // 31+s = probably a different recording
                }
            };

            // --- Artist score ----------------------------------------------
            let artist_score: i64 = if artist_l.is_empty() || rec_artist.is_empty() {
                0 // can't compare — neutral
            } else if rec_artist == artist_l {
                20
            } else if rec_artist.contains(&artist_l) || artist_l.contains(&rec_artist) {
                10
            } else {
                0
            };

            // --- Synced bonus ----------------------------------------------
            let synced_bonus = if r.synced_lyrics.is_some() { 20 } else { 0 };

            let total = title_score + duration_score + artist_score + synced_bonus;
            (total, r)
        })
        .filter(|(score, _)| *score >= THRESHOLD)
        .collect();

    // Highest score wins. Stable sort preserves the upstream order of
    // ties, which is roughly LRCLib's relevance order — close enough.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().next().map(|(_, r)| r)
}

fn to_cached_ref(rec: &LrcRecord) -> CachedLyrics {
    // Convenience: clone the bits we need without consuming the record.
    if rec.instrumental.unwrap_or(false) {
        return CachedLyrics::Instrumental;
    }
    if let Some(s) = rec.synced_lyrics.as_deref() {
        let lines = parse_lrc(s);
        if !lines.is_empty() {
            return CachedLyrics::Synced { lines, translation: None };
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
            return CachedLyrics::Synced { lines, translation: None };
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

// ─── SimpMusic fallback ────────────────────────────────────────────────────
//
// SimpMusic's API is YouTube-videoId-centric. It exposes /v1/search/title
// which returns a list of records matching the title. We filter client-side
// by artist similarity and duration ±5s, then prefer richSyncLyrics (word-
// level enhanced LRC) over plain syncedLyrics (line-level) when both exist.
// 30 req/min IP rate limit, no auth for read paths.

#[derive(Deserialize, Debug, Clone)]
struct SimpMusicWrapper {
    #[serde(default)]
    data: Vec<SimpMusicRecord>,
}

#[derive(Deserialize, Debug, Clone)]
struct SimpMusicRecord {
    #[serde(rename = "songTitle", default)]
    song_title: String,
    #[serde(rename = "artistName", default)]
    artist_name: String,
    #[serde(rename = "durationSeconds", default)]
    duration_seconds: i64,
    #[serde(rename = "plainLyric", default)]
    plain_lyric: String,
    #[serde(rename = "syncedLyrics", default)]
    synced_lyrics: String,
    #[serde(rename = "richSyncLyrics", default)]
    rich_sync_lyrics: String,
}

async fn fetch_simpmusic(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
    duration_ms: u64,
) -> Result<(CachedLyrics, String)> {
    let url = reqwest::Url::parse_with_params(
        "https://api-lyrics.simpmusic.org/v1/search/title",
        &[("title", title), ("limit", "10")],
    )
    .context("build simpmusic url")?;

    let resp = client.get(url).send().await.context("GET simpmusic")?;
    let status = resp.status();
    if !status.is_success() {
        if status.is_client_error() {
            return Ok((CachedLyrics::NotFound, "simpmusic".into()));
        }
        anyhow::bail!("simpmusic returned {status}");
    }
    let body = resp.text().await.context("read simpmusic body")?;
    let parsed: SimpMusicWrapper =
        serde_json::from_str(&body).context("parse simpmusic json")?;

    let chosen = pick_best_simpmusic(parsed.data, title, artist, duration_ms);
    let Some(rec) = chosen else {
        return Ok((CachedLyrics::NotFound, "simpmusic".into()));
    };

    // Prefer rich (word-level) when present + parseable, else line-level.
    if !rec.rich_sync_lyrics.trim().is_empty() {
        let lines = parse_enhanced_lrc(&rec.rich_sync_lyrics);
        if !lines.is_empty() {
            return Ok((
                CachedLyrics::Synced { lines, translation: None },
                "simpmusic".into(),
            ));
        }
    }
    if !rec.synced_lyrics.trim().is_empty() {
        let lines = parse_lrc(&rec.synced_lyrics);
        if !lines.is_empty() {
            return Ok((
                CachedLyrics::Synced { lines, translation: None },
                "simpmusic".into(),
            ));
        }
    }
    if !rec.plain_lyric.trim().is_empty() {
        return Ok((CachedLyrics::Plain { text: rec.plain_lyric }, "simpmusic".into()));
    }
    Ok((CachedLyrics::NotFound, "simpmusic".into()))
}

/// Score-based picker mirroring `pick_best` for LRCLib. Previously this
/// only filtered by artist + ±5s duration and IGNORED the title entirely,
/// which meant SimpMusic's broad title search returned the first record
/// within ±5s of the mashup audio's duration — for a "Twista x Wetter
/// (SW Mashup)" video at 227s, that happened to be Twista's "Wetter" at
/// 230s, and we surfaced its lyrics confidently misaligned against the
/// mashup playback. Same scoring shape as LRCLib but with rich-sync as
/// the highest-priority bonus since SimpMusic's edge is word-level
/// timing data.
fn pick_best_simpmusic(
    records: Vec<SimpMusicRecord>,
    title: &str,
    artist: &str,
    requested_duration_ms: u64,
) -> Option<SimpMusicRecord> {
    let title_l = normalize_for_match(title);
    let artist_l = normalize_for_match(artist);
    let requested_secs = (requested_duration_ms / 1000) as i64;

    const THRESHOLD: i64 = 80;

    let mut scored: Vec<(i64, SimpMusicRecord)> = records
        .into_iter()
        .map(|r| {
            let rec_title = normalize_for_match(&r.song_title);
            let rec_artist = normalize_for_match(&r.artist_name);

            // Title score — same shape as LRCLib's pick_best.
            let title_score: i64 = if rec_title.is_empty() {
                -1
            } else if rec_title == title_l {
                100
            } else if rec_title.contains(&title_l) || title_l.contains(&rec_title) {
                let shorter = rec_title.len().min(title_l.len()) as f64;
                let longer = rec_title.len().max(title_l.len()) as f64;
                let ratio = if longer > 0.0 { shorter / longer } else { 1.0 };
                (60.0 + 30.0 * ratio) as i64
            } else {
                let user_tokens: std::collections::HashSet<&str> =
                    title_l.split_whitespace().filter(|t| t.len() > 1).collect();
                let rec_tokens: std::collections::HashSet<&str> =
                    rec_title.split_whitespace().filter(|t| t.len() > 1).collect();
                if user_tokens.is_empty() || rec_tokens.is_empty() {
                    -1
                } else {
                    let shared = user_tokens.intersection(&rec_tokens).count();
                    let min_set = user_tokens.len().min(rec_tokens.len());
                    if shared == 0 {
                        -1
                    } else {
                        let frac = shared as f64 / min_set as f64;
                        (20.0 + 30.0 * frac) as i64
                    }
                }
            };

            if title_score < 0 {
                return (-1_000, r);
            }

            let duration_score: i64 = if requested_secs == 0 || r.duration_seconds == 0 {
                15
            } else {
                let diff = (r.duration_seconds - requested_secs).abs();
                match diff {
                    0..=5 => 30,
                    6..=10 => 22,
                    11..=20 => 12,
                    21..=30 => 4,
                    _ => -50,
                }
            };

            let artist_score: i64 = if artist_l.is_empty() || rec_artist.is_empty() {
                0
            } else if rec_artist == artist_l {
                20
            } else if rec_artist.contains(&artist_l) || artist_l.contains(&rec_artist) {
                10
            } else {
                0
            };

            // SimpMusic-specific lyric-quality bonus: rich (word-level)
            // beats plain synced beats plain text. Stronger than LRCLib's
            // synced bonus because SimpMusic's whole reason for being in
            // the cascade is the rich timing data.
            let lyric_bonus = if !r.rich_sync_lyrics.trim().is_empty() {
                25
            } else if !r.synced_lyrics.trim().is_empty() {
                20
            } else if !r.plain_lyric.trim().is_empty() {
                5
            } else {
                0
            };

            let total = title_score + duration_score + artist_score + lyric_bonus;
            (total, r)
        })
        .filter(|(score, _)| *score >= THRESHOLD)
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().next().map(|(_, r)| r)
}

// ─── NetEase fallback ──────────────────────────────────────────────────────
//
// NetEase Cloud Music's undocumented public API. Two-step:
//   1. POST /api/search/get with form body s=query, type=1 (songs) → song id
//   2. GET /api/song/lyric?id=X&lv=1&kv=1&tv=-1 → { lrc.lyric, tlyric.lyric }
//
// Cookie jar must be enabled (NMTID handshake). Some licensed tracks return
// empty `lrc.lyric` outside CN — treat that as NotFound.

const NETEASE_HEADERS: &[(&str, &str)] = &[
    ("Referer", "https://music.163.com"),
    (
        "User-Agent",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
    ),
];

#[derive(Deserialize, Debug)]
struct NeteaseSearchResp {
    #[serde(default)]
    code: i32,
    result: Option<NeteaseSearchResult>,
}

#[derive(Deserialize, Debug)]
struct NeteaseSearchResult {
    #[serde(default)]
    songs: Vec<NeteaseSong>,
}

#[derive(Deserialize, Debug)]
struct NeteaseSong {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    duration: u64,
    #[serde(default)]
    artists: Vec<NeteaseArtist>,
}

#[derive(Deserialize, Debug)]
struct NeteaseArtist {
    #[serde(default)]
    name: String,
}

#[derive(Deserialize, Debug)]
struct NeteaseLyricResp {
    #[serde(default)]
    code: i32,
    lrc: Option<NeteaseLyricBody>,
    tlyric: Option<NeteaseLyricBody>,
}

#[derive(Deserialize, Debug)]
struct NeteaseLyricBody {
    #[serde(default)]
    lyric: String,
}

async fn fetch_netease(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
    duration_ms: u64,
) -> Result<(CachedLyrics, String)> {
    let query = format!("{title} {artist}");
    // reqwest's RequestBuilder::form gates on a default feature that's been
    // problematic to enable cleanly; sidestep by manually building the urlen-
    // coded body via Url::query_pairs_mut (always available, no extra dep).
    let body = {
        let mut u = reqwest::Url::parse("https://example.invalid/")
            .context("build form-body url")?;
        u.query_pairs_mut()
            .append_pair("s", &query)
            .append_pair("type", "1")
            .append_pair("limit", "10")
            .append_pair("offset", "0");
        u.query().unwrap_or("").to_string()
    };
    let mut req = client
        .post("https://music.163.com/api/search/get")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body);
    for (k, v) in NETEASE_HEADERS {
        req = req.header(*k, *v);
    }
    let resp = req.send().await.context("POST netease search")?;
    let status = resp.status();
    if !status.is_success() {
        if status.is_client_error() {
            return Ok((CachedLyrics::NotFound, "netease".into()));
        }
        anyhow::bail!("netease search returned {status}");
    }
    let body = resp.text().await.context("read netease search body")?;
    let parsed: NeteaseSearchResp =
        serde_json::from_str(&body).context("parse netease search json")?;
    if parsed.code != 200 {
        return Ok((CachedLyrics::NotFound, "netease".into()));
    }
    let songs = parsed.result.map(|r| r.songs).unwrap_or_default();
    let Some(song) = pick_best_netease(songs, artist, title, duration_ms) else {
        return Ok((CachedLyrics::NotFound, "netease".into()));
    };

    let song_id = song.id.to_string();
    let lyric_url = reqwest::Url::parse_with_params(
        "https://music.163.com/api/song/lyric",
        &[
            ("id", song_id.as_str()),
            ("lv", "1"),
            ("kv", "1"),
            ("tv", "-1"),
        ],
    )
    .context("build netease lyric url")?;
    let mut req = client.get(lyric_url);
    for (k, v) in NETEASE_HEADERS {
        req = req.header(*k, *v);
    }
    let resp = req.send().await.context("GET netease lyric")?;
    let status = resp.status();
    if !status.is_success() {
        if status.is_client_error() {
            return Ok((CachedLyrics::NotFound, "netease".into()));
        }
        anyhow::bail!("netease lyric returned {status}");
    }
    let body = resp.text().await.context("read netease lyric body")?;
    let parsed: NeteaseLyricResp =
        serde_json::from_str(&body).context("parse netease lyric json")?;
    if parsed.code != 200 {
        return Ok((CachedLyrics::NotFound, "netease".into()));
    }
    let lrc = parsed.lrc.map(|l| l.lyric).unwrap_or_default();
    if lrc.trim().is_empty() {
        return Ok((CachedLyrics::NotFound, "netease".into()));
    }
    let lines = parse_lrc(&lrc);
    if lines.is_empty() {
        return Ok((CachedLyrics::NotFound, "netease".into()));
    }
    let translation = parsed
        .tlyric
        .map(|t| t.lyric)
        .filter(|t| !t.trim().is_empty())
        .map(|t| parse_lrc(&t))
        .filter(|t| !t.is_empty());
    Ok((CachedLyrics::Synced { lines, translation }, "netease".into()))
}

fn pick_best_netease(
    songs: Vec<NeteaseSong>,
    artist: &str,
    title: &str,
    requested_duration_ms: u64,
) -> Option<NeteaseSong> {
    let artist_l = artist.trim().to_lowercase();
    let title_l = title.trim().to_lowercase();
    let tolerance_ms: i64 = 5_000;

    let mut candidates: Vec<NeteaseSong> = songs
        .into_iter()
        .filter(|s| {
            let s_title = s.name.trim().to_lowercase();
            // Equivalent to `!empty && !(contains_a || contains_b)` —
            // De Morgan's: skip when title is present AND doesn't bidirectional-
            // substring-match. Empty titles pass through this gate and get
            // filtered by the artist/duration gates further down.
            if !(s_title.is_empty() || s_title.contains(&title_l) || title_l.contains(&s_title)) {
                return false;
            }
            if !artist_l.is_empty() {
                let any_artist_match = s
                    .artists
                    .iter()
                    .any(|a| {
                        let a_l = a.name.trim().to_lowercase();
                        !a_l.is_empty()
                            && (a_l.contains(&artist_l) || artist_l.contains(&a_l))
                    });
                if !any_artist_match {
                    return false;
                }
            }
            if requested_duration_ms == 0 {
                return true;
            }
            (s.duration as i64 - requested_duration_ms as i64).abs() <= tolerance_ms
        })
        .collect();

    candidates.sort_by_key(|s| {
        if requested_duration_ms == 0 {
            0
        } else {
            (s.duration as i64 - requested_duration_ms as i64).abs()
        }
    });
    candidates.into_iter().next()
}

// ─── Title cleaner ─────────────────────────────────────────────────────────

fn cleaner() -> &'static Regex {
    static C: OnceLock<Regex> = OnceLock::new();
    C.get_or_init(|| {
        // (?ix) = case-insensitive + ignore whitespace inside the pattern.
        //
        // Structural rule for the video/audio/visualizer terminals: accept
        // ANY sequence of words before the noise terminal — `(?:[\w'\-]+\s+)*
        // video` matches `(Video)`, `(Official Video)`, `(Official Music
        // Video)`, `(Official 4K Video)`, `(Official Animated 8K HD Music
        // Video)`, `(Live 1080p Audio)` and every other uploader fashion
        // ending in those words.
        //
        // The previous version enumerated the allowed modifiers — `(?:music
        // \s+|lyric\s+|hd\s+|animated\s+)?video` — which broke whenever
        // a new quality token appeared mid-string. Real failure cases:
        // `(Official 4K Video)` (4K not in allowlist), `(Official 60fps
        // Music Video)` (60fps not in allowlist). Loosening to "any words
        // before video/audio/visualizer" makes the cleaner robust to new
        // tokens without needing a regex patch each time.
        //
        // The remaining alternatives (lyrics, feat./ft., remaster, live at,
        // demo, acoustic, edit, etc.) stay as bounded vocabularies — they
        // genuinely are finite sets and don't benefit from loosening.
        Regex::new(
            r"(?ix)
              \s*[\[\(]\s*
              (?:
                  (?:[\w'\-]+\s+)*video |
                  (?:[\w'\-]+\s+)*audio |
                  (?:[\w'\-]+\s+)*visualizer |
                  lyrics? |
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
                  hd | uhd | mv | 1080p | 1440p | 2160p | 60fps | 30fps | hq
              )
              \s*[\]\)]
            ",
        )
        .unwrap()
    })
}

// Trailing pipe-delimited tags ("Song | Lyrics", "Song | Official Video",
// "Song | Music Video", etc.) are an extremely common YouTube uploader
// convention for lyric / promo videos. The bracketed `cleaner()` above
// misses these because they sit outside `[]` / `()`. Stripped from the
// END of the title only — interior pipes (e.g. "Hard Out Here | Live at
// Glastonbury") are left alone.
fn pipe_tag_cleaner() -> &'static Regex {
    static C: OnceLock<Regex> = OnceLock::new();
    C.get_or_init(|| {
        Regex::new(
            r"(?ix)
              \s*\|\s*
              (?:
                  (?:official\s+)?(?:music\s+|lyric\s+|hd\s+|animated\s+)?video |
                  (?:official\s+)?(?:music\s+)?audio |
                  (?:official\s+)?visualizer |
                  music\s+video |
                  lyric\s+video |
                  lyrics? |
                  hd | uhd | mv | 4k | 8k
              )
              \s*$
            ",
        )
        .unwrap()
    })
}

pub fn clean_title(title: &str) -> String {
    // 1. Strip trailing video/audio file extensions. YouTube uploads of legacy
    //    files often keep the original filename verbatim — e.g.
    //    `"Follow Me Uncle Kracker Lyrics.wmv"`. The extension shields the
    //    trailing `Lyrics` token from `bare_trailing_tag_cleaner` (which
    //    requires `\s+Lyrics\s*$`), so the whole uploader-chrome suffix
    //    survives every other cleaner. Stripping first lets the rest of
    //    the pipeline see the real title. Vocabulary is restricted to
    //    real media container extensions — no canonical released song has
    //    one of these in its title.
    let cleaned = file_extension_stripper().replace(title, "").to_string();
    // 2. Strip trailing YouTube lyric-channel quote excerpt. Channels like
    //    BangersOnly bait clicks by appending a memorable line in quotes
    //    after the real title — e.g. `Beautiful Things (Lyrics) "i want
    //    you i need you oh god"`. Nothing else in the cleaner pipeline
    //    touched these, so the quoted suffix tanked the title score in
    //    `pick_best`'s length-ratio path. Stripping first lets the rest
    //    of the pipeline see the real song title.
    let cleaned = trailing_quote_stripper().replace(&cleaned, "$1").to_string();
    // 3. Strip parenthetical / bracketed noise tags.
    let cleaned = cleaner().replace_all(&cleaned, "").to_string();
    // 4. Strip trailing pipe-separated tags.
    let cleaned = pipe_tag_cleaner().replace_all(&cleaned, "").to_string();
    // 5. Strip BARE trailing uploader-chrome tags — same vocabulary the
    //    bracketed cleaner catches, but without any surrounding `[]` / `()` /
    //    `|`. Real failure case: YouTube video titled `"Shaggy - Angel
    //    Lyrics"` reaches the resolver with the whole string in the title
    //    field; bracketed/pipe cleaners don't touch it, and the trailing
    //    bare word `Lyrics` poisons the LRCLib search query enough to miss
    //    even the most popular songs. Stripping it here makes the first-pass
    //    /api/search query match canonical records, and lets the retry path
    //    (`strip_youtube_noise`) see a clean title before stripping the
    //    leading `"Shaggy - "` channel prefix.
    let cleaned = bare_trailing_tag_cleaner().replace(&cleaned, "$1").to_string();
    cleaned.trim().to_string()
}

// Trailing media file extension — `.wmv`, `.mp4`, etc. Triggered by real
// YouTube uploads named after the source file: "Follow Me Uncle Kracker
// Lyrics.wmv". Match requires a dot + a known media-container extension at
// the end of the title (allowing trailing whitespace). Vocabulary is
// restricted to common video + audio container extensions; nothing
// ambiguous like `.live` or `.remix`. No canonical released song title
// contains one of these — the safe-strip bar is the same as v0.10.24's
// bare-trailing-tag cleaner.
fn file_extension_stripper() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?ix)
              \.
              (?:
                  wmv | mp4 | mkv | avi | mov | webm | flv | m4v | mpg | mpeg |
                  mp3 | wav | flac | m4a | aac | ogg | opus
              )
              \s*$
            ",
        )
        .unwrap()
    })
}

// Bare trailing uploader-chrome tags — `Lyrics`, `Lyric Video`, `Music Video`,
// `Official Music Video`, `Official Audio`, `Official Visualizer`, plus quality
// markers (`HD`, `UHD`, `4K`, `8K`, `1080p`, `1440p`, `2160p`). Matched as a
// repeated trailing group so compound tags like `"Song HD 4K Music Video"`
// collapse to `"Song"` in one pass. The capturing `(.*?\S)` is non-greedy so
// the regex consumes the *most* trailing tags rather than the fewest. Requires
// at least one non-whitespace char before the first tag, so a title that IS
// the bare tag (e.g. just `"Lyrics"` or `"Music Video"`) is preserved intact.
//
// Vocabulary is deliberately narrower than `cleaner()` — only the chrome words
// safe to strip without brackets. Bare `Audio` / `Visualizer` / `MV` / `HQ`
// without an `Official` qualifier are skipped because they appear in legit
// song titles often enough to risk false positives.
fn bare_trailing_tag_cleaner() -> &'static Regex {
    static C: OnceLock<Regex> = OnceLock::new();
    C.get_or_init(|| {
        Regex::new(
            r"(?ix)
              (.*?\S)
              (?:
                  \s+
                  (?:
                      lyrics? |
                      lyric\s+video |
                      official\s+lyric\s+video |
                      music\s+video |
                      official\s+(?:music\s+)?video |
                      official\s+audio |
                      official\s+visualizer |
                      hd | uhd | 4k | 8k | 1080p | 1440p | 2160p
                  )
              )+
              \s*$
            ",
        )
        .unwrap()
    })
}

// Trailing YouTube lyric-channel quote excerpts ("i want you i need you oh
// god"). Match requires non-whitespace + whitespace before the opening quote
// so legit fully-quoted titles like Macklemore's `"Same Love"` (no leading
// content) are left alone. Replace with the captured `\S` to preserve the
// last char of the real title. Handles both ASCII `"..."` and curly
// `\u{201C}...\u{201D}` quotes — uploaders use both, sometimes in the same
// title (curly opening, ASCII closing) because YouTube's smart-quote pass
// is inconsistent.
fn trailing_quote_stripper() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            "(?x)\
              (\\S)\\s+\
              [\"\u{201C}]\
              [^\"\u{201C}\u{201D}]+\
              [\"\u{201D}]\
              \\s*$\
            ",
        )
        .unwrap()
    })
}

// ─── Artist cleaner ────────────────────────────────────────────────────────
//
// YouTube auto-generated channels and uploader chrome poison the SMTC artist
// field in predictable ways. LRCLib's exact-match `/api/get` rejects them and
// search results are noisier than they should be. We strip:
//   - trailing " - Topic"          (YT auto-generated Topic channels)
//   - trailing " VEVO"             (e.g. "ArtistVEVO")
//   - trailing " - Official Artist Channel"
//   - trailing " - Official"
//   - trailing " (Official Artist Channel)" / "(Official)"
//   - leading/trailing dashes and whitespace
//
// We do NOT touch interior text — only suffix-style noise — so legitimate
// hyphenated band names ("Crosby, Stills, Nash & Young", "Earth, Wind & Fire")
// stay intact.

fn artist_cleaner() -> &'static Regex {
    static C: OnceLock<Regex> = OnceLock::new();
    C.get_or_init(|| {
        Regex::new(
            r"(?ix)
              (?:
                  \s*-\s*Topic |
                  \s*-\s*Official\s+Artist\s+Channel |
                  \s*-\s*Official |
                  \s*\(\s*Official\s+Artist\s+Channel\s*\) |
                  \s*\(\s*Official\s*\) |
                  \s*\[\s*Topic\s*\] |
                  \s*VEVO
              )
              \s*$
            ",
        )
        .unwrap()
    })
}

pub fn clean_artist(artist: &str) -> String {
    let stripped = artist_cleaner().replace(artist, "").to_string();
    stripped.trim().trim_matches('-').trim().to_string()
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
            lines.push(LyricLine { time_ms: t, text: text.clone(), words: None });
        }
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

// Parses SimpMusic-style enhanced LRC, where each line is a sequence of
// `<mm:ss.xx>word` segments (optionally prefixed with a `[mm:ss.xx]` line
// timestamp). Produces line-level entries with attached word-level timing.
//
// Example input line:
//   `[00:08.10]<00:08.10>Fonsi <00:08.33>DY`
// or
//   `<00:08.10>Fonsi <00:08.33>DY`
pub fn parse_enhanced_lrc(s: &str) -> Vec<LyricLine> {
    let line_re = ts_re();
    let word_re: &OnceLock<Regex> = {
        static R: OnceLock<Regex> = OnceLock::new();
        &R
    };
    let word_re = word_re.get_or_init(|| {
        Regex::new(r"<(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?>").unwrap()
    });

    let mut lines: Vec<LyricLine> = Vec::new();
    for raw in s.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut rest = trimmed;

        // Optional leading line-level timestamp
        let line_time: Option<u32> = if let Some(cap) = line_re.captures(rest) {
            let mm: u32 = cap[1].parse().unwrap_or(0);
            let ss: u32 = cap[2].parse().unwrap_or(0);
            let frac: u32 = cap.get(3).map_or(0, |m| frac_to_ms(m.as_str()));
            let consumed = cap[0].len();
            rest = &rest[consumed..];
            Some(mm.saturating_mul(60_000).saturating_add(ss * 1_000).saturating_add(frac))
        } else {
            None
        };

        // Walk through `<time>word <time>word` segments.
        let mut words: Vec<WordSpan> = Vec::new();
        let mut text_acc = String::new();
        let mut cursor = rest;
        while let Some(cap) = word_re.captures(cursor) {
            let m = cap.get(0).unwrap();
            let start = m.start();
            let end = m.end();
            // Any text BEFORE this marker (rare, usually a line-level prefix
            // word) — append to accumulator at the prior word time, or as
            // text-only if no prior word exists.
            if start > 0 {
                let prefix = &cursor[..start];
                if !prefix.is_empty() {
                    text_acc.push_str(prefix);
                    if let Some(last) = words.last_mut() {
                        last.text.push_str(prefix);
                    }
                }
            }
            let mm: u32 = cap[1].parse().unwrap_or(0);
            let ss: u32 = cap[2].parse().unwrap_or(0);
            let frac: u32 = cap.get(3).map_or(0, |m| frac_to_ms(m.as_str()));
            let t = mm.saturating_mul(60_000).saturating_add(ss * 1_000).saturating_add(frac);

            // Word text = chars between this marker and the next `<` (or eol).
            let after = &cursor[end..];
            let next_lt = after.find('<').unwrap_or(after.len());
            let word_text = after[..next_lt].to_string();
            text_acc.push_str(&word_text);
            words.push(WordSpan { time_ms: t, text: word_text });

            cursor = &after[next_lt..];
        }

        if words.is_empty() {
            continue;
        }
        let line_t = line_time.unwrap_or_else(|| words[0].time_ms);
        lines.push(LyricLine {
            time_ms: line_t,
            text: text_acc.trim().to_string(),
            words: Some(words),
        });
    }
    lines.sort_by_key(|l| l.time_ms);
    lines
}

fn frac_to_ms(s: &str) -> u32 {
    let n: u32 = s.parse().unwrap_or(0);
    match s.len() {
        1 => n * 100,
        2 => n * 10,
        _ => n,
    }
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
    let cached: CachedLyrics = serde_json::from_value(v).ok()?;
    // Discard any persisted NotFound entries — the lyric-finding algorithm
    // keeps evolving (new YouTube-noise patterns, punctuation normalization,
    // pick_best refinements), so a NotFound cached under a previous version
    // shouldn't lock the user out of a fresh fetch under the new logic.
    // Successful matches (Synced / Plain / Instrumental) stay cached forever
    // because their content doesn't depend on resolver heuristics.
    if matches!(cached, CachedLyrics::NotFound | CachedLyrics::Unsupported) {
        return None;
    }
    Some(cached)
}

fn write_store(app: &AppHandle, key: &str, cached: &CachedLyrics) {
    // Don't persist NotFound or Unsupported — these are session-local
    // states that should re-evaluate on every replay. Caching them would
    // mask both Hum's own improvements (resolver tweaks between releases)
    // and improvements in the upstream source (Pandora finally adding
    // Media Session metadata).
    if matches!(cached, CachedLyrics::NotFound | CachedLyrics::Unsupported) {
        return;
    }
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
        // ── Baseline (established v0.10.7+ behavior) ─────────────────────
        assert_eq!(clean_title("Apocalypse (Official Video)"), "Apocalypse");
        assert_eq!(clean_title("Apocalypse [Lyrics]"), "Apocalypse");
        assert_eq!(clean_title("Hey Jude (Remastered 2009)"), "Hey Jude");
        assert_eq!(clean_title("Sweet Caroline (feat. Someone)"), "Sweet Caroline");
        assert_eq!(clean_title("Test Song [HD] (4K)"), "Test Song");
        assert_eq!(clean_title("Track Name (Live at Wembley)"), "Track Name");
        assert_eq!(clean_title("Plain Title"), "Plain Title");

        // ── v0.10.11 — Official Audio variants ───────────────────────────
        assert_eq!(clean_title("Dreams (Official Audio)"), "Dreams");
        assert_eq!(clean_title("Dreams (Official Visualizer)"), "Dreams");
        assert_eq!(clean_title("Dreams (Official Animated Video)"), "Dreams");

        // ── v0.10.21 — flexible modifier alternation ─────────────────────
        //
        // Previously the cleaner only accepted (music|lyric|hd|animated)
        // before `video`; quality tokens like 4K / 8K / 60fps in the middle
        // of the parenthetical caused the whole tag to survive.
        //
        // Real-world failure: "Train - Drops Of Jupiter (Tell Me) (Official
        // 4K Video)" left `(Official 4K Video)` intact → length-ratio score
        // < 80 → no lyrics.
        assert_eq!(
            clean_title("Train - Drops Of Jupiter (Tell Me) (Official 4K Video)"),
            "Train - Drops Of Jupiter (Tell Me)"
        );
        assert_eq!(clean_title("Song (Official 8K Video)"), "Song");
        assert_eq!(clean_title("Track (Official 60fps Music Video)"), "Track");
        assert_eq!(clean_title("Song (Official Animated 4K Music Video)"), "Song");
        assert_eq!(clean_title("X [Official 1080p HD Music Video]"), "X");
        assert_eq!(clean_title("Y (Live 4K UHD Audio)"), "Y");
        assert_eq!(clean_title("Z (Official Animated Visualizer)"), "Z");
        assert_eq!(clean_title("A (HQ Audio)"), "A");
        assert_eq!(clean_title("B (2160p Music Video)"), "B");

        // ── v0.10.21 — trailing quote excerpt strip ──────────────────────
        //
        // YouTube lyric channels (BangersOnly, et al.) append a memorable
        // line in quotes after the real title to bait clicks. Real-world
        // failure: "Beautiful Things (Lyrics) \"i want you i need you oh
        // god\"" — quoted suffix survived, length ratio tanked the score.
        assert_eq!(
            clean_title("Benson Boone - Beautiful Things (Lyrics) \"i want you i need you oh god\""),
            "Benson Boone - Beautiful Things"
        );
        assert_eq!(
            clean_title("Plain Title \"with a quoted suffix\""),
            "Plain Title"
        );
        // Curly quotes (uploaders inconsistently smart-quote)
        assert_eq!(
            clean_title("Plain Title \u{201C}smart quoted suffix\u{201D}"),
            "Plain Title"
        );
        // Mixed curly + ASCII (also seen in the wild)
        assert_eq!(
            clean_title("Plain Title \u{201C}mixed quoted\""),
            "Plain Title"
        );

        // ── v0.10.21 — quote-stripper safeguards ─────────────────────────
        //
        // Fully-quoted titles (no leading non-quote content) must survive
        // intact — `Macklemore - "Same Love"` is the canonical example.
        // The artist `Macklemore - ` lives in the artist field separately;
        // the title shown here is just the song's quoted name.
        assert_eq!(clean_title("\"Same Love\""), "\"Same Love\"");
        assert_eq!(clean_title("\u{201C}Same Love\u{201D}"), "\u{201C}Same Love\u{201D}");

        // ── v0.10.21 — combined: cleaner runs AFTER quote-strip ──────────
        //
        // Ensure both layers compose: trailing quote AND trailing paren
        // noise both get cleaned, leaving the bare title.
        assert_eq!(
            clean_title("Song (Official 4K Video) \"quoted suffix\""),
            "Song"
        );
        assert_eq!(
            clean_title("Song (Tell Me) (Official 4K Video) \"the hook\""),
            "Song (Tell Me)"
        );

        // ── v0.10.24 — bare trailing uploader-chrome tags ────────────────
        //
        // Real-world failure case: YouTube uploader titled the video
        // `"Shaggy - Angel Lyrics"`. The whole string landed in SMTC's
        // title field (artist field was unrelated/empty), and the bare
        // trailing `Lyrics` survived every previous cleaner because there
        // were no brackets, parens, or pipe. After this slice the bare
        // tag strips before the retry path even sees the title, so the
        // first-pass LRCLib /api/search query matches Shaggy's "Angel"
        // canonically.
        assert_eq!(clean_title("Angel Lyrics"), "Angel");
        assert_eq!(clean_title("Beautiful Things Lyrics"), "Beautiful Things");
        assert_eq!(clean_title("Shaggy - Angel Lyrics"), "Shaggy - Angel");
        assert_eq!(clean_title("Some Song Lyric Video"), "Some Song");
        assert_eq!(clean_title("Track Music Video"), "Track");
        assert_eq!(clean_title("Track Official Music Video"), "Track");
        assert_eq!(clean_title("Track Official Video"), "Track");
        assert_eq!(clean_title("Track Official Audio"), "Track");
        assert_eq!(clean_title("Track Official Visualizer"), "Track");
        // Quality markers
        assert_eq!(clean_title("Song HD"), "Song");
        assert_eq!(clean_title("Song UHD"), "Song");
        assert_eq!(clean_title("Song 4K"), "Song");
        assert_eq!(clean_title("Song 8K"), "Song");
        assert_eq!(clean_title("Song 1080p"), "Song");
        // Compound trailing tags strip in one pass
        assert_eq!(clean_title("Song HD 4K"), "Song");
        assert_eq!(clean_title("Song HD 4K Music Video"), "Song");
        assert_eq!(clean_title("Song Official Music Video HD"), "Song");
        // Preserve titles that ARE the bare tag (single word, no preceding
        // content) — songs literally titled "Lyrics", "Music Video", etc.
        assert_eq!(clean_title("Lyrics"), "Lyrics");
        assert_eq!(clean_title("Music Video"), "Music Video");
        // Preserve case where the trailing token isn't in our safe-strip
        // vocabulary (bare `Audio` / `Visualizer` / `MV` / `HQ` without
        // `Official` qualifier — too risky for false positives).
        assert_eq!(clean_title("Song Audio"), "Song Audio");
        assert_eq!(clean_title("Song MV"), "Song MV");
        assert_eq!(clean_title("Song HQ"), "Song HQ");
        // Compose with bracketed cleaner — bare tag inside parens still works
        assert_eq!(clean_title("Angel (Lyrics)"), "Angel");
        // Compose with bracketed cleaner where bracketed AND bare appear
        assert_eq!(clean_title("Angel (HD) Lyrics"), "Angel");

        // ── v0.10.25 — trailing video/audio file extensions ──────────────
        //
        // Real-world failure case: YouTube upload titled "Follow Me Uncle
        // Kracker Lyrics.wmv". The `.wmv` shielded the bare trailing
        // `Lyrics` from v0.10.24's `bare_trailing_tag_cleaner` (which
        // requires `\s+Lyrics\s*$`), so the whole uploader-chrome suffix
        // survived. Now the extension strips first, then v0.10.24's bare-
        // tag cleaner runs on a clean trailing `Lyrics`.
        assert_eq!(
            clean_title("Follow Me Uncle Kracker Lyrics.wmv"),
            "Follow Me Uncle Kracker"
        );
        // Video container extensions
        assert_eq!(clean_title("Song.wmv"), "Song");
        assert_eq!(clean_title("Song.mp4"), "Song");
        assert_eq!(clean_title("Song.mkv"), "Song");
        assert_eq!(clean_title("Song.avi"), "Song");
        assert_eq!(clean_title("Song.mov"), "Song");
        assert_eq!(clean_title("Song.webm"), "Song");
        assert_eq!(clean_title("Song.flv"), "Song");
        assert_eq!(clean_title("Song.m4v"), "Song");
        assert_eq!(clean_title("Song.mpg"), "Song");
        assert_eq!(clean_title("Song.mpeg"), "Song");
        // Audio container extensions
        assert_eq!(clean_title("Song.mp3"), "Song");
        assert_eq!(clean_title("Song.wav"), "Song");
        assert_eq!(clean_title("Song.flac"), "Song");
        assert_eq!(clean_title("Song.m4a"), "Song");
        assert_eq!(clean_title("Song.aac"), "Song");
        assert_eq!(clean_title("Song.ogg"), "Song");
        assert_eq!(clean_title("Song.opus"), "Song");
        // Case-insensitive
        assert_eq!(clean_title("Song.WMV"), "Song");
        assert_eq!(clean_title("Song.Mp4"), "Song");
        // Trailing whitespace after the extension
        assert_eq!(clean_title("Song.wmv  "), "Song");
        // Compose with bare-tag cleaner — extension strips first, then bare
        // tag cleaner sees the cleaned trailing keyword.
        assert_eq!(clean_title("Angel Lyrics.wmv"), "Angel");
        assert_eq!(clean_title("Song Official Music Video.mp4"), "Song");
        // Compose with bracketed cleaner
        assert_eq!(clean_title("Song (Official Video).mkv"), "Song");
        // Preserve titles where the only "extension" is part of a real word
        // — no token with a recognized extension should slip through. (No
        // false positives expected; vocabulary is restricted to file-format
        // container extensions, none of which look like English words.)
        assert_eq!(clean_title("Plain Title"), "Plain Title");
        // Extension in the middle of the title (not trailing) is left alone
        assert_eq!(clean_title("Song.mp3 (Live)"), "Song.mp3");
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

    #[test]
    fn parses_enhanced_lrc_word_level() {
        // SimpMusic richSyncLyrics format — `<mm:ss.xx>word` segments
        let s = "<00:01.00>Hello <00:01.50>world\n<00:03.00>Second <00:03.40>line";
        let lines = parse_enhanced_lrc(s);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].time_ms, 1_000);
        assert_eq!(lines[0].text, "Hello world");
        let words = lines[0].words.as_ref().unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].time_ms, 1_000);
        assert_eq!(words[0].text, "Hello ");
        assert_eq!(words[1].time_ms, 1_500);
        assert_eq!(words[1].text, "world");
        assert_eq!(lines[1].time_ms, 3_000);
        assert_eq!(lines[1].text, "Second line");
    }

    #[test]
    fn parses_enhanced_lrc_with_line_prefix() {
        // Some sources include a leading [mm:ss.xx] line timestamp before
        // the per-word `<mm:ss.xx>` markers.
        let s = "[00:08.10]<00:08.10>Fonsi <00:08.33>DY";
        let lines = parse_enhanced_lrc(s);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].time_ms, 8_100);
        let words = lines[0].words.as_ref().unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(words[1].time_ms, 8_330);
    }

    #[test]
    fn enhanced_lrc_skips_empty_lines() {
        let s = "\n\n<00:01.00>only line\n\n";
        let lines = parse_enhanced_lrc(s);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].time_ms, 1_000);
    }
}
