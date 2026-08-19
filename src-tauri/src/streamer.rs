//! OBS / browser-source HTTP server for the lyrics overlay.
//!
//! Spawns an axum server bound to 127.0.0.1:<port> that exposes:
//!
//! - `GET  /state`: JSON snapshot of current track + lyrics + cursor.
//!   Stateless poll endpoint; used as a fallback when the SSE stream is
//!   unavailable and by external tools that want a one-shot read.
//! - `GET  /events`: Server-Sent Events stream. Pushes the same state
//!   payload as `/state` whenever any change-relevant field flips (track,
//!   lyrics, cursor, source, playback state, album art, or saved timing).
//!   Native seeks also push when the reported position anchor departs from
//!   projected playback beyond the saved jitter tolerance. Position ticks
//!   are NOT pushed because the client interpolates locally from
//!   `position_ms + (now - last_update_unix_ms)` so the progress bar
//!   advances smoothly without the server flooding the wire.
//! - `GET  /art`: Current album art image bytes. Decoded from the
//!   `data:image/...` URL the desktop fetch chain produces, with the right
//!   Content-Type so `<img src="/art">` Just Works.
//! - `GET  /overlay`: Self-contained HTML page rendering the same chrome
//!   (album art, metadata, progress bar, source badge, gold dashed border)
//!   as the desktop overlay. Background is fully transparent so OBS
//!   browser-source layering needs no chroma-key tricks.
//! - `GET  /healthz`: Minimal liveness probe ("ok").
//!
//! The server is gated by `settings.streamer_enabled`. When off, no
//! port is bound. When toggled on at runtime via `update_settings`, a
//! new server task is spawned. When toggled off, the task's shutdown
//! signal fires and the port is freed.

use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
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
use crate::media::{CurrentTrack, PlaybackState, SharedAlbumArt, SharedSnapshot};
use crate::settings::SharedSettings;
use crate::window_effects::backdrop::BackdropKind;

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
    /// Global lyric offset so the browser-source client can compute its
    /// own cursor with the same anticipation as the desktop overlay.
    anticipate_ms: i32,
    /// Full timing adjustment used for line and word lookup. This includes
    /// expert anticipation and the selected listening profile delay.
    effective_offset_ms: i32,
}

async fn build_state(s: &AppState) -> StateResponse {
    let snap = s.snapshot.read().await.clone();
    let lyrics = s.lyrics.read().await.clone();
    let (anticipate_ms, effective_offset_ms) = {
        let settings = s.settings.read().await;
        (
            settings.anticipate_ms,
            crate::settings::effective_timing_offset_ms(&settings),
        )
    };

    let now_ms = unix_ms_now();
    let pos_ms = if snap.state == PlaybackState::Playing {
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
    let lookup_pos_ms = apply_signed_offset(pos_ms, effective_offset_ms);

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
        anticipate_ms,
        effective_offset_ms,
    }
}

fn apply_signed_offset(position_ms: u64, offset_ms: i32) -> u64 {
    if offset_ms >= 0 {
        position_ms.saturating_add(offset_ms as u64)
    } else {
        position_ms.saturating_sub(offset_ms.unsigned_abs() as u64)
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
    // Specific browser identification so the source badge can render
    // the right browser logo. "Browser" was too generic for
    // logo lookups since simple-icons doesn't have a "browser" icon.
    if lower.contains("chrome") {
        return "Chrome".into();
    }
    if lower.contains("msedge") || lower.contains("edge") {
        return "Edge".into();
    }
    if lower.contains("firefox") {
        return "Firefox".into();
    }
    if lower.contains("brave") {
        return "Brave".into();
    }
    if lower.contains("opera") {
        return "Opera".into();
    }
    if lower.contains("vivaldi") {
        return "Vivaldi".into();
    }
    if lower.contains("safari") {
        return "Safari".into();
    }
    if lower.contains("arc") {
        return "Arc".into();
    }
    if lower.contains("zen") {
        // Old broad-match path retained for backwards compatibility
        // with any cached state. Falls through to Browser otherwise.
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

#[derive(Serialize)]
struct RenderTrackFingerprint<'a> {
    title: &'a str,
    artist: &'a str,
    album: &'a str,
    duration_ms: u64,
    state: PlaybackState,
    source_app_id: Option<&'a str>,
    ad_active: bool,
    bridge_source: Option<&'a str>,
}

#[derive(Serialize)]
struct RenderStateFingerprint<'a> {
    track: RenderTrackFingerprint<'a>,
    lyrics: &'a CurrentLyrics,
    cursor: i32,
    art_key: &'a str,
    source_label: &'a str,
    anticipate_ms: i32,
    effective_offset_ms: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SseEventIdentity {
    render_fingerprint: String,
    position_revision: u64,
}

#[derive(Clone, Copy)]
struct PositionAnchor {
    position_ms: u64,
    last_update_unix_ms: i64,
    is_playing: bool,
}

impl From<&StateResponse> for PositionAnchor {
    fn from(state: &StateResponse) -> Self {
        Self {
            position_ms: state.track.position_ms,
            last_update_unix_ms: state.track.last_update_unix_ms,
            is_playing: state.track.state == PlaybackState::Playing,
        }
    }
}

struct SseEventTracker {
    published_anchor: PositionAnchor,
    published_identity: SseEventIdentity,
}

impl SseEventTracker {
    fn new(initial: &StateResponse) -> Self {
        Self {
            published_anchor: initial.into(),
            published_identity: SseEventIdentity {
                render_fingerprint: change_fingerprint(initial),
                position_revision: 0,
            },
        }
    }

    fn observe(
        &mut self,
        state: &StateResponse,
        jitter_tolerance_ms: i32,
    ) -> Option<SseEventIdentity> {
        let next_anchor = PositionAnchor::from(state);
        let render_fingerprint = change_fingerprint(state);
        let position_changed = position_anchor_is_discontinuous(
            self.published_anchor,
            next_anchor,
            jitter_tolerance_ms,
        );
        if render_fingerprint == self.published_identity.render_fingerprint && !position_changed {
            return None;
        }

        let position_revision = if position_changed {
            self.published_identity.position_revision.saturating_add(1)
        } else {
            self.published_identity.position_revision
        };
        let identity = SseEventIdentity {
            render_fingerprint,
            position_revision,
        };
        self.published_anchor = next_anchor;
        self.published_identity = identity.clone();
        Some(identity)
    }
}

fn position_anchor_is_discontinuous(
    previous: PositionAnchor,
    current: PositionAnchor,
    jitter_tolerance_ms: i32,
) -> bool {
    let elapsed_ms = current
        .last_update_unix_ms
        .saturating_sub(previous.last_update_unix_ms)
        .max(0) as u64;
    let expected_position_ms = if previous.is_playing {
        previous.position_ms.saturating_add(elapsed_ms)
    } else {
        previous.position_ms
    };
    expected_position_ms.abs_diff(current.position_ms) > jitter_tolerance_ms.max(0) as u64
}

/// Fingerprint of every render-relevant field except position interpolation
/// ticks. The client advances position from the last native observation, so
/// position, its update timestamp, and the server clock are intentionally
/// excluded. A cursor change still publishes because it changes the lyric.
fn change_fingerprint(s: &StateResponse) -> String {
    let payload = RenderStateFingerprint {
        track: RenderTrackFingerprint {
            title: &s.track.title,
            artist: &s.track.artist,
            album: &s.track.album,
            duration_ms: s.track.duration_ms,
            state: s.track.state,
            source_app_id: s.track.source_app_id.as_deref(),
            ad_active: s.track.ad_active,
            bridge_source: s.track.bridge_source.as_deref(),
        },
        lyrics: &s.lyrics,
        cursor: s.cursor,
        art_key: &s.art_key,
        source_label: &s.source_label,
        anticipate_ms: s.anticipate_ms,
        effective_offset_ms: s.effective_offset_ms,
    };
    let encoded = serde_json::to_vec(&payload).expect("render fingerprint must serialize");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    encoded.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Subset of Settings exposed to the OBS browser source so it can mirror the
/// desktop overlay's appearance without needing the full Settings struct.
#[derive(Serialize)]
struct OverlaySettings {
    font_family: String,
    font_size_px: f32,
    font_weight: i32,
    text_color: String,
    text_color_dim: String,
    bg_color: String,
    bg_opacity: f32,
    text_align: String,
    line_padding_px: i32,
    layout_mode: String,
    show_album_art: bool,
    tint_bg_from_album_art: bool,
    blur_album_art_background: bool,
    bg_hidden: bool,
    show_media: bool,
    window_backdrop: String,
    anticipate_ms: i32,
    effective_offset_ms: i32,
    listening_mode: String,
    profile_delay_ms: i32,
}

fn projected_backdrop(kind: BackdropKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "acrylic".to_string())
}

async fn get_settings_overlay(State(s): State<AppState>) -> impl IntoResponse {
    let cfg = s.settings.read().await;
    let body = OverlaySettings {
        font_family: cfg.font_family.clone(),
        font_size_px: cfg.font_size_px,
        font_weight: cfg.font_weight,
        text_color: cfg.text_color.clone(),
        text_color_dim: cfg.text_color_dim.clone(),
        bg_color: cfg.bg_color.clone(),
        bg_opacity: cfg.bg_opacity,
        text_align: cfg.text_align.clone(),
        line_padding_px: cfg.line_padding_px,
        layout_mode: cfg.layout_mode.clone(),
        show_album_art: cfg.show_album_art,
        tint_bg_from_album_art: cfg.tint_bg_from_album_art,
        blur_album_art_background: cfg.blur_album_art_background,
        bg_hidden: cfg.bg_hidden,
        show_media: cfg.show_media,
        window_backdrop: projected_backdrop(cfg.window_backdrop),
        anticipate_ms: cfg.anticipate_ms,
        effective_offset_ms: crate::settings::effective_timing_offset_ms(&cfg),
        listening_mode: cfg.listening_mode.clone(),
        profile_delay_ms: crate::settings::selected_profile_delay_ms(&cfg),
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
        let mut event_tracker = SseEventTracker::new(&initial);
        let initial_json = serde_json::to_string(&initial).unwrap_or_else(|_| "{}".into());
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("state").data(initial_json));

        let mut tick = tokio::time::interval(Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let body = build_state(&s).await;
            let jitter_tolerance_ms = s.settings.read().await.jitter_tolerance_ms;
            if event_tracker.observe(&body, jitter_tolerance_ms).is_none() {
                continue;
            }
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
/// Returns None on any structural mismatch, and the caller falls back to 500.
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

// Hum brand mark for the centered ghost watermark. Embedded so the streamer
// endpoint stays self-contained. Vite serves the same file from `public/`
// for the desktop overlay.
async fn get_logo() -> Response {
    let bytes: &[u8] = include_bytes!("../../public/hum-logo.png");
    let mut resp = (StatusCode::OK, bytes.to_vec()).into_response();
    resp.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    resp.headers_mut().insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    resp
}

// Service brand logos (Netflix / Twitch / YouTube / etc.). Embedded as
// bytes so the streamer endpoint stays self-contained; the desktop
// overlay loads the same files via Vite's `public/` static-serve.
// Each entry is a `(slug, svg-bytes)` pair. Adding a new logo means
// dropping the SVG into `public/logos/` and adding an entry below.
const SERVICE_LOGOS: &[(&str, &[u8])] = &[
    // Video services
    ("netflix", include_bytes!("../../public/logos/netflix.svg")),
    ("twitch", include_bytes!("../../public/logos/twitch.svg")),
    ("youtube", include_bytes!("../../public/logos/youtube.svg")),
    (
        "crunchyroll",
        include_bytes!("../../public/logos/crunchyroll.svg"),
    ),
    (
        "paramountplus",
        include_bytes!("../../public/logos/paramountplus.svg"),
    ),
    ("max", include_bytes!("../../public/logos/max.svg")),
    ("appletv", include_bytes!("../../public/logos/appletv.svg")),
    ("hbomax", include_bytes!("../../public/logos/hbomax.svg")),
    // Music services / apps
    ("pandora", include_bytes!("../../public/logos/pandora.svg")),
    ("spotify", include_bytes!("../../public/logos/spotify.svg")),
    ("itunes", include_bytes!("../../public/logos/itunes.svg")),
    (
        "applemusic",
        include_bytes!("../../public/logos/applemusic.svg"),
    ),
    (
        "youtubemusic",
        include_bytes!("../../public/logos/youtubemusic.svg"),
    ),
    ("tidal", include_bytes!("../../public/logos/tidal.svg")),
    ("deezer", include_bytes!("../../public/logos/deezer.svg")),
    (
        "vlcmediaplayer",
        include_bytes!("../../public/logos/vlcmediaplayer.svg"),
    ),
    (
        "foobar2000",
        include_bytes!("../../public/logos/foobar2000.svg"),
    ),
    ("winamp", include_bytes!("../../public/logos/winamp.svg")),
    // Browsers (source-badge fallback for sources where the actual
    // service can't be identified beyond "running in a browser")
    (
        "googlechrome",
        include_bytes!("../../public/logos/googlechrome.svg"),
    ),
    ("firefox", include_bytes!("../../public/logos/firefox.svg")),
    ("brave", include_bytes!("../../public/logos/brave.svg")),
    ("opera", include_bytes!("../../public/logos/opera.svg")),
    ("safari", include_bytes!("../../public/logos/safari.svg")),
    ("arc", include_bytes!("../../public/logos/arc.svg")),
    ("vivaldi", include_bytes!("../../public/logos/vivaldi.svg")),
];

async fn get_service_logo(axum::extract::Path(slug): axum::extract::Path<String>) -> Response {
    // Strip the .svg extension if present so /logos/netflix.svg and
    // /logos/netflix both work.
    let needle = slug.strip_suffix(".svg").unwrap_or(&slug);
    // Defensive: reject path traversal attempts (`..`, `/`, `\`). The
    // axum path matcher already restricts to a single segment, but a
    // belt-and-suspenders check doesn't cost anything.
    if needle.contains('/') || needle.contains('\\') || needle.contains("..") {
        return (StatusCode::BAD_REQUEST, "bad slug").into_response();
    }
    let Some((_, bytes)) = SERVICE_LOGOS.iter().find(|(n, _)| *n == needle) else {
        return (StatusCode::NOT_FOUND, "no such logo").into_response();
    };
    let mut resp = (StatusCode::OK, bytes.to_vec()).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml"),
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
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

/// DNS-rebinding guard. The server binds 127.0.0.1, but every response sets
/// `Access-Control-Allow-Origin: *` for OBS browser-source compatibility.
/// without this a malicious web page could rebind a hostname it controls to
/// 127.0.0.1 and read the now-playing data cross-origin. Only accept loopback
/// `Host` headers, which is exactly what OBS / the browser source
/// (`http://localhost:<port>/…`) send.
async fn host_guard(port: u16, req: Request, next: Next) -> Response {
    let host_ok = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .is_some_and(|h| {
            h == format!("127.0.0.1:{port}")
                || h == format!("localhost:{port}")
                || h == format!("[::1]:{port}")
        });
    if !host_ok {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(req).await
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

    let state = AppState {
        snapshot,
        lyrics,
        art,
        settings,
    };

    let app_router: Router = Router::new()
        .route("/state", get(get_state))
        .route("/settings", get(get_settings_overlay))
        .route("/events", get(get_events))
        .route("/art", get(get_art))
        .route("/overlay", get(get_overlay))
        .route("/", get(get_overlay))
        .route("/hum-logo.png", get(get_logo))
        .route("/logos/{slug}", get(get_service_logo))
        .route("/healthz", get(get_healthz))
        .with_state(state)
        .layer(middleware::from_fn(move |req: Request, next: Next| {
            host_guard(port, req, next)
        }));

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
        let server = axum::serve(listener, app_router.into_make_service()).with_graceful_shutdown(
            async move {
                let _ = rx.await;
                eprintln!("[streamer] shutdown signal received");
            },
        );
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
    /// The running server handle paired with the port it's bound to. The port
    /// is tracked so a runtime port change (Settings → Port while the server
    /// is already running) can be detected and trigger a rebind.
    pub handle: std::sync::Mutex<Option<(ServerHandle, u16)>>,
}

impl StreamerSupervisor {
    pub fn new() -> Self {
        Self {
            handle: std::sync::Mutex::new(None),
        }
    }
}

/// Start or stop the server based on the desired enabled state + port.
/// Idempotent: no-op when already in the requested state on the requested
/// port. If the server is running on a different port than requested, it is
/// stopped and restarted on the new port.
pub fn apply_settings(app: &AppHandle, enabled: bool, port: u16) {
    let supervisor = match app.try_state::<Arc<StreamerSupervisor>>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    let mut guard = supervisor.handle.lock().unwrap();
    let running_port = guard.as_ref().map(|(_, p)| *p);
    match (enabled, running_port) {
        // Want it on, nothing running → start.
        (true, None) => match start(app.clone(), port) {
            Ok(h) => *guard = Some((h, port)),
            Err(e) => eprintln!("[streamer] failed to start: {e:#}"),
        },
        // Want it on, running on a stale port → stop, then restart on the new
        // port. Different port, so no bind conflict with the draining old one.
        (true, Some(running)) if running != port => {
            if let Some((mut h, _)) = guard.take() {
                h.shutdown();
            }
            match start(app.clone(), port) {
                Ok(h) => *guard = Some((h, port)),
                Err(e) => eprintln!("[streamer] failed to restart on port {port}: {e:#}"),
            }
        }
        // Want it off, something running → stop.
        (false, Some(_)) => {
            if let Some((mut h, _)) = guard.take() {
                h.shutdown();
            }
        }
        // Already in the desired state (on + same port, or off + stopped).
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::{LyricLine, Status, WordSpan};

    fn response_with_word(duration_ms: u32) -> StateResponse {
        let lyrics = CurrentLyrics {
            status: Status::Synced,
            line_count: 1,
            lines: vec![LyricLine {
                time_ms: 1_000,
                text: "Hello".into(),
                words: Some(vec![WordSpan {
                    time_ms: 1_000,
                    duration_ms: Some(duration_ms),
                    text: "Hello".into(),
                }]),
            }],
            ..Default::default()
        };
        StateResponse {
            track: CurrentTrack::default(),
            lyrics,
            cursor: 0,
            server_now_ms: 0,
            art_key: String::new(),
            source_label: String::new(),
            anticipate_ms: 0,
            effective_offset_ms: 0,
        }
    }

    #[test]
    fn signed_offset_saturates_at_zero() {
        assert_eq!(apply_signed_offset(100, -250), 0);
        assert_eq!(apply_signed_offset(100, 250), 350);
    }

    #[test]
    fn sse_fingerprint_changes_when_word_timing_changes() {
        let first = response_with_word(300);
        let second = response_with_word(450);
        assert_ne!(change_fingerprint(&first), change_fingerprint(&second));
    }

    #[test]
    fn sse_fingerprint_changes_when_same_count_line_text_changes() {
        let first = response_with_word(300);
        let mut second = first.clone();
        second.lyrics.lines[0].text = "Goodbye".into();

        assert_ne!(change_fingerprint(&first), change_fingerprint(&second));
    }

    #[test]
    fn sse_fingerprint_changes_when_plain_lyrics_change() {
        let mut first = response_with_word(300);
        first.lyrics.status = Status::Plain;
        first.lyrics.line_count = 0;
        first.lyrics.lines.clear();
        first.lyrics.plain = Some("First plain lyric".into());
        let mut second = first.clone();
        second.lyrics.plain = Some("Second plain lyric".into());

        assert_ne!(change_fingerprint(&first), change_fingerprint(&second));
    }

    #[test]
    fn sse_fingerprint_changes_when_translations_change() {
        let mut first = response_with_word(300);
        first.lyrics.translation = Some(vec![LyricLine {
            time_ms: 1_000,
            text: "First translation".into(),
            words: None,
        }]);
        let mut second = first.clone();
        second.lyrics.translation.as_mut().unwrap()[0].text = "Second translation".into();

        assert_ne!(change_fingerprint(&first), change_fingerprint(&second));
    }

    #[test]
    fn sse_fingerprint_changes_when_track_duration_changes() {
        let first = response_with_word(300);
        let mut second = first.clone();
        second.track.duration_ms = 240_000;

        assert_ne!(change_fingerprint(&first), change_fingerprint(&second));
    }

    #[test]
    fn sse_fingerprint_changes_when_source_app_id_changes() {
        let first = response_with_word(300);
        let mut second = first.clone();
        second.track.source_app_id = Some("Spotify.exe".into());

        assert_ne!(change_fingerprint(&first), change_fingerprint(&second));
    }

    #[test]
    fn sse_fingerprint_changes_when_source_label_changes() {
        let first = response_with_word(300);
        let mut second = first.clone();
        second.source_label = "Spotify".into();

        assert_ne!(change_fingerprint(&first), change_fingerprint(&second));
    }

    #[test]
    fn sse_fingerprint_ignores_position_interpolation_ticks() {
        let first = response_with_word(300);
        let mut second = first.clone();
        second.track.position_ms = 15_000;
        second.track.last_update_unix_ms = 42_000;
        second.server_now_ms = 43_000;

        assert_eq!(change_fingerprint(&first), change_fingerprint(&second));
    }

    #[test]
    fn sse_event_identity_changes_for_same_line_seek() {
        let mut first = response_with_word(300);
        first.track.state = PlaybackState::Playing;
        first.track.position_ms = 10_000;
        first.track.last_update_unix_ms = 100_000;
        first.server_now_ms = 100_000;
        let mut second = first.clone();
        second.track.position_ms = 30_000;
        second.track.last_update_unix_ms = 101_000;
        second.server_now_ms = 101_000;

        let mut tracker = SseEventTracker::new(&first);
        let first_identity = tracker.published_identity.clone();
        let second_identity = tracker.observe(&second, 2_000).unwrap();

        assert_ne!(first_identity, second_identity);
    }

    #[test]
    fn sse_event_identity_ignores_equivalent_natural_position_anchor() {
        let mut first = response_with_word(300);
        first.track.state = PlaybackState::Playing;
        first.track.position_ms = 10_000;
        first.track.last_update_unix_ms = 100_000;
        first.server_now_ms = 100_000;
        let mut second = first.clone();
        second.track.position_ms = 11_000;
        second.track.last_update_unix_ms = 101_000;
        second.server_now_ms = 101_000;

        let mut tracker = SseEventTracker::new(&first);
        let first_identity = tracker.published_identity.clone();
        let second_identity = tracker.observe(&second, 2_000);

        assert!(second_identity.is_none());
        assert_eq!(first_identity, tracker.published_identity);
    }

    #[test]
    fn sse_event_identity_accumulates_sub_tolerance_drift_from_published_anchor() {
        let mut initial = response_with_word(300);
        initial.track.state = PlaybackState::Playing;
        initial.track.position_ms = 10_000;
        initial.track.last_update_unix_ms = 100_000;
        initial.server_now_ms = 100_000;
        let mut first_drift = initial.clone();
        first_drift.track.position_ms = 12_500;
        first_drift.track.last_update_unix_ms = 101_000;
        first_drift.server_now_ms = 101_000;
        let mut cumulative_drift = first_drift.clone();
        cumulative_drift.track.position_ms = 15_000;
        cumulative_drift.track.last_update_unix_ms = 102_000;
        cumulative_drift.server_now_ms = 102_000;

        let mut tracker = SseEventTracker::new(&initial);
        let initial_identity = tracker.published_identity.clone();
        let first_identity = tracker.observe(&first_drift, 2_000);
        let cumulative_identity = tracker.observe(&cumulative_drift, 2_000).unwrap();

        assert!(first_identity.is_none());
        assert_ne!(initial_identity, cumulative_identity);
    }

    #[test]
    fn sse_render_event_resets_published_position_anchor() {
        let mut initial = response_with_word(300);
        initial.track.state = PlaybackState::Playing;
        initial.track.position_ms = 10_000;
        initial.track.last_update_unix_ms = 100_000;
        initial.server_now_ms = 100_000;
        let mut first_drift = initial.clone();
        first_drift.track.position_ms = 12_500;
        first_drift.track.last_update_unix_ms = 101_000;
        first_drift.server_now_ms = 101_000;
        let mut render_change = first_drift.clone();
        render_change.track.position_ms = 13_500;
        render_change.track.last_update_unix_ms = 102_000;
        render_change.server_now_ms = 102_000;
        render_change.lyrics.lines[0].text = "Updated lyric".into();
        let mut drift_after_render = render_change.clone();
        drift_after_render.track.position_ms = 16_000;
        drift_after_render.track.last_update_unix_ms = 103_000;
        drift_after_render.server_now_ms = 103_000;

        let mut tracker = SseEventTracker::new(&initial);
        let initial_identity = tracker.published_identity.clone();
        let first_identity = tracker.observe(&first_drift, 2_000);
        let render_identity = tracker.observe(&render_change, 2_000).unwrap();
        let after_render_identity = tracker.observe(&drift_after_render, 2_000);

        assert!(first_identity.is_none());
        assert_ne!(initial_identity, render_identity);
        assert_eq!(
            initial_identity.position_revision,
            render_identity.position_revision
        );
        assert!(after_render_identity.is_none());
        assert_eq!(render_identity, tracker.published_identity);
    }

    #[test]
    fn obs_backdrop_projection_matches_portable_wire_values() {
        let cases = [
            (BackdropKind::None, "none"),
            (BackdropKind::Mica, "mica"),
            (BackdropKind::Acrylic, "acrylic"),
            (BackdropKind::TabbedMica, "tabbed_mica"),
        ];
        for (kind, expected) in cases {
            assert_eq!(projected_backdrop(kind), expected);
        }
    }
}
