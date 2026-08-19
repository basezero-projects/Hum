use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::AppHandle;

use crate::lyrics::SharedLyrics;

use super::{CurrentTrack, PlaybackState, SharedAlbumArt, SharedSnapshot};

/// Shared state required to start a platform media backend. The context stays
/// free of native handles and platform-specific playback types.
pub(crate) struct MediaBackendContext {
    pub app: AppHandle,
    pub snapshot: SharedSnapshot,
    pub album_art: SharedAlbumArt,
    pub lyrics: SharedLyrics,
    pub smtc_playing: Arc<AtomicBool>,
}

/// Lifecycle boundary for a platform playback implementation.
pub(crate) trait MediaBackend {
    fn start(self, context: MediaBackendContext);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SmtcPublication {
    Raw,
    Suppress,
    Blend,
}

/// Decide whether an SMTC observation publishes raw, yields to a bridge, or
/// publishes after bridge enrichment. This encodes the existing Windows source
/// authority without coupling the policy to Tauri or Windows APIs.
pub(crate) fn smtc_publication_policy(
    snapshot: &CurrentTrack,
    bridge_is_authoritative: bool,
) -> SmtcPublication {
    let smtc_is_active =
        snapshot.state == PlaybackState::Playing && !snapshot.title.trim().is_empty();
    if smtc_is_active {
        SmtcPublication::Raw
    } else if bridge_is_authoritative {
        SmtcPublication::Suppress
    } else {
        SmtcPublication::Blend
    }
}

pub(crate) fn should_publish_itunes(smtc_playing: bool) -> bool {
    !smtc_playing
}

pub(crate) fn should_publish_bridge_timeline(
    has_bridge_position: bool,
    raw_snapshot: &CurrentTrack,
) -> bool {
    has_bridge_position
        && (raw_snapshot.state != PlaybackState::Playing || raw_snapshot.title.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{CurrentTrack, PlaybackState};

    fn snapshot(state: PlaybackState, title: &str) -> CurrentTrack {
        CurrentTrack {
            state,
            title: title.to_string(),
            ..CurrentTrack::default()
        }
    }

    #[test]
    fn playing_smtc_publishes_raw_even_with_authoritative_bridge() {
        assert_eq!(
            smtc_publication_policy(&snapshot(PlaybackState::Playing, "Song"), true,),
            SmtcPublication::Raw,
        );
    }

    #[test]
    fn inactive_smtc_with_authoritative_bridge_is_suppressed() {
        assert_eq!(
            smtc_publication_policy(&snapshot(PlaybackState::Stopped, "Old song"), true,),
            SmtcPublication::Suppress,
        );
    }

    #[test]
    fn inactive_smtc_without_authoritative_bridge_is_blended() {
        assert_eq!(
            smtc_publication_policy(&snapshot(PlaybackState::Stopped, "Old song"), false,),
            SmtcPublication::Blend,
        );
    }

    #[test]
    fn paused_smtc_is_not_active_authority() {
        let paused = snapshot(PlaybackState::Paused, "Song");
        assert_eq!(
            smtc_publication_policy(&paused, false),
            SmtcPublication::Blend,
        );
        assert_eq!(
            smtc_publication_policy(&paused, true),
            SmtcPublication::Suppress,
        );
    }

    #[test]
    fn itunes_publishes_only_when_smtc_is_not_playing() {
        assert!(!should_publish_itunes(true));
        assert!(should_publish_itunes(false));
    }

    #[test]
    fn bridge_timeline_requires_position() {
        assert!(!should_publish_bridge_timeline(
            false,
            &snapshot(PlaybackState::Stopped, "Song"),
        ));
    }

    #[test]
    fn bridge_timeline_yields_to_non_empty_playing_smtc() {
        assert!(!should_publish_bridge_timeline(
            true,
            &snapshot(PlaybackState::Playing, "Song"),
        ));
    }

    #[test]
    fn bridge_timeline_can_publish_against_non_authoritative_raw_states() {
        for raw in [
            snapshot(PlaybackState::Paused, "Song"),
            snapshot(PlaybackState::Stopped, "Song"),
            snapshot(PlaybackState::Unknown, ""),
            snapshot(PlaybackState::Unknown, "Stale song"),
            snapshot(PlaybackState::Playing, ""),
        ] {
            assert!(should_publish_bridge_timeline(true, &raw));
        }
    }
}
