//! OBS / browser-source HTTP server for the lyrics overlay.
//!
//! Spawns an axum server bound to 127.0.0.1:<port> that exposes:
//!
//! - `GET  /state`    — JSON snapshot of current track + lyrics + cursor.
//!                      Polled by the /overlay page; can also be hit by
//!                      external tools that want to display lyrics elsewhere.
//! - `GET  /overlay`  — Self-contained HTML page (inline CSS + JS) that
//!                      polls /state every 250ms and renders the same 3-line
//!                      look the desktop overlay does. Background is fully
//!                      transparent so OBS browser-source layering Just
//!                      Works without chroma-key tricks.
//! - `GET  /healthz`  — Minimal liveness probe ("ok").
//!
//! The server is gated by `settings.streamer_enabled` — when off, no
//! port is bound. When toggled on at runtime via `update_settings`, a
//! new server task is spawned. When toggled off, the task's shutdown
//! signal fires and the port is freed.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

use crate::lyrics::{CurrentLyrics, SharedLyrics};
use crate::smtc::{CurrentTrack, SharedSnapshot};

#[derive(Clone)]
struct AppState {
    snapshot: SharedSnapshot,
    lyrics: SharedLyrics,
}

/// Combined snapshot returned by /state. Frontend (the embedded overlay
/// HTML, or any third-party consumer) renders from this.
#[derive(Serialize)]
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
}

async fn get_state(State(s): State<AppState>) -> impl IntoResponse {
    let snap = s.snapshot.read().await.clone();
    let lyrics = s.lyrics.read().await.clone();

    // Compute current cursor based on interpolated position. Mirrors the
    // logic in src/Overlay.tsx::tick so /state consumers don't have to.
    let now_ms = unix_ms_now();
    let pos_ms = if snap.state == crate::smtc::PlaybackState::Playing {
        let elapsed = (now_ms - snap.last_update_unix_ms).max(0);
        snap.position_ms.saturating_add(elapsed as u64)
    } else {
        snap.position_ms
    };

    let mut cursor: i32 = -1;
    if matches!(lyrics.status, crate::lyrics::Status::Synced) {
        for (i, line) in lyrics.lines.iter().enumerate() {
            if line.time_ms as u64 <= pos_ms {
                cursor = i as i32;
            } else {
                break;
            }
        }
    }

    let body = StateResponse {
        track: snap,
        lyrics,
        cursor,
        server_now_ms: now_ms,
    };
    (
        StatusCode::OK,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        Json(body),
    )
}

async fn get_healthz() -> &'static str {
    "ok"
}

async fn get_overlay() -> Response {
    // Self-contained HTML with inline CSS + JS. Polls /state every 250ms
    // and renders prev / cur / next lines + album art (when present). No
    // build step, no external assets. OBS browser source URL: set width
    // ~1100, height ~200, custom CSS empty, "Refresh browser when scene
    // becomes active" recommended.
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

    let state = AppState { snapshot, lyrics };

    let app_router: Router = Router::new()
        .route("/state", get(get_state))
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
