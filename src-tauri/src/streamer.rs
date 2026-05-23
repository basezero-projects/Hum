//! OBS / browser-source HTTP server for the lyrics overlay.
//!
//! Spawns an axum server bound to 127.0.0.1:<port> that exposes:
//!
//! - `GET  /state`   — JSON snapshot of current track + lyrics + cursor.
//!   Stateless poll endpoint; used as a fallback when the SSE stream is
//!   unavailable and by external tools that want a one-shot read.
//! - `GET  /events`  — Server-Sent Events stream. Pushes the same state
//!   payload as `/state` whenever any change-relevant field flips (track,
//!   lyrics status, cursor, ad_active, playback state, album art). Position
//!   ticks are NOT pushed — the client interpolates locally from
//!   `position_ms + (now - last_update_unix_ms)` so the progress bar
//!   advances smoothly without the server flooding the wire.
//! - `GET  /art`     — Current album art image bytes. Decoded from the
//!   `data:image/...` URL the desktop fetch chain produces, with the right
//!   Content-Type so `<img src="/art">` Just Works.
//! - `GET  /overlay` — Self-contained HTML page rendering the same chrome
//!   (album art, metadata, progress bar, source badge, gold dashed border)
//!   as the desktop overlay. Background is fully transparent so OBS
//!   browser-source layering needs no chroma-key tricks.
//! - `GET  /healthz` — Minimal liveness probe ("ok").
//!
//! The server is gated by `settings.streamer_enabled` — when off, no
//! port is bound. When toggled on at runtime via `update_settings`, a
//! new server task is spawned. When toggled off, the task's shutdown
//! signal fires and the port is freed.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Json, Router,
};
use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

use crate::lyrics::{CurrentLyrics, SharedLyrics};
use crate::settings::SharedSettings;
use crate::smtc::{CurrentTrack, SharedAlbumArt, SharedSnapshot};

#[derive(Clone)]
struct AppState {
    snapshot: SharedSnapshot,
    lyrics: SharedLyrics,
    art: SharedAlbumArt,
    settings: SharedSettings,
}

/// Combined snapshot returned by /state and pushed by /events. Frontend
/// (the embedded overlay HTML, or any third-party consumer) renders from
/// this.
#[derive(Clone, Serialize)]
struct StateResponse {
    track: CurrentTrack,
    lyrics: CurrentLyrics,
    /// Server-side computed cursor (which line index is currently active).
    /// Saves clients from re-implementing the rAF interpolation logic.
    cursor: i32,
    /// Server-side wall clock (unix ms). Lets clients compute
    /// "interpolated position = track.position_ms + (now - server_now_ms +
    /// (track.last_update_unix_ms - server_now_ms))" if they want
    /// sub-poll-tick accuracy.
    server_now_ms: i64,
    /// Stable key for the current art payload. When this changes, the
    /// browser source's `<img>` element should re-request `/art` (e.g.
    /// `src="/art?k={art_key}"`). Empty when no art is currently cached.
    art_key: String,
    /// User-facing source label derived from `source_app_id`. Mirrors the
    /// labelling in `src/Overlay.tsx::sourceLabel`. Empty when unknown.
    source_label: String,
}

async fn build_state(s: &AppState) -> StateResponse {
    let snap = s.snapshot.read().await.clone();
    let lyrics = s.lyrics.read().await.clone();
    let anticipate_ms = { s.settings.read().await.anticipate_ms };

    let now_ms = unix_ms_now();
    let pos_ms = if snap.state == crate::smtc::PlaybackState::Playing {
        let elapsed = (now_ms - snap.last_update_unix_ms).max(0);
        snap.position_ms.saturating_add(elapsed as u64)
    } else {
        snap.position_ms
    };
    // Apply the global lyric offset the same way the live overlay does
    // (`src/Overlay.tsx::lookupPositionMs`). Positive = lyrics show
    // earlier; negative = lyrics show later. Saturate at 0 so a large
    // negative offset on a track playing from the start can't underflow
    // the u64 lookup.
    let lookup_pos_ms = if anticipate_ms >= 0 {
        pos_ms.saturating_add(anticipate_ms as u64)
    } else {
        pos_ms.saturating_sub((-anticipate_ms) as u64)
    };

    let mut cursor: i32 = -1;
    if matches!(lyrics.status, crate::lyrics::Status::Synced) {
        for (i, line) in lyrics.lines.iter().enumerate() {
            if line.time_ms as u64 <= lookup_pos_ms {
                cursor = i as i32;
            } else {
                break;
            }
        }
    }

    let (art_key, _) = {
        let art = s.art.read().await;
        match &*art {
            Some(a) => (format!("{}|{}", a.artist, a.title), true),
            None => (String::new(), false),
        }
    };

    let source_label = source_label_for(snap.source_app_id.as_deref().unwrap_or(""));

    StateResponse {
        track: snap,
        lyrics,
        cursor,
        server_now_ms: now_ms,
        art_key,
        source_label,
    }
}

/// Maps `source_app_id` (e.g. `Spotify.exe`, `chrome.exe`) to a
/// presentable label. Mirrors `sourceLabel` in `src/Overlay.tsx`.
fn source_label_for(app_id: &str) -> String {
    let lower = app_id.to_lowercase();
    if lower.is_empty() {
        return String::new();
    }
    if lower.contains("spotify") {
        return "Spotify".into();
    }
    if lower.contains("pandora") {
        return "Pandora".into();
    }
    if lower.contains("itunes") {
        return "iTunes".into();
    }
    if lower.contains("apple") && lower.contains("music") {
        return "Apple Music".into();
    }
    if lower.contains("apple") {
        return "Apple Music".into();
    }
    if lower.contains("youtube") {
        return "YouTube Music".into();
    }
    if lower.contains("chrome") || lower.contains("edge") || lower.contains("firefox") {
        return "Browser".into();
    }
    // Strip ".exe" + capitalize first char as a fallback.
    let stem = app_id.strip_suffix(".exe").unwrap_or(app_id);
    let mut chars = stem.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Fingerprint of the change-relevant fields. Used by `/events` to
/// suppress pushes that would only carry a position-tick (the client
/// interpolates the progress bar locally). Two state snapshots with the
/// same fingerprint render identically apart from the progress bar's
/// elapsed milliseconds.
fn change_fingerprint(s: &StateResponse) -> String {
    let lines_hash = s.lyrics.line_count;
    format!(
        "{}|{}|{}|{}|{:?}|{:?}|{}|{}|{}|{}",
        s.track.title,
        s.track.artist,
        s.track.album,
        s.track.ad_active,
        s.track.state,
        s.lyrics.status,
        lines_hash,
        s.cursor,
        s.art_key,
        s.lyrics.source.as_deref().unwrap_or(""),
    )
}

async fn get_state(State(s): State<AppState>) -> impl IntoResponse {
    let body = build_state(&s).await;
    (
        StatusCode::OK,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        Json(body),
    )
}

/// SSE stream that pushes `StateResponse` on change. Internal cadence is
/// 100 ms (cheap RwLock reads); HTTP pushes happen only on fingerprint
/// change, plus a heartbeat every 15 s to keep proxies / browser sources
/// from idling out the connection.
async fn get_events(State(s): State<AppState>) -> impl IntoResponse {
    let stream = async_stream::stream! {
        // Initial push so a freshly-connected client renders immediately.
        let initial = build_state(&s).await;
        let mut last_fp = change_fingerprint(&initial);
        let initial_json = serde_json::to_string(&initial).unwrap_or_else(|_| "{}".into());
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("state").data(initial_json));

        let mut tick = tokio::time::interval(Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let body = build_state(&s).await;
            let fp = change_fingerprint(&body);
            if fp == last_fp {
                continue;
            }
            last_fp = fp;
            let json = serde_json::to_string(&body).unwrap_or_else(|_| "{}".into());
            yield Ok::<Event, std::convert::Infallible>(Event::default().event("state").data(json));
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Decode the cached `data:image/...;base64,...` URL into raw image bytes
/// and serve with the right Content-Type. 404 when no art is cached.
async fn get_art(State(s): State<AppState>) -> Response {
    let payload = { s.art.read().await.clone() };
    let Some(payload) = payload else {
        return (StatusCode::NOT_FOUND, "no art").into_response();
    };

    let Some((mime, b64)) = parse_data_url(&payload.data_url) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "bad data url").into_response();
    };

    let bytes = match base64::engine::general_purpose::STANDARD.decode(b64) {
        Ok(b) => b,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "bad base64").into_response(),
    };

    let mut resp = (StatusCode::OK, bytes).into_response();
    let headers = resp.headers_mut();
    if let Ok(v) = HeaderValue::from_str(mime) {
        headers.insert(header::CONTENT_TYPE, v);
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    // The browser will cache per URL; the embedded overlay appends
    // ?k={art_key} so a new track invalidates naturally.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    resp
}

/// Extract `(mime, base64_body)` from a `data:<mime>;base64,<body>` URL.
/// Returns None on any structural mismatch — caller falls back to 500.
fn parse_data_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("data:")?;
    let (header_part, body) = rest.split_once(',')?;
    let (mime, encoding) = header_part.split_once(';').unwrap_or((header_part, ""));
    if !encoding.eq_ignore_ascii_case("base64") {
        return None;
    }
    Some((mime, body))
}

async fn get_healthz() -> &'static str {
    "ok"
}

async fn get_overlay() -> Response {
    let html = include_str!("streamer_overlay.html");
    let mut resp = (StatusCode::OK, html).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    resp.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    resp
}

fn unix_ms_now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Handle to a running server, used to ask it to shut down when the
/// streamer setting is toggled off.
pub struct ServerHandle {
    shutdown: Option<oneshot::Sender<()>>,
}

impl ServerHandle {
    pub fn shutdown(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Boot a server on `127.0.0.1:port`. Returns immediately; the server
/// runs in a background task. Call `.shutdown()` on the handle to stop it.
pub fn start(app: AppHandle, port: u16) -> Result<ServerHandle> {
    let snapshot = app
        .try_state::<SharedSnapshot>()
        .context("SharedSnapshot not managed")?
        .inner()
        .clone();
    let lyrics = app
        .try_state::<SharedLyrics>()
        .context("SharedLyrics not managed")?
        .inner()
        .clone();
    let art = app
        .try_state::<SharedAlbumArt>()
        .context("SharedAlbumArt not managed")?
        .inner()
        .clone();
    let settings = app
        .try_state::<SharedSettings>()
        .context("SharedSettings not managed")?
        .inner()
        .clone();

    let state = AppState { snapshot, lyrics, art, settings };

    let app_router: Router = Router::new()
        .route("/state", get(get_state))
        .route("/events", get(get_events))
        .route("/art", get(get_art))
        .route("/overlay", get(get_overlay))
        .route("/", get(get_overlay))
        .route("/healthz", get(get_healthz))
        .with_state(state);

    let (tx, rx) = oneshot::channel::<()>();
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();

    tauri::async_runtime::spawn(async move {
        eprintln!("[streamer] starting on http://{addr}");
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[streamer] bind {addr} failed: {e}");
                return;
            }
        };
        let server = axum::serve(listener, app_router.into_make_service())
            .with_graceful_shutdown(async move {
                let _ = rx.await;
                eprintln!("[streamer] shutdown signal received");
            });
        if let Err(e) = server.await {
            eprintln!("[streamer] server exited with error: {e:#}");
        } else {
            eprintln!("[streamer] server stopped cleanly");
        }
    });

    Ok(ServerHandle { shutdown: Some(tx) })
}

/// Manages the lifecycle of an optionally-running streamer server. Stored
/// in Tauri state so update_settings can start / stop the server when the
/// `streamer_enabled` setting flips.
pub struct StreamerSupervisor {
    pub handle: std::sync::Mutex<Option<ServerHandle>>,
}

impl StreamerSupervisor {
    pub fn new() -> Self {
        Self {
            handle: std::sync::Mutex::new(None),
        }
    }
}

/// Start or stop the server based on the desired enabled state. Idempotent:
/// no-op if already in the requested state.
pub fn apply_settings(app: &AppHandle, enabled: bool, port: u16) {
    let supervisor = match app.try_state::<Arc<StreamerSupervisor>>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    let mut guard = supervisor.handle.lock().unwrap();
    let currently_running = guard.is_some();
    if enabled && !currently_running {
        match start(app.clone(), port) {
            Ok(h) => *guard = Some(h),
            Err(e) => eprintln!("[streamer] failed to start: {e:#}"),
        }
    } else if !enabled && currently_running {
        if let Some(mut h) = guard.take() {
            h.shutdown();
        }
    }
    // (enabled && currently_running with a different port would need a
    // restart — handle in v2 if anyone changes ports at runtime.)
}
