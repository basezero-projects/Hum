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
    /// True when the current source is playing an ad break (Spotify's
    /// "Advertisement" track, Pandora's ad interlude, YouTube's ad rolls).
    /// Drives the overlay to render the SYVR promo card in place of lyrics.
    #[serde(default)]
    pub ad_active: bool,
}

pub type SharedSnapshot = Arc<RwLock<CurrentTrack>>;

/// Heuristic detector for Spotify ad breaks via SMTC metadata patterns.
///
/// Spotify keeps publishing track metadata during ads, just with telltale
/// patterns:
/// - title == "Advertisement" (the most explicit case)
/// - title == "Spotify" (Spotify rotates "Spotify" through this slot too)
/// - artist == "Spotify" with a non-empty title (ad copy in the title slot)
/// - empty title + empty artist while Playing (rare but observed)
///
/// All matches require the source to be Spotify (matched on `source_app_id`
/// containing "spotify" case-insensitively).
pub(crate) fn is_spotify_ad(t: &CurrentTrack) -> bool {
    let src = t.source_app_id.as_deref().unwrap_or("").to_lowercase();
    if !src.contains("spotify") {
        return false;
    }

    let title = t.title.trim();
    let artist = t.artist.trim();

    // Spotify's own house ads ("Listen to music, ad-free", etc.) use
    // explicit signals in the title/artist slots.
    if title.eq_ignore_ascii_case("Advertisement") { return true; }
    if title.eq_ignore_ascii_case("Spotify") { return true; }
    if artist.eq_ignore_ascii_case("Spotify") && !title.is_empty() { return true; }

    if title.is_empty() && artist.is_empty() && t.state == PlaybackState::Playing {
        return true;
    }

    // Third-party Spotify ads (Hotels.com, TikTok, BINI promos, etc.) DON'T
    // get any of the explicit signals above — Spotify publishes them with
    // arbitrary title/artist strings (`"—"`, `"LISTEN NOW"`, etc.) and the
    // advertiser's own branding. The remaining reliable signal is duration:
    // Spotify ads are typically 15-30s; real songs are virtually never under
    // ~60s. Combined with the source-is-spotify gate above and an actively-
    // playing state, sub-35s tracks are ads.
    //
    // False-positive risk: legitimate sub-35s songs (intro tracks, skits,
    // sound effects). Rare on Spotify; acceptable trade-off vs the much more
    // common case of third-party ads going undetected.
    if t.duration_ms > 0
        && t.duration_ms < 35_000
        && t.state == PlaybackState::Playing
    {
        return true;
    }

    false
}

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

/// Emit a Tauri event, but defer to the web_bridge worker when a
/// position-supplying probe (Pandora desktop, etc.) is currently
/// authoritative. Without this, SMTC's own `timeline-changed` emits —
/// fired whenever iTunes' COM session pings — would yank the frontend's
/// track/position back to iTunes between bridge polls, causing the
/// "lyrics jumping around" failure mode that v0.11.4/v0.11.5 still
/// showed.
///
/// Suppression model: when `bridge_is_authoritative` returns true, this
/// function returns without emitting. The bridge worker emits its own
/// `timeline-changed` events every 2 s, so the frontend stays in sync
/// without our help. When bridge data is `None` (Pandora.exe closed) or
/// truly stale beyond the authority window, we fall through to the
/// regular blended emit — keeping the original "Pandora-in-Chrome
/// bridge fills in title; SMTC owns position" behavior for the
/// `PandoraProbe` (Chrome) case.
async fn emit_blended(
    app: &AppHandle,
    snapshot: &SharedSnapshot,
    event: &str,
    mut snap: CurrentTrack,
) {
    use tauri::Manager;

    // Spotify ad detection runs before the tier-1 priority check so that
    // the ad_active flag rides on the snapshot regardless of playback state.
    //
    // Set / clear semantics:
    //   - Spotify ad detected → ad_active = true
    //   - Spotify real song (source is Spotify, duration ≥ 35s) → ad_active
    //     = false. This clear is critical: without it, transitioning from
    //     a Spotify ad break to the next real song would leave the overlay
    //     stuck on the SYVR promo card forever (emit_snap inherits the
    //     prior ad_active = true through snapshot.clone(), is_spotify_ad
    //     returns false for the new real song, and any conditional like
    //     `if is_spotify_ad { = true }` would never clear it).
    //   - Non-Spotify source (Chrome with Pandora tab, iTunes, etc.) →
    //     LEAVE ad_active alone. The bridge worker (web_bridge.rs) owns
    //     the flag for those sources via its own emit-and-sync path; if
    //     we unconditionally cleared here, a Pandora-web ad detected by
    //     the bridge would be clobbered by SMTC's parallel MediaChanged
    //     emit for the same Chrome session.
    let detected_ad = is_spotify_ad(&snap);
    let is_spotify_source = snap
        .source_app_id
        .as_deref()
        .unwrap_or("")
        .to_lowercase()
        .contains("spotify");
    if detected_ad {
        snap.ad_active = true;
    } else if is_spotify_source && snap.duration_ms >= 35_000 {
        snap.ad_active = false;
    }

    // Sync the SMTC-derived ad_active to the shared snapshot. The lyrics
    // resolver reads the shared snapshot (not the local emit-path `snap`)
    // and short-circuits on `ad_active` to render the SYVR promo card.
    // Without this write, the frontend's `track` state (from the emit
    // payload below) would correctly show `ad_active = true` (driving the
    // AD BREAK chip in MetadataColumn) while the resolver still sees the
    // stale `false` on the shared snapshot — leading to the chip firing
    // but the lyric area showing the "fetching" / "no lyrics" status line
    // instead of the promo card. Sync ALWAYS, regardless of which tier
    // emits below (tier-2 returns early without emitting; the bridge
    // worker's parallel emit path also writes ad_active back — see
    // web_bridge.rs).
    {
        let mut shared = snapshot.write().await;
        shared.ad_active = snap.ad_active;
    }

    // Tier 1: SMTC has a real actively-playing session (Spotify / iTunes /
    // YouTube via Chrome / etc.). Real publishers always beat the bridge's
    // estimated position. Emit raw and skip the bridge entirely so a paused
    // Pandora session can't keep eating the airwaves.
    let smtc_actively_playing =
        snap.state == PlaybackState::Playing && !snap.title.trim().is_empty();
    if smtc_actively_playing {
        let _ = app.emit(event, &snap);
        return;
    }
    // Tier 2: defer to the bridge if a position-supplying probe is currently
    // authoritative (Pandora desktop with Hyperlinks present + state =
    // Playing). The bridge worker emits its own timeline-changed events, so
    // suppress ours to avoid two streams racing into the frontend.
    if let Some(bridge_state) = app.try_state::<crate::web_bridge::SharedWebBridge>() {
        if crate::web_bridge::bridge_is_authoritative(bridge_state.inner()).await {
            return;
        }
        crate::web_bridge::blend_bridge_into_snapshot(&mut snap, bridge_state.inner()).await;
        // Bridge blend may have flipped ad_active to true via bt.is_ad
        // (Pandora ad detection). Re-sync to shared so the resolver sees it.
        {
            let mut shared = snapshot.write().await;
            shared.ad_active = snap.ad_active;
        }
    }
    // Tier 3: nothing claimed authority — emit whatever SMTC has (possibly
    // stale, possibly Paused/Stopped from an idle app).
    let _ = app.emit(event, &snap);
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
                    let emit_snap = {
                        let mut snap = snapshot.write().await;
                        *snap = CurrentTrack::default();
                        snap.clone()
                    };
                    emit_blended(&app, &snapshot, "track-changed", emit_snap.clone()).await;
                    emit_blended(&app, &snapshot, "playback-state-changed", emit_snap).await;
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
                            let emit_snap = {
                                let mut snap = snapshot.write().await;
                                snap.title = track.title;
                                snap.artist = track.artist;
                                snap.album = track.album;
                                snap.duration_ms = track.duration_ms;
                                snap.clone()
                            };
                            emit_blended(&app, &snapshot, "track-changed", emit_snap).await;
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
                            let emit_snap = {
                                let mut snap = snapshot.write().await;
                                snap.position_ms = position_ms;
                                if duration_ms > 0 {
                                    snap.duration_ms = duration_ms;
                                }
                                snap.last_update_unix_ms = last_update;
                                snap.clone()
                            };
                            emit_blended(&app, &snapshot, "timeline-changed", emit_snap).await;
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
                            let emit_snap = {
                                let mut snap = snapshot.write().await;
                                snap.state = state;
                                snap.clone()
                            };
                            emit_blended(&app, &snapshot, "playback-state-changed", emit_snap).await;
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
        let emit_snap = {
            let snap = snapshot.read().await;
            eprintln!(
                "[smtc] emit_full → title='{}' artist='{}' state={:?} pos={}ms dur={}ms",
                snap.title, snap.artist, snap.state, snap.position_ms, snap.duration_ms
            );
            snap.clone()
        };
        emit_blended(app, snapshot, "track-changed", emit_snap.clone()).await;
        emit_blended(app, snapshot, "timeline-changed", emit_snap.clone()).await;
        emit_blended(app, snapshot, "playback-state-changed", emit_snap.clone()).await;
        (emit_snap.title.clone(), emit_snap.artist.clone())
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
/// Source preference (each variant tried in turn until one returns):
/// 1. iTunes Search API — Apple's catalog, 600×600 JPEG.
/// 2. Deezer Search API — different catalog (especially European /
///    non-US-charting tracks). 1000×1000 JPEG.
///
/// Query variants (tried in order until one of the sources returns
/// a hit). The retry chain mostly exists for YouTube-style metadata
/// where the SMTC artist field is a channel name (`"RockHype"`) and
/// the actual artist is in the title (`"Kelly Clarkson - Since U
/// Been Gone"`). Each variant is tried against iTunes first, then
/// Deezer, before moving to the next variant.
///   a. Original (artist, title) — works for clean SMTC metadata.
///   b. (title_prefix, title_suffix) when title contains " - " —
///      treats the title as `"Real Artist - Real Song"`, ignoring
///      the channel-name artist field.
///   c. ("", title) — title-only search, last resort when neither
///      the SMTC artist nor any prefix in title looks like a real
///      artist.
///
/// Returns a `data:image/jpeg;base64,...` URL on success, `None` when
/// every variant misses or all network steps fail.
pub async fn fetch_art_via_itunes(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
) -> Option<String> {
    // Variant a — try with the SMTC-supplied fields verbatim. Validate
    // returned records against the SMTC artist so iTunes/Deezer's free-
    // text-relevance ranking can't surface a wrong-artist track that
    // textually overlaps with the query (real failure: Lil Wayne's "Let It
    // All Work Out" was returning a T-Pain cover because iTunes ranked a
    // T-Pain track higher on the free-text query "Lil Wayne Let It All
    // Work Out", and v0.10.22's `limit=1` accepted whatever came back).
    if let Some(url) = try_one_variant(client, artist, title, artist, "as-is").await {
        return Some(url);
    }

    // Variant b — if the title is shaped `"Real Artist - Real Song"`,
    // try treating those halves as the real artist+title. Common for
    // YouTube uploads where the channel name (the artist field) is not
    // the performer. Validation uses the title-prefix as the expected
    // artist since the SMTC artist is presumed junk in this variant.
    if let Some((prefix, suffix)) = title.split_once(" - ") {
        let real_artist = prefix.trim();
        let real_title = suffix.trim();
        // Avoid degenerate splits (empty halves, single-letter prefixes)
        // and avoid redoing variant (a) when SMTC already supplied the
        // same artist text via the title-prefix.
        if !real_artist.is_empty()
            && !real_title.is_empty()
            && real_artist.len() >= 2
            && !real_artist.eq_ignore_ascii_case(artist)
        {
            if let Some(url) =
                try_one_variant(client, real_artist, real_title, real_artist, "title-split").await
            {
                return Some(url);
            }
        }
    }

    // Variant c — title-only search. The QUERY has no artist filter (so
    // catalogs return broader matches), but the VALIDATION still pins
    // against the SMTC artist so we don't accept a wrong-artist track
    // just because the title is generic.
    if let Some(url) = try_one_variant(client, "", title, artist, "title-only").await {
        return Some(url);
    }

    eprintln!("[smtc] art: all variants and sources missed for {artist:?} - {title:?}");
    None
}

/// One query variant tried against both iTunes and Deezer in order.
/// `query_artist` is what gets sent to the API (may be empty for title-only
/// queries); `validation_artist` is what the returned record's artistName
/// must match (case-insensitive, primary-artist-only — see
/// `primary_artist_matches`). An empty `validation_artist` disables the
/// artist filter (accept whatever ranks first).
/// `label` is logged so we can see which variant ended up matching.
async fn try_one_variant(
    client: &reqwest::Client,
    query_artist: &str,
    query_title: &str,
    validation_artist: &str,
    label: &str,
) -> Option<String> {
    if let Some(url) =
        fetch_art_itunes_only(client, query_artist, query_title, validation_artist).await
    {
        eprintln!(
            "[smtc] art: iTunes hit ({label}) for {validation_artist:?} - {query_title:?}"
        );
        return Some(url);
    }
    if let Some(url) =
        fetch_art_deezer_only(client, query_artist, query_title, validation_artist).await
    {
        eprintln!(
            "[smtc] art: Deezer hit ({label}) for {validation_artist:?} - {query_title:?}"
        );
        return Some(url);
    }
    None
}

async fn fetch_art_itunes_only(
    client: &reqwest::Client,
    query_artist: &str,
    query_title: &str,
    validation_artist: &str,
) -> Option<String> {
    use base64::Engine;

    let query = if query_artist.trim().is_empty() {
        query_title.to_string()
    } else {
        format!("{query_artist} {query_title}")
    };
    let search_url = reqwest::Url::parse_with_params(
        "https://itunes.apple.com/search",
        &[
            ("term", query.as_str()),
            ("entity", "song"),
            ("limit", "10"),
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
    let results = body.get("results")?.as_array()?;
    let chosen = pick_artist_matched(
        results.iter(),
        |r| {
            r.get("artistName")
                .and_then(|a| a.as_str())
                .map(|s| s.to_string())
        },
        validation_artist,
    )?;
    let art_url_100 = chosen.get("artworkUrl100")?.as_str()?;

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
    query_artist: &str,
    query_title: &str,
    validation_artist: &str,
) -> Option<String> {
    use base64::Engine;

    // Deezer accepts a single `q` term with quoted artist/track filters,
    // but a plain free-text query works just as well for our use case and
    // is more forgiving on minor metadata mismatches.
    let query = if query_artist.trim().is_empty() {
        query_title.to_string()
    } else {
        format!("{query_artist} {query_title}")
    };
    let search_url = reqwest::Url::parse_with_params(
        "https://api.deezer.com/search",
        &[("q", query.as_str()), ("limit", "10")],
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
    let results = body.get("data")?.as_array()?;
    let chosen = pick_artist_matched(
        results.iter(),
        |r| {
            r.get("artist")
                .and_then(|a| a.get("name"))
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        },
        validation_artist,
    )?;
    // Deezer track payload exposes album.cover_xl (1000×1000),
    // cover_big (500), cover_medium (250), cover_small (56). Take XL.
    let art_url = chosen
        .get("album")?
        .get("cover_xl")
        .or_else(|| chosen.get("album")?.get("cover_big"))?
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

/// Iterate over search results and pick the first one whose extracted
/// artist name fuzzy-matches `validation_artist`. Empty `validation_artist`
/// disables filtering (returns the first record). Returns the chosen
/// `&serde_json::Value` or `None` if no record passes validation.
fn pick_artist_matched<'a, I, F>(
    results: I,
    extract_artist: F,
    validation_artist: &str,
) -> Option<&'a serde_json::Value>
where
    I: IntoIterator<Item = &'a serde_json::Value>,
    F: Fn(&serde_json::Value) -> Option<String>,
{
    if validation_artist.trim().is_empty() {
        return results.into_iter().next();
    }
    for r in results {
        if let Some(rec_artist) = extract_artist(r) {
            if primary_artist_matches(&rec_artist, validation_artist) {
                return Some(r);
            }
        }
    }
    None
}

/// Does `rec_artist`'s primary artist (the part before `feat.`/`ft.`/`&`/
/// `,` separators) fuzzy-match `expected`?
///
/// The match is bidirectional substring containment after normalization
/// (lowercase + collapse common Unicode punctuation variants). Bidirectional
/// because legitimate artist-name variation goes both ways: SMTC reports
/// `"Beatles"`, iTunes returns `"The Beatles"` → "the beatles" contains
/// "beatles" → match. SMTC reports `"Lil Wayne"`, iTunes returns `"Lil
/// Wayne feat. T-Pain"` → primary is "Lil Wayne" → "lil wayne" contains
/// "lil wayne" → match. Wrong case: SMTC reports `"Lil Wayne"`, iTunes
/// returns `"T-Pain feat. Lil Wayne"` → primary is "T-Pain" → neither
/// contains the other → reject (correctly).
pub(crate) fn primary_artist_matches(rec_artist: &str, expected: &str) -> bool {
    let primary = primary_artist_token(rec_artist);
    let primary_norm = art_normalize(&primary);
    let expected_norm = art_normalize(expected);
    if primary_norm.is_empty() || expected_norm.is_empty() {
        return false;
    }
    primary_norm.contains(&expected_norm) || expected_norm.contains(&primary_norm)
}

/// Split an artist string at the first feat./ft./&/, separator and return
/// the head (the primary credited artist). Returns the trimmed input
/// unchanged if no separator is found.
pub(crate) fn primary_artist_token(rec_artist: &str) -> String {
    let lower = rec_artist.to_lowercase();
    // Order matters less than coverage. The leading space requirement
    // prevents `Sufjan Stevens` from being split on a non-existent `ft`
    // substring inside a real artist name.
    let separators = [
        " feat.", " feat ", " ft.", " ft ", " featuring ", " & ", " + ", ", ", "; ", " / ",
        " vs.", " vs ",
    ];
    let mut cut = rec_artist.len();
    for sep in separators {
        if let Some(idx) = lower.find(sep) {
            if idx < cut {
                cut = idx;
            }
        }
    }
    rec_artist[..cut].trim().to_string()
}

/// Lowercase + collapse common Unicode punctuation flavors into ASCII
/// equivalents. Smaller-scope sibling of `lyrics::normalize_for_match`
/// kept local to avoid pulling lyrics's full normalizer surface across
/// the module boundary.
fn art_normalize(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2019}' | '\u{2018}' | '\u{2032}' | '\u{2035}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{2033}' => '"',
            '\u{2013}' | '\u{2014}' | '\u{2012}' | '\u{2015}' => '-',
            '\u{00A0}' => ' ',
            _ => c,
        })
        .collect::<String>()
        .to_lowercase()
        .trim()
        .to_string()
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

#[cfg(test)]
mod is_spotify_ad_tests {
    use super::*;

    fn snap_with(title: &str, artist: &str, app_id: &str, state: PlaybackState) -> CurrentTrack {
        CurrentTrack {
            title: title.into(),
            artist: artist.into(),
            state,
            source_app_id: if app_id.is_empty() { None } else { Some(app_id.into()) },
            ..Default::default()
        }
    }

    #[test]
    fn non_spotify_source_never_ad() {
        let t = snap_with("Advertisement", "Spotify", "Chrome.exe", PlaybackState::Playing);
        assert!(!is_spotify_ad(&t));
    }

    #[test]
    fn spotify_title_advertisement_matches() {
        let t = snap_with("Advertisement", "", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_title_advertisement_case_insensitive() {
        let t = snap_with("advertisement", "", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_title_literal_spotify_matches() {
        let t = snap_with("Spotify", "", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_artist_field_spotify_with_nonempty_title_matches() {
        // Spotify sometimes sets artist="Spotify" and title=<some ad copy>.
        let t = snap_with("Try Premium Free", "Spotify", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_real_song_never_ad() {
        // Normal song length (3:42) — explicit so the duration heuristic
        // can't accidentally flag this as an ad if its threshold tightens.
        let mut t = snap_with("Mr. Brightside", "The Killers", "Spotify.exe", PlaybackState::Playing);
        t.duration_ms = 222_000;
        assert!(!is_spotify_ad(&t));
    }

    #[test]
    fn spotify_aumid_format_also_matches() {
        // Spotify also appears as AUMID `SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify`.
        let t = snap_with("Advertisement", "", "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_empty_title_and_artist_while_playing_matches() {
        let t = snap_with("", "", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_empty_while_paused_not_ad() {
        let t = snap_with("", "", "Spotify.exe", PlaybackState::Paused);
        assert!(!is_spotify_ad(&t));
    }

    // --- Duration heuristic — catches third-party ads that don't use
    // --- Spotify's explicit "Advertisement" / "Spotify" string signals.

    #[test]
    fn spotify_third_party_ad_with_short_duration_matches() {
        // Observed in real Spotify free ads: title is the advertiser's CTA,
        // artist is the advertiser's brand, duration is sub-30s.
        let mut t = snap_with("LISTEN NOW", "BINI", "Spotify.exe", PlaybackState::Playing);
        t.duration_ms = 29_000;
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_emdash_title_short_duration_matches() {
        // Hotels.com-style ad observed: title is just "—" (em-dash placeholder).
        let mut t = snap_with("—", "", "Spotify.exe", PlaybackState::Playing);
        t.duration_ms = 17_000;
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_short_duration_while_paused_not_ad() {
        // Don't flag paused tracks even when short — the heuristic gates on
        // Playing so a sub-35s paused track (e.g. user paused mid-intro) is
        // not mistaken for an active ad break.
        let mut t = snap_with("LISTEN NOW", "BINI", "Spotify.exe", PlaybackState::Paused);
        t.duration_ms = 29_000;
        assert!(!is_spotify_ad(&t));
    }

    #[test]
    fn spotify_zero_duration_does_not_trip_heuristic() {
        // Fresh track-changed events sometimes arrive with duration_ms = 0
        // before the full media properties resolve. Don't false-positive on
        // that — the explicit signals above still apply if present, but a
        // zero-duration track alone shouldn't be classified as an ad.
        let t = snap_with("Some Song", "Some Artist", "Spotify.exe", PlaybackState::Playing);
        assert_eq!(t.duration_ms, 0);
        assert!(!is_spotify_ad(&t));
    }

    #[test]
    fn spotify_duration_at_threshold_not_ad() {
        // Exactly 35s: should NOT match (the comparison is `< 35_000`).
        let mut t = snap_with("Short Song", "Artist", "Spotify.exe", PlaybackState::Playing);
        t.duration_ms = 35_000;
        assert!(!is_spotify_ad(&t));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_artist_token_strips_feat_variants() {
        // Plain artist — unchanged.
        assert_eq!(primary_artist_token("Lil Wayne"), "Lil Wayne");
        // feat./ft./featuring (case-insensitive).
        assert_eq!(primary_artist_token("Lil Wayne feat. T-Pain"), "Lil Wayne");
        assert_eq!(primary_artist_token("Lil Wayne ft. Drake"), "Lil Wayne");
        assert_eq!(primary_artist_token("Lil Wayne Feat T-Pain"), "Lil Wayne");
        assert_eq!(primary_artist_token("Lil Wayne featuring Drake"), "Lil Wayne");
        // Other separators.
        assert_eq!(primary_artist_token("Lil Wayne & Drake"), "Lil Wayne");
        assert_eq!(primary_artist_token("Lil Wayne + Drake"), "Lil Wayne");
        assert_eq!(primary_artist_token("Lil Wayne, Drake"), "Lil Wayne");
        assert_eq!(primary_artist_token("Lil Wayne; Drake"), "Lil Wayne");
        assert_eq!(primary_artist_token("Lil Wayne / Drake"), "Lil Wayne");
        // The TRUE primary in a feat. arrangement must be the leading
        // artist — this is the v0.10.26 bug case.
        assert_eq!(
            primary_artist_token("T-Pain feat. Lil Wayne"),
            "T-Pain"
        );
        // Empty.
        assert_eq!(primary_artist_token(""), "");
        // Whitespace only.
        assert_eq!(primary_artist_token("   "), "");
    }

    #[test]
    fn primary_artist_matches_accepts_real_artist() {
        // Exact match.
        assert!(primary_artist_matches("Lil Wayne", "Lil Wayne"));
        // Case-insensitive.
        assert!(primary_artist_matches("LIL WAYNE", "lil wayne"));
        // Feat. credit — primary is still Lil Wayne, validation passes.
        assert!(primary_artist_matches("Lil Wayne feat. T-Pain", "Lil Wayne"));
        assert!(primary_artist_matches("Lil Wayne & Drake", "Lil Wayne"));
        // Bidirectional substring — SMTC reports "Beatles", iTunes returns
        // "The Beatles".
        assert!(primary_artist_matches("The Beatles", "Beatles"));
        assert!(primary_artist_matches("Beatles", "The Beatles"));
        // Punctuation variants normalize.
        assert!(primary_artist_matches("AC/DC", "AC/DC"));
        // Curly apostrophe in record vs ASCII in SMTC.
        assert!(primary_artist_matches("Beyonc\u{00E9}", "Beyonc\u{00E9}"));
    }

    #[test]
    fn primary_artist_matches_rejects_wrong_artist() {
        // The v0.10.26 failure case: iTunes returns a T-Pain track when we
        // asked for Lil Wayne. Must reject.
        assert!(!primary_artist_matches("T-Pain", "Lil Wayne"));
        // T-Pain feat. Lil Wayne does NOT satisfy a Lil Wayne validation —
        // primary is T-Pain.
        assert!(!primary_artist_matches("T-Pain feat. Lil Wayne", "Lil Wayne"));
        // Completely unrelated.
        assert!(!primary_artist_matches("Drake", "Kendrick Lamar"));
        // Empty inputs — bail out rather than spuriously match.
        assert!(!primary_artist_matches("", "Lil Wayne"));
        assert!(!primary_artist_matches("Lil Wayne", ""));
        assert!(!primary_artist_matches("", ""));
    }

    #[test]
    fn pick_artist_matched_accepts_first_on_empty_validation() {
        // Empty validation_artist passes the first record through (matches
        // variant (c) call sites where the caller deliberately disables
        // validation — though those don't happen in current code).
        let results = [
            serde_json::json!({ "artistName": "T-Pain", "id": 1 }),
            serde_json::json!({ "artistName": "Lil Wayne", "id": 2 }),
        ];
        let chosen = pick_artist_matched(
            results.iter(),
            |r| r.get("artistName").and_then(|a| a.as_str()).map(String::from),
            "",
        );
        assert_eq!(chosen.and_then(|r| r.get("id")?.as_i64()), Some(1));
    }

    #[test]
    fn pick_artist_matched_skips_to_matching_record() {
        // Simulates the Lil Wayne case: iTunes returns a T-Pain track
        // first, the real Lil Wayne track second. Validation must skip
        // T-Pain and pick the Lil Wayne record.
        let results = [
            serde_json::json!({ "artistName": "T-Pain", "id": 1 }),
            serde_json::json!({ "artistName": "Lil Wayne", "id": 2 }),
        ];
        let chosen = pick_artist_matched(
            results.iter(),
            |r| r.get("artistName").and_then(|a| a.as_str()).map(String::from),
            "Lil Wayne",
        );
        assert_eq!(chosen.and_then(|r| r.get("id")?.as_i64()), Some(2));
    }

    #[test]
    fn pick_artist_matched_returns_none_when_no_record_matches() {
        // 10 results, none by the requested artist → reject (better no art
        // than wrong art).
        let results: Vec<serde_json::Value> = (0..10)
            .map(|i| serde_json::json!({ "artistName": format!("Wrong Artist {i}"), "id": i }))
            .collect();
        let chosen = pick_artist_matched(
            results.iter(),
            |r| r.get("artistName").and_then(|a| a.as_str()).map(String::from),
            "Lil Wayne",
        );
        assert!(chosen.is_none());
    }
}
