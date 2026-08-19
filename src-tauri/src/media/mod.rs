#[cfg(any(windows, test))]
mod backend;
mod model;
#[cfg(any(windows, test))]
mod publisher;

#[cfg(windows)]
pub(crate) use backend::{
    should_publish_bridge_timeline, should_publish_itunes, smtc_publication_policy, MediaBackend,
    MediaBackendContext, SmtcPublication,
};
pub use model::{AlbumArtPayload, CurrentTrack, PlaybackState, SharedAlbumArt, SharedSnapshot};
#[cfg(windows)]
pub(crate) use publisher::{MediaPublisher, SnapshotEvent};
