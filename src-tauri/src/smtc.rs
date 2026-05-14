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
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, RwLock};
use windows::Foundation::{EventRegistrationToken, TypedEventHandler};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

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

/// Spawn the SMTC worker. Logs and exits if it can't initialize — the rest of
/// the app keeps running so the user can at least see the dev shell.
///
/// `smtc_playing` is set to `true` only when SMTC has an active session that
/// is *currently playing* — not merely attached. Other source modules (e.g.
/// the iTunes COM bridge) read this flag to decide whether to suppress their
/// own emissions. SMTC sessions can hang around in Paused/Stopped/Closed states
/// long after a tab closed (Chrome is notorious for this), so "session exists"
/// is too coarse to use as a priority signal.
pub fn start(app: AppHandle, snapshot: SharedSnapshot, smtc_playing: Arc<AtomicBool>) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run(app, snapshot, smtc_playing).await {
            eprintln!("[smtc] worker exited: {e:#}");
        }
    });
}

async fn run(
    app: AppHandle,
    snapshot: SharedSnapshot,
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

    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();

    // Manager-level: fires when the foreground media source changes (e.g. user
    // switches from Spotify to YouTube in Chrome).
    let tx_session = tx.clone();
    let _session_token = manager.CurrentSessionChanged(&TypedEventHandler::new(
        move |_, _| {
            let _ = tx_session.send(Msg::SessionChanged);
            Ok(())
        },
    ))?;

    let mut hooks: Option<SessionHooks> = attach_session(&manager, &tx).ok();
    if let Some(ref h) = hooks {
        let state = read_state(&h.session).unwrap_or_default();
        smtc_playing.store(state == PlaybackState::Playing, Ordering::Relaxed);
        emit_full(&app, &snapshot, &h.session).await;
    } else {
        smtc_playing.store(false, Ordering::Relaxed);
    }

    while let Some(msg) = rx.recv().await {
        match msg {
            Msg::SessionChanged => {
                hooks = attach_session(&manager, &tx).ok();
                if let Some(ref h) = hooks {
                    let state = read_state(&h.session).unwrap_or_default();
                    smtc_playing.store(state == PlaybackState::Playing, Ordering::Relaxed);
                    emit_full(&app, &snapshot, &h.session).await;
                } else {
                    // No active session — clear the snapshot and notify.
                    smtc_playing.store(false, Ordering::Relaxed);
                    let mut snap = snapshot.write().await;
                    *snap = CurrentTrack::default();
                    let _ = app.emit("track-changed", &*snap);
                    let _ = app.emit("playback-state-changed", &*snap);
                }
            }
            Msg::MediaChanged => {
                if let Some(ref h) = hooks {
                    if let Ok(track) = read_track(&h.session).await {
                        let mut snap = snapshot.write().await;
                        snap.title = track.title;
                        snap.artist = track.artist;
                        snap.album = track.album;
                        snap.duration_ms = track.duration_ms;
                        let _ = app.emit("track-changed", &*snap);
                    }
                }
            }
            Msg::TimelineChanged => {
                if let Some(ref h) = hooks {
                    if let Ok((position_ms, duration_ms, last_update)) = read_timeline(&h.session) {
                        let mut snap = snapshot.write().await;
                        snap.position_ms = position_ms;
                        if duration_ms > 0 {
                            snap.duration_ms = duration_ms;
                        }
                        snap.last_update_unix_ms = last_update;
                        let _ = app.emit("timeline-changed", &*snap);
                    }
                }
            }
            Msg::PlaybackChanged => {
                if let Some(ref h) = hooks {
                    if let Ok(state) = read_state(&h.session) {
                        smtc_playing.store(state == PlaybackState::Playing, Ordering::Relaxed);
                        let mut snap = snapshot.write().await;
                        snap.state = state;
                        let _ = app.emit("playback-state-changed", &*snap);
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

    let snap = snapshot.read().await;
    let _ = app.emit("track-changed", &*snap);
    let _ = app.emit("timeline-changed", &*snap);
    let _ = app.emit("playback-state-changed", &*snap);
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
