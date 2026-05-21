//! Windows SMTC (System Media Transport Controls) bridge.
//!
//! Subscribes to the Windows global media session manager, watches the active
//! session, and emits three Tauri events whenever state changes:
//!
//! - `track-changed`             — title/artist/album/duration update
//! - `timeline-changed`          — position update (used for client-side interpolation)
//! - `playback-state-changed`    — play/pause/stop transition
//!
//! All three carry the same flat `CurrentTrack` payload — the consumer reads
//! whichever fields it cares about. The full snapshot is also retrievable via
//! the `get_current_track` Tauri command.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};
use windows::Foundation::{EventRegistrationToken, TypedEventHandler};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionMediaProperties,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::DataReader;

#[derive(Clone, Copy, Serialize, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackState {
    #[default]
    Unknown,
    Closed,
    Opened,
    Changing,
    Stopped,
    Playing,
    Paused,
}

impl From<GlobalSystemMediaTransportControlsSessionPlaybackStatus> for PlaybackState {
    fn from(s: GlobalSystemMediaTransportControlsSessionPlaybackStatus) -> Self {
        use GlobalSystemMediaTransportControlsSessionPlaybackStatus as P;
        match s {
            P::Closed => Self::Closed,
            P::Opened => Self::Opened,
            P::Changing => Self::Changing,
            P::Stopped => Self::Stopped,
            P::Playing => Self::Playing,
            P::Paused => Self::Paused,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Serialize, Debug, Default)]
pub struct CurrentTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub position_ms: u64,
    /// Unix epoch ms when SMTC last reported the position. The frontend uses
    /// `position_ms + (now - last_update_unix_ms)` to interpolate while playing.
    pub last_update_unix_ms: i64,
    pub state: PlaybackState,
    /// e.g. "Spotify.exe", "308046B0AF4A39CB" (Firefox AUMID), etc. Useful for
    /// debugging / future per-source behavior.
    pub source_app_id: Option<String>,
}

pub type SharedSnapshot = Arc<RwLock<CurrentTrack>>;

/// Last emitted album-art payload, kept so the frontend can `invoke`
/// `get_current_album_art` on mount and pick up the art that fired BEFORE
/// its `listen("album-art-loaded", …)` subscription completed. Without
/// this, a fresh app launch with Chrome/Spotify already playing shows
/// lyrics but no artwork until the user switches tracks (which fires
/// `MediaPropertiesChanged` and re-emits the event with the listener now
/// attached). The cache is overwritten on every successful art fetch and
/// the frontend's render filter (`payload.title === track.title &&
/// payload.artist === track.artist`) hides stale entries automatically
/// during the ~100ms between track-change and the fresh art arriving.
pub type SharedAlbumArt = Arc<RwLock<Option<AlbumArtPayload>>>;

#[derive(Clone, Copy, Debug)]
#[allow(clippy::enum_variant_names)]
enum Msg {
    SessionChanged,
    MediaChanged,
    TimelineChanged,
    PlaybackChanged,
}

/// Owns the per-session event-handler registrations. Dropping it removes them.
struct SessionHooks {
    session: GlobalSystemMediaTransportControlsSession,
    media_token: EventRegistrationToken,
    timeline_token: EventRegistrationToken,
    playback_token: EventRegistrationToken,
}

impl Drop for SessionHooks {
    fn drop(&mut self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.media_token);
        let _ = self
            .session
            .RemoveTimelinePropertiesChanged(self.timeline_token);
        let _ = self.session.RemovePlaybackInfoChanged(self.playback_token);
    }
}

/// Owns the manager-level `CurrentSessionChanged` registration. Dropping it
/// removes the handler so cancelling the worker future doesn't leave a
/// dangling COM callback firing into a closed mpsc channel.
struct ManagerHook {
    manager: GlobalSystemMediaTransportControlsSessionManager,
    token: EventRegistrationToken,
}

impl Drop for ManagerHook {
    fn drop(&mut self) {
        let _ = self.manager.RemoveCurrentSessionChanged(self.token);
    }
}

/// Spawn the SMTC worker. Logs and exits if it can't initialize — the rest of
/// the app keeps running so the user can at least see the dev shell.
///
/// `smtc_playing` is set to `true` only when SMTC has an active session that
/// is *currently playing* — not merely attached. Other source modules (e.g.
/// the iTunes COM bridge) read this flag to decide whether to suppress their
/// own emissions. SMTC sessions can hang around in Paused/Stopped/Closed states
/// long after a tab closed (Chrome is notorious for this), so "session exists"
/// is too coarse to use as a priority signal.
pub fn start(
    app: AppHandle,
    snapshot: SharedSnapshot,
    art: SharedAlbumArt,
    smtc_playing: Arc<AtomicBool>,
) {
    tauri::async_runtime::spawn(async move {
        eprintln!("[smtc] worker starting");
        if let Err(e) = run(app, snapshot, art, smtc_playing).await {
            eprintln!("[smtc] worker exited: {e:#}");
        } else {
            eprintln!("[smtc] worker exited (rx channel closed)");
        }
    });
}

async fn run(
    app: AppHandle,
    snapshot: SharedSnapshot,
    art: SharedAlbumArt,
    smtc_playing: Arc<AtomicBool>,
) -> Result<()> {
    // RequestAsync returns IAsyncOperation; .get() blocks until ready. The
    // call is one-shot at startup and resolves in milliseconds, so blocking
    // the worker task here is fine.
    let manager = tokio::task::spawn_blocking(|| -> Result<_> {
        let op = GlobalSystemMediaTransportControlsSessionManager::RequestAsync()
            .context("RequestAsync handle")?;
        op.get().context("RequestAsync get")
    })
    .await
    .context("spawn_blocking RequestAsync")??;
    eprintln!("[smtc] manager acquired");

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();

    // Manager-level: fires when the foreground media source changes (e.g. user
    // switches from Spotify to YouTube in Chrome).
    let tx_session = tx.clone();
    let session_token = manager.CurrentSessionChanged(&TypedEventHandler::new(
        move |_, _| {
            let _ = tx_session.send(Msg::SessionChanged);
            Ok(())
        },
    ))?;
    let _manager_hook = ManagerHook {
        manager: manager.clone(),
        token: session_token,
    };
    eprintln!("[smtc] CurrentSessionChanged handler registered");

    let mut hooks: Option<SessionHooks> = match attach_session(&manager, &tx) {
        Ok(h) => Some(h),
        Err(e) => {
            eprintln!("[smtc] startup attach_session failed (probably no active SMTC session): {e:#}");
            None
        }
    };
    if let Some(ref h) = hooks {
        let state = read_state(&h.session).unwrap_or_default();
        let aumid = h
            .session
            .SourceAppUserModelId()
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_default();
        smtc_playing.store(state == PlaybackState::Playing, Ordering::Relaxed);
        eprintln!("[smtc] startup: session attached, source='{aumid}', state={state:?}");
        emit_full(&app, &snapshot, &art, &h.session).await;
    } else {
        smtc_playing.store(false, Ordering::Relaxed);
        eprintln!("[smtc] startup: no active session, smtc_playing=false");
    }

    while let Some(msg) = rx.recv().await {
        match msg {
            Msg::SessionChanged => {
                eprintln!("[smtc] Msg::SessionChanged");
                hooks = match attach_session(&manager, &tx) {
                    Ok(h) => Some(h),
                    Err(e) => {
                        eprintln!("[smtc] session-change attach_session failed: {e:#}");
                        None
                    }
                };
                if let Some(ref h) = hooks {
                    let state = read_state(&h.session).unwrap_or_default();
                    let aumid = h
                        .session
                        .SourceAppUserModelId()
                        .ok()
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    smtc_playing.store(state == PlaybackState::Playing, Ordering::Relaxed);
                    eprintln!("[smtc] new session attached, source='{aumid}', state={state:?}");
                    emit_full(&app, &snapshot, &art, &h.session).await;
                } else {
                    // No active session — clear the snapshot and notify.
                    smtc_playing.store(false, Ordering::Relaxed);
                    let mut snap = snapshot.write().await;
                    *snap = CurrentTrack::default();
                    let _ = app.emit("track-changed", &*snap);
                    let _ = app.emit("playback-state-changed", &*snap);
                    eprintln!("[smtc] no active session, snapshot cleared");
                }
            }
            Msg::MediaChanged => {
                if let Some(ref h) = hooks {
                    match read_track(&h.session).await {
                        Ok(track) => {
                            let (title, artist) = (track.title.clone(), track.artist.clone());
                            eprintln!(
                                "[smtc] Msg::MediaChanged → title='{title}' artist='{artist}' album='{}' dur={}ms",
                                track.album, track.duration_ms
                            );
                            let mut snap = snapshot.write().await;
                            snap.title = track.title;
                            snap.artist = track.artist;
                            snap.album = track.album;
                            snap.duration_ms = track.duration_ms;
                            let _ = app.emit("track-changed", &*snap);
                            drop(snap);
                            spawn_art_fetch(app.clone(), art.clone(), h.session.clone(), title, artist);
                        }
                        Err(e) => {
                            eprintln!("[smtc] Msg::MediaChanged → read_track failed: {e:#}");
                        }
                    }
                }
            }
            Msg::TimelineChanged => {
                // Routine — fires ~1Hz during playback. No log on success.
                if let Some(ref h) = hooks {
                    match read_timeline(&h.session) {
                        Ok((position_ms, duration_ms, last_update)) => {
                            let mut snap = snapshot.write().await;
                            snap.position_ms = position_ms;
                            if duration_ms > 0 {
                                snap.duration_ms = duration_ms;
                            }
                            snap.last_update_unix_ms = last_update;
                            let _ = app.emit("timeline-changed", &*snap);
                        }
                        Err(e) => {
                            eprintln!("[smtc] Msg::TimelineChanged → read_timeline failed: {e:#}");
                        }
                    }
                }
            }
            Msg::PlaybackChanged => {
                if let Some(ref h) = hooks {
                    match read_state(&h.session) {
                        Ok(state) => {
                            eprintln!("[smtc] Msg::PlaybackChanged → state={state:?}");
                            smtc_playing.store(state == PlaybackState::Playing, Ordering::Relaxed);
                            let mut snap = snapshot.write().await;
                            snap.state = state;
                            let _ = app.emit("playback-state-changed", &*snap);
                        }
                        Err(e) => {
                            eprintln!("[smtc] Msg::PlaybackChanged → read_state failed: {e:#}");
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn attach_session(
    manager: &GlobalSystemMediaTransportControlsSessionManager,
    tx: &mpsc::UnboundedSender<Msg>,
) -> Result<SessionHooks> {
    let session = manager.GetCurrentSession()?;

    let tx1 = tx.clone();
    let media_token = session.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx1.send(Msg::MediaChanged);
        Ok(())
    }))?;

    let tx2 = tx.clone();
    let timeline_token = session.TimelinePropertiesChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx2.send(Msg::TimelineChanged);
        Ok(())
    }))?;

    let tx3 = tx.clone();
    let playback_token = session.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
        let _ = tx3.send(Msg::PlaybackChanged);
        Ok(())
    }))?;

    Ok(SessionHooks {
        session,
        media_token,
        timeline_token,
        playback_token,
    })
}

async fn emit_full(
    app: &AppHandle,
    snapshot: &SharedSnapshot,
    art: &SharedAlbumArt,
    session: &GlobalSystemMediaTransportControlsSession,
) {
    let track = read_track(session).await.ok();
    let timeline = read_timeline(session).ok();
    let state = read_state(session).unwrap_or_default();
    let source_app_id = session
        .SourceAppUserModelId()
        .ok()
        .map(|s| s.to_string());

    {
        let mut snap = snapshot.write().await;
        if let Some(t) = track {
            snap.title = t.title;
            snap.artist = t.artist;
            snap.album = t.album;
            snap.duration_ms = t.duration_ms;
        }
        if let Some((pos, dur, last)) = timeline {
            snap.position_ms = pos;
            if dur > 0 {
                snap.duration_ms = dur;
            }
            snap.last_update_unix_ms = last;
        }
        snap.state = state;
        snap.source_app_id = source_app_id;
    }

    let (snap_title, snap_artist) = {
        let snap = snapshot.read().await;
        eprintln!(
            "[smtc] emit_full → title='{}' artist='{}' state={:?} pos={}ms dur={}ms",
            snap.title, snap.artist, snap.state, snap.position_ms, snap.duration_ms
        );
        let _ = app.emit("track-changed", &*snap);
        let _ = app.emit("timeline-changed", &*snap);
        let _ = app.emit("playback-state-changed", &*snap);
        (snap.title.clone(), snap.artist.clone())
    };

    if !snap_title.trim().is_empty() {
        spawn_art_fetch(app.clone(), art.clone(), session.clone(), snap_title, snap_artist);
    }
}

#[derive(Clone, Serialize)]
pub struct AlbumArtPayload {
    pub title: String,
    pub artist: String,
    pub data_url: String,
}

// Album art is large (50-200KB base64). Carrying it inside CurrentTrack would
// bloat every timeline-changed payload by that much; we emit it via a
// dedicated `album-art-loaded` event the frontend keys against the current
// track. Best-effort — many sources don't expose a thumbnail at all.
//
// Priority: iTunes Search API first (real album cover at 600×600) then
// fall back to the SMTC-supplied thumbnail. SMTC thumbnails for browser
// sources are either video thumbnails (YouTube) or the browser favicon
// (Pandora, etc.); both look wrong behind the blurred-art treatment.
// For Spotify desktop / iTunes desktop / Apple Music desktop the SMTC
// thumbnail IS the canonical album art and iTunes Search usually
// returns the same cover anyway, so the iTunes-first preference is
// strictly an improvement or a no-op.
fn spawn_art_fetch(
    app: AppHandle,
    art: SharedAlbumArt,
    session: GlobalSystemMediaTransportControlsSession,
    title: String,
    artist: String,
) {
    tauri::async_runtime::spawn(async move {
        // If a web-bridge probe matches the current SMTC snapshot,
        // skip art entirely on the SMTC side — the bridge has the
        // real artist+title and will fetch art keyed to those values.
        // Without this, smtc.rs would emit an art-loaded event keyed
        // to Pandora's garbage tab title, racing the bridge's correct
        // emission and potentially overwriting the SharedAlbumArt
        // cache with the wrong value.
        let source_app_id = session.SourceAppUserModelId()
            .ok()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if crate::web_bridge::any_probe_detects(&title, &source_app_id) {
            return;
        }

        // First try iTunes Search if we have something to query with.
        // Empty artist + title means SMTC didn't give us a real song
        // (idle session, etc.) — skip the external call.
        let itunes_data_url = if !title.trim().is_empty() {
            match build_itunes_http_client() {
                Ok(client) => fetch_art_via_itunes(&client, &artist, &title).await,
                Err(_) => None,
            }
        } else {
            None
        };

        let data_url = if let Some(url) = itunes_data_url {
            url
        } else {
            // iTunes miss — fall back to the SMTC thumbnail.
            let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>> {
                let props = session.TryGetMediaPropertiesAsync()?.get()?;
                read_thumbnail_bytes(&props)
            })
            .await;
            let bytes = match result {
                Ok(Ok(b)) => b,
                Ok(Err(_)) => return,
                Err(_) => return,
            };
            if bytes.is_empty() {
                return;
            }
            let mime = guess_image_mime(&bytes);
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            format!("data:{mime};base64,{b64}")
        };

        let payload = AlbumArtPayload {
            title,
            artist,
            data_url,
        };
        // Write to shared cache BEFORE emitting so a get_current_album_art
        // invocation racing the listener subscription on the frontend's mount
        // never sees a stale value relative to the just-emitted event.
        {
            let mut a = art.write().await;
            *a = Some(payload.clone());
        }
        let _ = app.emit("album-art-loaded", &payload);
    });
}

fn build_itunes_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(format!("hum/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .context("build itunes search http client")
}

/// Fetch album art from an external source. Public, no-auth. Used by
/// both `spawn_art_fetch` (any SMTC source) and `web_bridge` (Pandora-
/// style probes) to get a real album cover instead of whatever SMTC
/// supplied (favicons, video thumbnails, etc.).
///
/// Source preference:
/// 1. iTunes Search API — Apple's catalog, 600×600 JPEG. Broad coverage
///    of commercial releases, fast (~300ms typical).
/// 2. Deezer Search API — fallback when iTunes misses. Different catalog
///    (especially European / non-US-charting tracks). 1000×1000 JPEG.
///    Public, no-auth, no quota at our usage levels.
///
/// Returns a `data:image/jpeg;base64,...` URL on success, `None` when
/// both sources miss or all network steps fail.
pub async fn fetch_art_via_itunes(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
) -> Option<String> {
    if let Some(url) = fetch_art_itunes_only(client, artist, title).await {
        eprintln!("[smtc] art: iTunes hit for {artist:?} - {title:?}");
        return Some(url);
    }
    eprintln!("[smtc] art: iTunes miss for {artist:?} - {title:?}, trying Deezer");
    if let Some(url) = fetch_art_deezer_only(client, artist, title).await {
        eprintln!("[smtc] art: Deezer hit for {artist:?} - {title:?}");
        return Some(url);
    }
    eprintln!("[smtc] art: Deezer miss for {artist:?} - {title:?}");
    None
}

async fn fetch_art_itunes_only(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
) -> Option<String> {
    use base64::Engine;

    let query = format!("{artist} {title}");
    let search_url = reqwest::Url::parse_with_params(
        "https://itunes.apple.com/search",
        &[
            ("term", query.as_str()),
            ("entity", "song"),
            ("limit", "1"),
        ],
    )
    .ok()?;

    let resp = match client.get(search_url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[smtc] art: iTunes search request failed: {e}");
            return None;
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[smtc] art: iTunes JSON parse failed: {e}");
            return None;
        }
    };
    let first = body.get("results")?.as_array()?.first()?;
    let art_url_100 = first.get("artworkUrl100")?.as_str()?;

    let art_url = art_url_100.replace("100x100bb", "600x600bb");
    let bytes = match client.get(&art_url).send().await {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[smtc] art: iTunes image bytes read failed: {e}");
                return None;
            }
        },
        Err(e) => {
            eprintln!("[smtc] art: iTunes image fetch failed: {e}");
            return None;
        }
    };
    if bytes.is_empty() {
        return None;
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:image/jpeg;base64,{b64}"))
}

async fn fetch_art_deezer_only(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
) -> Option<String> {
    use base64::Engine;

    // Deezer accepts a single `q` term with quoted artist/track filters,
    // but a plain free-text query works just as well for our use case and
    // is more forgiving on minor metadata mismatches.
    let query = format!("{artist} {title}");
    let search_url = reqwest::Url::parse_with_params(
        "https://api.deezer.com/search",
        &[("q", query.as_str()), ("limit", "1")],
    )
    .ok()?;

    let resp = match client.get(search_url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[smtc] art: Deezer search request failed: {e}");
            return None;
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[smtc] art: Deezer JSON parse failed: {e}");
            return None;
        }
    };
    let first = body.get("data")?.as_array()?.first()?;
    // Deezer track payload exposes album.cover_xl (1000×1000),
    // cover_big (500), cover_medium (250), cover_small (56). Take XL.
    let art_url = first
        .get("album")?
        .get("cover_xl")
        .or_else(|| first.get("album")?.get("cover_big"))?
        .as_str()?;

    let bytes = match client.get(art_url).send().await {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[smtc] art: Deezer image bytes read failed: {e}");
                return None;
            }
        },
        Err(e) => {
            eprintln!("[smtc] art: Deezer image fetch failed: {e}");
            return None;
        }
    };
    if bytes.is_empty() {
        return None;
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:image/jpeg;base64,{b64}"))
}

/// Hard cap on thumbnail size we'll accept from SMTC. Real-world album art
/// is well under 1MB; anything larger is either a misbehaving source or a
/// hostile one trying to balloon our memory. 10MB is generous.
const MAX_THUMBNAIL_BYTES: u64 = 10 * 1024 * 1024;

fn read_thumbnail_bytes(
    props: &GlobalSystemMediaTransportControlsSessionMediaProperties,
) -> Result<Vec<u8>> {
    let thumb_ref = props.Thumbnail().context("Thumbnail()")?;
    let stream = thumb_ref
        .OpenReadAsync()
        .context("OpenReadAsync handle")?
        .get()
        .context("OpenReadAsync get")?;
    let size_u64 = stream.Size().context("Size()")?;
    if size_u64 == 0 {
        anyhow::bail!("empty thumbnail stream");
    }
    if size_u64 > MAX_THUMBNAIL_BYTES {
        anyhow::bail!("thumbnail too large: {size_u64} bytes (cap {MAX_THUMBNAIL_BYTES})");
    }
    // Cast safe after the cap check above.
    let size = size_u64 as u32;
    let reader = DataReader::CreateDataReader(&stream).context("CreateDataReader")?;
    reader
        .LoadAsync(size)
        .context("LoadAsync handle")?
        .get()
        .context("LoadAsync get")?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes).context("ReadBytes")?;
    Ok(bytes)
}

fn guess_image_mime(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 8 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.len() >= 3 && &bytes[..3] == b"GIF" {
        "image/gif"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        // SMTC almost always returns JPEG.
        "image/jpeg"
    }
}

/// Local mini-shape for the metadata read — assembled into the snapshot.
struct ReadTrack {
    title: String,
    artist: String,
    album: String,
    duration_ms: u64,
}

async fn read_track(session: &GlobalSystemMediaTransportControlsSession) -> Result<ReadTrack> {
    // Same blocking-on-async pattern as RequestAsync — see note in `run`.
    let session_for_blocking = session.clone();
    let (title, artist, album) = tokio::task::spawn_blocking(move || -> Result<_> {
        let props = session_for_blocking
            .TryGetMediaPropertiesAsync()?
            .get()?;
        let title = props.Title().unwrap_or_default().to_string();
        let artist = props.Artist().unwrap_or_default().to_string();
        let album = props.AlbumTitle().unwrap_or_default().to_string();
        Ok((title, artist, album))
    })
    .await??;

    let duration_ms = read_timeline(session).map(|t| t.1).unwrap_or(0);
    Ok(ReadTrack {
        title,
        artist,
        album,
        duration_ms,
    })
}

/// Returns (position_ms, duration_ms, last_update_unix_ms).
fn read_timeline(session: &GlobalSystemMediaTransportControlsSession) -> Result<(u64, u64, i64)> {
    let t = session.GetTimelineProperties()?;

    // TimeSpan.Duration is i64 in 100-nanosecond ticks.
    let position_ticks = t.Position()?.Duration.max(0);
    let end_ticks = t.EndTime()?.Duration.max(0);
    let start_ticks = t.StartTime()?.Duration.max(0);

    let position_ms = (position_ticks / 10_000) as u64;
    let duration_ms = ((end_ticks - start_ticks).max(0) / 10_000) as u64;

    // DateTime.UniversalTime is i64 100ns ticks since 1601-01-01 UTC.
    // Convert to Unix epoch ms (seconds between 1601-01-01 and 1970-01-01 = 11644473600).
    let universal_ticks = t.LastUpdatedTime()?.UniversalTime;
    const TICKS_BETWEEN_EPOCHS: i64 = 11_644_473_600 * 10_000_000;
    let last_update_unix_ms = (universal_ticks - TICKS_BETWEEN_EPOCHS) / 10_000;

    Ok((position_ms, duration_ms, last_update_unix_ms))
}

fn read_state(session: &GlobalSystemMediaTransportControlsSession) -> Result<PlaybackState> {
    let info = session.GetPlaybackInfo()?;
    Ok(info.PlaybackStatus()?.into())
}
