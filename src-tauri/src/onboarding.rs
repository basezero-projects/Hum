use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio_output::model::{AudioOutputRoute, SharedAudioOutputState};
use crate::license::{LicenseService, LicenseStatus};
use crate::mode::{apply_mode, OverlayMode};
use crate::settings::{self, Settings, SharedSettings};

pub(crate) const CURRENT_ONBOARDING_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CustomerWindowPlan {
    pub activation: bool,
    pub setup: bool,
    pub overlay: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct OnboardingState {
    pub version: u8,
    pub current_version: u8,
    pub completed: bool,
    pub recommended_listening_mode: Option<String>,
}

pub(crate) fn onboarding_completed(version: u8) -> bool {
    version >= CURRENT_ONBOARDING_VERSION
}

pub(crate) fn completion_allowed(status: LicenseStatus) -> bool {
    status.is_licensed()
}

pub(crate) fn customer_window_plan(
    status: LicenseStatus,
    onboarding_version: u8,
) -> CustomerWindowPlan {
    if !status.is_licensed() {
        return CustomerWindowPlan {
            activation: true,
            setup: false,
            overlay: false,
        };
    }
    if !onboarding_completed(onboarding_version) {
        return CustomerWindowPlan {
            activation: false,
            setup: true,
            overlay: true,
        };
    }
    CustomerWindowPlan {
        activation: false,
        setup: false,
        overlay: true,
    }
}

pub(crate) fn listening_mode_for_route(route: AudioOutputRoute) -> Option<&'static str> {
    match route {
        AudioOutputRoute::Wired => Some("wired"),
        AudioOutputRoute::Speakers | AudioOutputRoute::Hdmi => Some("speakers"),
        AudioOutputRoute::Bluetooth => Some("bluetooth"),
        AudioOutputRoute::Unknown => None,
    }
}

fn onboarding_state(
    settings: &Settings,
    recommended_listening_mode: Option<&str>,
) -> OnboardingState {
    OnboardingState {
        version: settings.onboarding_version,
        current_version: CURRENT_ONBOARDING_VERSION,
        completed: onboarding_completed(settings.onboarding_version),
        recommended_listening_mode: recommended_listening_mode.map(str::to_string),
    }
}

fn set_window_visibility(app: &AppHandle, label: &str, visible: bool, focus: bool) {
    let Some(window) = app.get_webview_window(label) else {
        return;
    };
    if visible {
        let _ = window.show();
        let _ = window.unminimize();
        if focus {
            let _ = window.set_focus();
        }
    } else {
        let _ = window.hide();
    }
}

pub(crate) fn apply_customer_windows(
    app: &AppHandle,
    status: LicenseStatus,
    onboarding_version: u8,
) {
    let plan = customer_window_plan(status, onboarding_version);
    if !plan.activation {
        set_window_visibility(app, "activation", false, false);
    }
    if !plan.setup {
        set_window_visibility(app, "setup", false, false);
    }
    set_window_visibility(app, "overlay", plan.overlay, false);
    if plan.activation {
        set_window_visibility(app, "activation", true, true);
    }
    if plan.setup {
        set_window_visibility(app, "setup", true, true);
    }
    if plan.setup {
        apply_mode(app, OverlayMode::Edit);
    }
}

pub(crate) fn open_onboarding_session(
    app: &AppHandle,
    status: LicenseStatus,
) -> Result<(), String> {
    if !completion_allowed(status) {
        apply_customer_windows(app, status, 0);
        return Err("Activate Hum before running setup.".to_string());
    }
    set_window_visibility(app, "activation", false, false);
    set_window_visibility(app, "overlay", true, false);
    set_window_visibility(app, "setup", true, true);
    apply_mode(app, OverlayMode::Edit);
    let _ = app.emit("onboarding-opened", ());
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_onboarding_state(
    settings: tauri::State<'_, SharedSettings>,
    audio_outputs: tauri::State<'_, SharedAudioOutputState>,
) -> Result<OnboardingState, String> {
    let current = settings.read().await;
    let recommended = audio_outputs.read().ok().and_then(|state| {
        state
            .active
            .as_ref()
            .and_then(|output| listening_mode_for_route(output.route))
    });
    Ok(onboarding_state(&current, recommended))
}

#[tauri::command]
pub(crate) async fn open_onboarding_window(
    app: AppHandle,
    license: tauri::State<'_, Arc<LicenseService>>,
) -> Result<(), String> {
    let license_state = license.state().await;
    open_onboarding_session(&app, license_state.status)
}

#[tauri::command]
pub(crate) async fn complete_onboarding(
    app: AppHandle,
    license: tauri::State<'_, Arc<LicenseService>>,
    settings: tauri::State<'_, SharedSettings>,
) -> Result<OnboardingState, String> {
    let license_state = license.state().await;
    if !completion_allowed(license_state.status) {
        apply_customer_windows(&app, license_state.status, 0);
        return Err("Activate Hum before finishing setup.".to_string());
    }

    let saved = {
        let mut current = settings.write().await;
        current.onboarding_version = current.onboarding_version.max(CURRENT_ONBOARDING_VERSION);
        current.clone()
    };
    settings::save_to_store(&app, &saved);
    let state = onboarding_state(&saved, None);
    let _ = app.emit("settings-changed", &saved);
    apply_mode(&app, OverlayMode::Locked);
    apply_customer_windows(&app, license_state.status, saved.onboarding_version);
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_output::model::AudioOutputRoute;
    use crate::license::LicenseStatus;

    #[test]
    fn unlicensed_states_show_only_activation() {
        for status in [
            LicenseStatus::Unlicensed,
            LicenseStatus::VerificationRequired,
            LicenseStatus::Invalid,
            LicenseStatus::Revoked,
            LicenseStatus::DeviceLimit,
            LicenseStatus::ClockError,
            LicenseStatus::ServiceUnavailable,
        ] {
            assert_eq!(
                customer_window_plan(status, 0),
                CustomerWindowPlan {
                    activation: true,
                    setup: false,
                    overlay: false,
                },
                "{status:?}"
            );
            assert_eq!(
                customer_window_plan(status, 9),
                customer_window_plan(status, 0)
            );
        }
    }

    #[test]
    fn licensed_states_require_setup_until_the_current_version_is_complete() {
        for status in [
            LicenseStatus::Development,
            LicenseStatus::Verified,
            LicenseStatus::VerificationDue,
            LicenseStatus::OfflineGrace,
        ] {
            assert_eq!(
                customer_window_plan(status, 0),
                CustomerWindowPlan {
                    activation: false,
                    setup: true,
                    overlay: true,
                },
                "{status:?}"
            );
            assert_eq!(
                customer_window_plan(status, CURRENT_ONBOARDING_VERSION),
                CustomerWindowPlan {
                    activation: false,
                    setup: false,
                    overlay: true,
                },
                "{status:?}"
            );
        }
    }

    #[test]
    fn future_setup_versions_remain_complete() {
        assert!(!onboarding_completed(0));
        assert!(onboarding_completed(CURRENT_ONBOARDING_VERSION));
        assert!(onboarding_completed(
            CURRENT_ONBOARDING_VERSION.saturating_add(8)
        ));
    }

    #[test]
    fn only_licensed_states_can_complete_setup() {
        assert!(completion_allowed(LicenseStatus::Development));
        assert!(completion_allowed(LicenseStatus::Verified));
        assert!(completion_allowed(LicenseStatus::VerificationDue));
        assert!(completion_allowed(LicenseStatus::OfflineGrace));
        assert!(!completion_allowed(LicenseStatus::Unlicensed));
        assert!(!completion_allowed(LicenseStatus::Revoked));
    }

    #[test]
    fn detected_routes_map_to_customer_listening_profiles() {
        assert_eq!(
            listening_mode_for_route(AudioOutputRoute::Wired),
            Some("wired")
        );
        assert_eq!(
            listening_mode_for_route(AudioOutputRoute::Speakers),
            Some("speakers")
        );
        assert_eq!(
            listening_mode_for_route(AudioOutputRoute::Bluetooth),
            Some("bluetooth")
        );
        assert_eq!(
            listening_mode_for_route(AudioOutputRoute::Hdmi),
            Some("speakers")
        );
        assert_eq!(listening_mode_for_route(AudioOutputRoute::Unknown), None);
    }

    #[test]
    fn setup_state_preserves_its_wire_contract_and_recommendation() {
        let settings = Settings {
            onboarding_version: CURRENT_ONBOARDING_VERSION,
            ..Default::default()
        };

        assert_eq!(
            serde_json::to_value(onboarding_state(&settings, Some("bluetooth"))).unwrap(),
            serde_json::json!({
                "version": 1,
                "current_version": 1,
                "completed": true,
                "recommended_listening_mode": "bluetooth"
            })
        );
    }
}
