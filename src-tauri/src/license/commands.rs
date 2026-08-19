use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use super::{LicenseService, LicenseState, LicenseStatus};

const MAX_CUSTOMER_KEY_LENGTH: usize = 256;
const POLAR_HOSTS: &[&str] = &["buy.polar.sh", "polar.sh", "www.polar.sh"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LicenseWindowAction {
    Overlay,
    Activation,
}

pub(crate) fn license_window_action(status: LicenseStatus) -> LicenseWindowAction {
    if status.is_licensed() {
        LicenseWindowAction::Overlay
    } else {
        LicenseWindowAction::Activation
    }
}

pub(crate) fn apply_license_windows(app: &AppHandle, state: &LicenseState) {
    match license_window_action(state.status) {
        LicenseWindowAction::Overlay => {
            if let Some(window) = app.get_webview_window("activation") {
                let _ = window.hide();
            }
            if let Some(window) = app.get_webview_window("overlay") {
                let _ = window.show();
            }
        }
        LicenseWindowAction::Activation => {
            if let Some(window) = app.get_webview_window("overlay") {
                let _ = window.hide();
            }
            let _ = show_license_window(app);
        }
    }
}

pub(crate) fn current_unix_ms() -> i64 {
    let milliseconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(milliseconds).unwrap_or(i64::MAX)
}

#[tauri::command]
pub async fn get_license_state(
    service: tauri::State<'_, Arc<LicenseService>>,
) -> Result<LicenseState, String> {
    Ok(service.state().await)
}

#[tauri::command]
pub async fn activate_license(
    app: AppHandle,
    service: tauri::State<'_, Arc<LicenseService>>,
    license_key: String,
) -> Result<LicenseState, String> {
    let license_key = validate_customer_key(&license_key)?;
    let state = service
        .activate(license_key, current_unix_ms())
        .await
        .map_err(|error| error.to_string())?;
    publish_state(&app, &state);
    Ok(state)
}

#[tauri::command]
pub async fn refresh_license(
    app: AppHandle,
    service: tauri::State<'_, Arc<LicenseService>>,
) -> Result<LicenseState, String> {
    let state = service
        .bootstrap(current_unix_ms())
        .await
        .map_err(|error| error.to_string())?;
    publish_state(&app, &state);
    Ok(state)
}

#[tauri::command]
pub async fn deactivate_license(
    app: AppHandle,
    service: tauri::State<'_, Arc<LicenseService>>,
) -> Result<LicenseState, String> {
    let state = service
        .deactivate()
        .await
        .map_err(|error| error.to_string())?;
    if !deactivation_released(state.licensed) {
        return Err(
            "Hum could not release this PC. Check your connection and try again.".to_string(),
        );
    }
    publish_state(&app, &state);
    Ok(state)
}

#[tauri::command]
pub fn open_license_window(app: AppHandle) -> Result<(), String> {
    show_license_window(&app)
}

#[tauri::command]
pub fn open_license_checkout() -> Result<(), String> {
    open_configured_polar_url(option_env!("HUM_POLAR_CHECKOUT_URL"), "checkout")
}

#[tauri::command]
pub fn open_license_portal() -> Result<(), String> {
    open_configured_polar_url(
        option_env!("HUM_POLAR_CUSTOMER_PORTAL_URL"),
        "customer portal",
    )
}

fn publish_state(app: &AppHandle, state: &LicenseState) {
    apply_license_windows(app, state);
    let _ = app.emit("license-state-changed", state);
}

fn deactivation_released(is_licensed: bool) -> bool {
    !is_licensed
}

fn show_license_window(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("activation")
        .ok_or_else(|| "Hum's license window is not available.".to_string())?;
    window
        .show()
        .map_err(|_| "Hum could not show the license window.".to_string())?;
    let _ = window.unminimize();
    let _ = window.set_focus();
    Ok(())
}

fn validate_customer_key(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("Enter the license key from your Hum receipt.".to_string());
    }
    if trimmed.len() > MAX_CUSTOMER_KEY_LENGTH {
        return Err("That license key is too long.".to_string());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("That license key contains unsupported characters.".to_string());
    }
    Ok(trimmed.to_string())
}

fn configured_polar_url(value: Option<&str>, label: &str) -> Result<String, String> {
    let value = value
        .filter(|candidate| !candidate.trim().is_empty())
        .ok_or_else(|| format!("Hum {label} is not configured yet."))?;
    validate_polar_url(value)?;
    Ok(value.to_string())
}

fn validate_polar_url(value: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| "Hum could not open that Polar link.".to_string())?;
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let valid = parsed.scheme() == "https"
        && POLAR_HOSTS.contains(&host.as_str())
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.fragment().is_none();
    if !valid {
        return Err("Hum could not open that Polar link.".to_string());
    }
    Ok(())
}

fn open_configured_polar_url(value: Option<&str>, label: &str) -> Result<(), String> {
    let url = configured_polar_url(value, label)?;
    opener::open(url).map_err(|_| format!("Hum could not open the {label}."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license::LicenseStatus;

    #[test]
    fn only_licensed_statuses_choose_the_overlay() {
        for (status, expected) in [
            (LicenseStatus::Development, LicenseWindowAction::Overlay),
            (LicenseStatus::Verified, LicenseWindowAction::Overlay),
            (LicenseStatus::VerificationDue, LicenseWindowAction::Overlay),
            (LicenseStatus::OfflineGrace, LicenseWindowAction::Overlay),
            (LicenseStatus::Unlicensed, LicenseWindowAction::Activation),
            (
                LicenseStatus::VerificationRequired,
                LicenseWindowAction::Activation,
            ),
            (LicenseStatus::Invalid, LicenseWindowAction::Activation),
            (LicenseStatus::Revoked, LicenseWindowAction::Activation),
            (LicenseStatus::DeviceLimit, LicenseWindowAction::Activation),
            (LicenseStatus::ClockError, LicenseWindowAction::Activation),
            (
                LicenseStatus::ServiceUnavailable,
                LicenseWindowAction::Activation,
            ),
        ] {
            assert_eq!(license_window_action(status), expected);
        }
    }

    #[test]
    fn customer_keys_are_trimmed_and_invalid_values_are_redacted() {
        assert_eq!(
            validate_customer_key("  HUM-ABCD-1234  ").unwrap(),
            "HUM-ABCD-1234"
        );
        for value in ["", "   ", "HUM\nSECRET", "HUM\0SECRET", &"A".repeat(257)] {
            let error = validate_customer_key(value).unwrap_err();
            if !value.is_empty() {
                assert!(!error.contains(value));
            }
            assert!(!error.contains("SECRET"));
        }
    }

    #[test]
    fn external_license_links_accept_only_safe_polar_https_urls() {
        for url in [
            "https://buy.polar.sh/polar_cl_example",
            "https://polar.sh/purchases",
            "https://www.polar.sh/purchases",
        ] {
            assert!(validate_polar_url(url).is_ok(), "{url}");
        }
        for url in [
            "http://buy.polar.sh/polar_cl_example",
            "https://polar.sh.evil.example/purchases",
            "https://evil.example/polar.sh",
            "https://user:pass@polar.sh/purchases",
            "https://polar.sh/purchases#license-key",
            "file:///C:/license.txt",
            "",
        ] {
            let error = validate_polar_url(url).unwrap_err();
            if !url.is_empty() {
                assert!(!error.contains(url));
            }
        }
    }

    #[test]
    fn missing_link_configuration_returns_a_useful_safe_error() {
        assert_eq!(
            configured_polar_url(None, "checkout").unwrap_err(),
            "Hum checkout is not configured yet."
        );
        assert_eq!(
            configured_polar_url(Some(""), "customer portal").unwrap_err(),
            "Hum customer portal is not configured yet."
        );
    }

    #[test]
    fn deactivation_only_succeeds_after_the_entitlement_is_released() {
        assert!(deactivation_released(false));
        assert!(!deactivation_released(true));
    }
}
