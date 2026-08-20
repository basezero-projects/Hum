use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::onboarding::apply_customer_windows;
use crate::settings::SharedSettings;

use super::{LicenseService, LicenseState};

const MAX_CUSTOMER_KEY_LENGTH: usize = 256;

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
    settings: tauri::State<'_, SharedSettings>,
    license_key: String,
) -> Result<LicenseState, String> {
    let license_key = validate_customer_key(&license_key)?;
    let state = service
        .activate(license_key, current_unix_ms())
        .await
        .map_err(|error| error.to_string())?;
    publish_state(&app, &state, &settings).await;
    Ok(state)
}

#[tauri::command]
pub async fn refresh_license(
    app: AppHandle,
    service: tauri::State<'_, Arc<LicenseService>>,
    settings: tauri::State<'_, SharedSettings>,
) -> Result<LicenseState, String> {
    let state = service
        .bootstrap(current_unix_ms())
        .await
        .map_err(|error| error.to_string())?;
    publish_state(&app, &state, &settings).await;
    Ok(state)
}

#[tauri::command]
pub async fn deactivate_license(
    app: AppHandle,
    service: tauri::State<'_, Arc<LicenseService>>,
    settings: tauri::State<'_, SharedSettings>,
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
    publish_state(&app, &state, &settings).await;
    Ok(state)
}

#[tauri::command]
pub fn open_license_window(app: AppHandle) -> Result<(), String> {
    show_license_window(&app)
}

#[tauri::command]
pub fn open_license_checkout() -> Result<(), String> {
    open_configured_polar_url(
        option_env!("HUM_POLAR_CHECKOUT_URL"),
        "checkout",
        validate_polar_checkout_url,
    )
}

#[tauri::command]
pub fn open_license_portal() -> Result<(), String> {
    open_configured_polar_url(
        option_env!("HUM_POLAR_CUSTOMER_PORTAL_URL"),
        "customer portal",
        validate_polar_portal_url,
    )
}

async fn publish_state(app: &AppHandle, state: &LicenseState, settings: &SharedSettings) {
    let onboarding_version = settings.read().await.onboarding_version;
    apply_customer_windows(app, state.status, onboarding_version);
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

fn configured_polar_url(
    value: Option<&str>,
    label: &str,
    validate: fn(&str) -> Result<(), String>,
) -> Result<String, String> {
    let value = value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .ok_or_else(|| format!("Hum {label} is not configured yet."))?;
    validate(value)?;
    Ok(value.to_string())
}

fn validate_polar_checkout_url(value: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| "Hum could not open that Polar link.".to_string())?;
    let common_valid = parsed.scheme() == "https"
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
        && parsed.as_str() == value;
    let path = parsed.path();
    let valid = common_valid
        && match parsed.host_str() {
            Some("buy.polar.sh") => path
                .strip_prefix('/')
                .is_some_and(is_polar_checkout_link_id),
            Some("sandbox-api.polar.sh") => path
                .strip_prefix("/v1/checkout-links/")
                .and_then(|rest| rest.strip_suffix("/redirect"))
                .is_some_and(is_polar_checkout_link_id),
            _ => false,
        };
    if !valid {
        return Err("Hum could not open that Polar link.".to_string());
    }
    Ok(())
}

fn validate_polar_portal_url(value: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| "Hum could not open that Polar link.".to_string())?;
    let mut segments = parsed
        .path()
        .split('/')
        .filter(|segment| !segment.is_empty());
    let organization = segments.next().unwrap_or_default();
    let portal = segments.next().unwrap_or_default();
    let valid = parsed.scheme() == "https"
        && parsed.host_str() == Some("polar.sh")
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
        && parsed.as_str() == value
        && is_polar_organization_slug(organization)
        && portal == "portal"
        && parsed.path() == format!("/{organization}/portal")
        && segments.next().is_none();
    if !valid {
        return Err("Hum could not open that Polar link.".to_string());
    }
    Ok(())
}

fn is_polar_checkout_link_id(value: &str) -> bool {
    value.strip_prefix("polar_cl_").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
    })
}

fn is_polar_organization_slug(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        && value
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
}

fn open_configured_polar_url(
    value: Option<&str>,
    label: &str,
    validate: fn(&str) -> Result<(), String>,
) -> Result<(), String> {
    let url = configured_polar_url(value, label, validate)?;
    opener::open(url).map_err(|_| format!("Hum could not open the {label}."))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn checkout_links_accept_only_persistent_production_and_sandbox_redirect_urls() {
        for url in [
            "https://buy.polar.sh/polar_cl_01abcXYZ",
            "https://sandbox-api.polar.sh/v1/checkout-links/polar_cl_01abcXYZ/redirect",
        ] {
            assert!(validate_polar_checkout_url(url).is_ok(), "{url}");
        }
        for url in [
            "https://buy.polar.sh/polar_c_temporary_session",
            "https://buy.polar.sh/polar_cl_01abcXYZ/extra",
            "https://buy.polar.sh/polar_cl_01abcXYZ?theme=dark",
            "https://buy.polar.sh:443/polar_cl_01abcXYZ",
            "https://buy.polar.sh:8443/polar_cl_01abcXYZ",
            "https://user:pass@buy.polar.sh/polar_cl_01abcXYZ",
            "https://buy.polar.sh/polar_cl_01abcXYZ#payment",
            "https://polar.sh/checkout/polar_cl_01abcXYZ",
            "https://www.polar.sh/checkout/polar_cl_01abcXYZ",
            "https://evil.buy.polar.sh/polar_cl_01abcXYZ",
            "https://buy.polar.sh.evil.example/polar_cl_01abcXYZ",
            "https://sandbox-api.polar.sh/v1/checkout-links/polar_cl_01abcXYZ",
            "https://sandbox-api.polar.sh/v1/checkout-links/polar_cl_01abcXYZ/redirect/extra",
            "https://sandbox-api.polar.sh:443/v1/checkout-links/polar_cl_01abcXYZ/redirect",
            "http://buy.polar.sh/polar_cl_01abcXYZ",
            "file:///C:/license.txt",
            "",
        ] {
            let error = validate_polar_checkout_url(url).unwrap_err();
            if !url.is_empty() {
                assert!(!error.contains(url));
            }
        }
    }

    #[test]
    fn portal_links_accept_only_the_organization_portal_path() {
        assert!(validate_polar_portal_url("https://polar.sh/syvr-studios/portal").is_ok());

        for url in [
            "https://polar.sh/portal",
            "https://polar.sh/syvr-studios/portal/",
            "https://polar.sh/syvr-studios/portal/orders",
            "https://polar.sh/syvr-studios/portal?customer=example",
            "https://polar.sh:443/syvr-studios/portal",
            "https://polar.sh:8443/syvr-studios/portal",
            "https://user:pass@polar.sh/syvr-studios/portal",
            "https://www.polar.sh/syvr-studios/portal",
            "https://evil.polar.sh/syvr-studios/portal",
            "https://polar.sh.evil.example/syvr-studios/portal",
            "http://polar.sh/syvr-studios/portal",
        ] {
            let error = validate_polar_portal_url(url).unwrap_err();
            assert!(!error.contains(url));
        }
    }

    #[test]
    fn missing_link_configuration_returns_a_useful_safe_error() {
        assert_eq!(
            configured_polar_url(None, "checkout", validate_polar_checkout_url).unwrap_err(),
            "Hum checkout is not configured yet."
        );
        assert_eq!(
            configured_polar_url(Some(""), "customer portal", validate_polar_portal_url)
                .unwrap_err(),
            "Hum customer portal is not configured yet."
        );
    }

    #[test]
    fn deactivation_only_succeeds_after_the_entitlement_is_released() {
        assert!(deactivation_released(false));
        assert!(!deactivation_released(true));
    }
}
