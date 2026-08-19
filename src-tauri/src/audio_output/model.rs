#![cfg_attr(not(windows), allow(dead_code))]

use std::cmp::Ordering;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioOutputRoute {
    Wired,
    Speakers,
    Bluetooth,
    Hdmi,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AudioOutputDevice {
    pub id: String,
    pub display_name: String,
    pub route: AudioOutputRoute,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct AudioOutputState {
    pub outputs: Vec<AudioOutputDevice>,
    pub active: Option<AudioOutputDevice>,
}

impl AudioOutputState {
    pub fn normalized(mut outputs: Vec<AudioOutputDevice>, active_id: Option<&str>) -> Self {
        outputs.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.display_name.cmp(&right.display_name))
                .then_with(|| route_order(left.route, right.route))
        });
        outputs.dedup_by(|left, right| left.id == right.id);
        let active = active_id
            .and_then(|id| outputs.iter().find(|output| output.id == id))
            .cloned();
        Self { outputs, active }
    }
}

fn route_order(left: AudioOutputRoute, right: AudioOutputRoute) -> Ordering {
    left.cmp(&right)
}

pub type SharedAudioOutputState = Arc<RwLock<AudioOutputState>>;

pub fn new_shared_state() -> SharedAudioOutputState {
    Arc::new(RwLock::new(AudioOutputState::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, display_name: &str, route: AudioOutputRoute) -> AudioOutputDevice {
        AudioOutputDevice {
            id: id.to_string(),
            display_name: display_name.to_string(),
            route,
        }
    }

    #[test]
    fn routes_and_device_preserve_exact_wire_contract() {
        for (route, wire) in [
            (AudioOutputRoute::Wired, "wired"),
            (AudioOutputRoute::Speakers, "speakers"),
            (AudioOutputRoute::Bluetooth, "bluetooth"),
            (AudioOutputRoute::Hdmi, "hdmi"),
            (AudioOutputRoute::Unknown, "unknown"),
        ] {
            assert_eq!(serde_json::to_value(route).unwrap(), wire);
        }

        let output = device(
            "{0.0.0.00000000}.{opaque-endpoint-id}",
            "Studio Headphones",
            AudioOutputRoute::Wired,
        );
        assert_eq!(
            serde_json::to_value(&output).unwrap(),
            serde_json::json!({
                "id": "{0.0.0.00000000}.{opaque-endpoint-id}",
                "display_name": "Studio Headphones",
                "route": "wired"
            })
        );
    }

    #[test]
    fn opaque_endpoint_ids_round_trip_unchanged() {
        let id = "{0.0.0.00000000}.{A1B2-C3D4}\\raw endpoint";
        let output = device(id, "Output", AudioOutputRoute::Unknown);
        let value = serde_json::to_value(output).unwrap();
        let decoded: AudioOutputDevice = serde_json::from_value(value).unwrap();

        assert_eq!(decoded.id, id);
    }

    #[test]
    fn normalization_sorts_and_resolves_duplicate_ids_deterministically() {
        let inputs = vec![
            device("z", "Last", AudioOutputRoute::Unknown),
            device("duplicate", "Zulu", AudioOutputRoute::Speakers),
            device("a", "First", AudioOutputRoute::Bluetooth),
            device("duplicate", "Alpha", AudioOutputRoute::Wired),
        ];
        let normalized = AudioOutputState::normalized(inputs.clone(), Some("duplicate"));
        let reversed =
            AudioOutputState::normalized(inputs.into_iter().rev().collect(), Some("duplicate"));

        assert_eq!(
            normalized
                .outputs
                .iter()
                .map(|output| output.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "duplicate", "z"]
        );
        assert_eq!(normalized.outputs[1].display_name, "Alpha");
        assert_eq!(normalized.outputs[1].route, AudioOutputRoute::Wired);
        assert_eq!(normalized.active, Some(normalized.outputs[1].clone()));
        assert_eq!(reversed, normalized);
    }

    #[test]
    fn empty_inventory_has_no_active_output() {
        let normalized = AudioOutputState::normalized(vec![], Some("missing"));

        assert!(normalized.outputs.is_empty());
        assert!(normalized.active.is_none());
    }
}
