use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use super::{AlbumArtPayload, CurrentTrack, SharedAlbumArt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SnapshotEvent {
    Track,
    Timeline,
    Playback,
}

impl SnapshotEvent {
    const fn name(self) -> &'static str {
        match self {
            Self::Track => "track-changed",
            Self::Timeline => "timeline-changed",
            Self::Playback => "playback-state-changed",
        }
    }
}

const ALBUM_ART_EVENT: &str = "album-art-loaded";
const FULL_REFRESH_ORDER: [SnapshotEvent; 3] = [
    SnapshotEvent::Track,
    SnapshotEvent::Timeline,
    SnapshotEvent::Playback,
];

/// Publishes complete media snapshots while keeping the raw snapshot owned by
/// the source workers. Artwork alone owns a shared cache write, which happens
/// before its notification is emitted.
#[derive(Clone)]
pub(crate) struct MediaPublisher {
    events: Arc<dyn MediaEventSink>,
    album_art: SharedAlbumArt,
}

impl MediaPublisher {
    pub(crate) fn new(app: AppHandle, album_art: SharedAlbumArt) -> Self {
        Self::with_sink(Arc::new(TauriMediaEventSink(app)), album_art)
    }

    fn with_sink(events: Arc<dyn MediaEventSink>, album_art: SharedAlbumArt) -> Self {
        Self { events, album_art }
    }

    pub(crate) fn publish_track(&self, snapshot: &CurrentTrack) {
        self.publish_snapshot(SnapshotEvent::Track, snapshot);
    }

    pub(crate) fn publish_timeline(&self, snapshot: &CurrentTrack) {
        self.publish_snapshot(SnapshotEvent::Timeline, snapshot);
    }

    pub(crate) fn publish_playback(&self, snapshot: &CurrentTrack) {
        self.publish_snapshot(SnapshotEvent::Playback, snapshot);
    }

    pub(crate) const fn full_refresh_order() -> [SnapshotEvent; 3] {
        FULL_REFRESH_ORDER
    }

    pub(crate) fn publish_event(&self, event: SnapshotEvent, snapshot: &CurrentTrack) {
        match event {
            SnapshotEvent::Track => self.publish_track(snapshot),
            SnapshotEvent::Timeline => self.publish_timeline(snapshot),
            SnapshotEvent::Playback => self.publish_playback(snapshot),
        }
    }

    pub(crate) async fn publish_artwork(&self, payload: AlbumArtPayload) {
        {
            let mut cached = self.album_art.write().await;
            *cached = Some(payload.clone());
        }
        self.events.emit_artwork(ALBUM_ART_EVENT, &payload);
    }

    fn publish_snapshot(&self, event: SnapshotEvent, snapshot: &CurrentTrack) {
        self.events.emit_snapshot(event.name(), snapshot);
    }
}

trait MediaEventSink: Send + Sync {
    fn emit_snapshot(&self, event: &'static str, snapshot: &CurrentTrack);
    fn emit_artwork(&self, event: &'static str, payload: &AlbumArtPayload);
}

struct TauriMediaEventSink(AppHandle);

impl MediaEventSink for TauriMediaEventSink {
    fn emit_snapshot(&self, event: &'static str, snapshot: &CurrentTrack) {
        let _ = self.0.emit(event, snapshot);
    }

    fn emit_artwork(&self, event: &'static str, payload: &AlbumArtPayload) {
        let _ = self.0.emit(event, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publisher_event_names_remain_exact() {
        assert_eq!(SnapshotEvent::Track.name(), "track-changed");
        assert_eq!(SnapshotEvent::Timeline.name(), "timeline-changed");
        assert_eq!(SnapshotEvent::Playback.name(), "playback-state-changed");
        assert_eq!(ALBUM_ART_EVENT, "album-art-loaded");
    }

    #[test]
    fn full_smtc_refresh_publication_order_remains_exact() {
        assert_eq!(
            MediaPublisher::full_refresh_order(),
            [
                SnapshotEvent::Track,
                SnapshotEvent::Timeline,
                SnapshotEvent::Playback
            ],
        );
    }

    #[tokio::test]
    async fn artwork_is_cached_before_listener_observes_event() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use tokio::sync::RwLock;

        struct ArtworkListener {
            album_art: SharedAlbumArt,
            observed: Arc<AtomicBool>,
        }

        impl MediaEventSink for ArtworkListener {
            fn emit_snapshot(&self, _: &'static str, _: &CurrentTrack) {}

            fn emit_artwork(&self, event: &'static str, _: &AlbumArtPayload) {
                assert_eq!(event, "album-art-loaded");
                let cached = self
                    .album_art
                    .try_read()
                    .expect("art cache must be unlocked before artwork event")
                    .clone();
                let cached = cached.expect("artwork must be cached before notification");
                assert_eq!(cached.title, "Song");
                assert_eq!(cached.artist, "Artist");
                assert_eq!(cached.data_url, "data:image/png;base64,AA==");
                self.observed.store(true, Ordering::SeqCst);
            }
        }

        let album_art: SharedAlbumArt = Arc::new(RwLock::new(None));
        let observed = Arc::new(AtomicBool::new(false));
        let payload = AlbumArtPayload {
            title: "Song".into(),
            artist: "Artist".into(),
            data_url: "data:image/png;base64,AA==".into(),
        };
        let publisher = MediaPublisher::with_sink(
            Arc::new(ArtworkListener {
                album_art: album_art.clone(),
                observed: observed.clone(),
            }),
            album_art,
        );

        publisher.publish_artwork(payload).await;

        assert!(observed.load(Ordering::SeqCst));
    }
}
