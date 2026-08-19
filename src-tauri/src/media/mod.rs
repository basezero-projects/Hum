mod backend;
mod model;
mod publisher;

pub(crate) use backend::{
    should_publish_bridge_timeline, should_publish_itunes, smtc_publication_policy, MediaBackend,
    MediaBackendContext, SmtcPublication,
};
pub use model::{AlbumArtPayload, CurrentTrack, PlaybackState, SharedAlbumArt, SharedSnapshot};
pub(crate) use publisher::{MediaPublisher, SnapshotEvent};
