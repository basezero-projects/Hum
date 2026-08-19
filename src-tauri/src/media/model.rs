use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;

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

#[derive(Clone, Serialize, Debug, Default)]
pub struct CurrentTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub position_ms: u64,
    /// Unix epoch ms when the media source last reported the position. The
    /// frontend uses `position_ms + (now - last_update_unix_ms)` to interpolate
    /// while playing.
    pub last_update_unix_ms: i64,
    pub state: PlaybackState,
    /// For example, "Spotify.exe" or a browser AUMID. Useful for debugging
    /// and future per-source behavior.
    pub source_app_id: Option<String>,
    /// True when the current source is playing an ad break.
    #[serde(default)]
    pub ad_active: bool,
    /// Identifies an authoritative browser bridge when one enriches the
    /// effective snapshot with metadata that the raw media source lacks.
    #[serde(default)]
    pub bridge_source: Option<String>,
}

pub type SharedSnapshot = Arc<RwLock<CurrentTrack>>;

#[derive(Clone, Serialize)]
pub struct AlbumArtPayload {
    pub title: String,
    pub artist: String,
    pub data_url: String,
}

/// Last emitted album artwork, retained so late frontend subscribers can
/// request the current image after startup.
pub type SharedAlbumArt = Arc<RwLock<Option<AlbumArtPayload>>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_track_preserves_wire_contract() {
        let value = serde_json::to_value(CurrentTrack::default()).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "title": "",
                "artist": "",
                "album": "",
                "duration_ms": 0,
                "position_ms": 0,
                "last_update_unix_ms": 0,
                "state": "unknown",
                "source_app_id": null,
                "ad_active": false,
                "bridge_source": null
            })
        );
    }

    #[test]
    fn playback_states_preserve_lowercase_wire_values() {
        let states = [
            (PlaybackState::Unknown, "unknown"),
            (PlaybackState::Closed, "closed"),
            (PlaybackState::Opened, "opened"),
            (PlaybackState::Changing, "changing"),
            (PlaybackState::Stopped, "stopped"),
            (PlaybackState::Playing, "playing"),
            (PlaybackState::Paused, "paused"),
        ];

        for (state, expected) in states {
            assert_eq!(serde_json::to_value(state).unwrap(), expected);
        }
    }

    #[test]
    fn album_art_payload_preserves_wire_contract() {
        let value = serde_json::to_value(AlbumArtPayload {
            title: "A Song".into(),
            artist: "An Artist".into(),
            data_url: "data:image/png;base64,aHVt".into(),
        })
        .unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "title": "A Song",
                "artist": "An Artist",
                "data_url": "data:image/png;base64,aHVt"
            })
        );
    }
}
