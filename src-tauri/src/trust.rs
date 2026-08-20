use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::license::{current_unix_ms, LicenseService, LicenseState, LicenseStatus};
use crate::platform::info::PlatformInfo;
use crate::settings::{Settings, SharedSettings};

const SUPPORT_DESTINATION: &str = "mailto:info@syvr.dev?subject=Hum%20support";
const PRIVACY_DESTINATION: &str = "https://humlyrics.com/privacy";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TrustDestination {
    Support,
    Privacy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AboutInfo {
    pub(crate) product_name: String,
    pub(crate) version: String,
    pub(crate) operating_system: String,
    pub(crate) architecture: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct BuildInfo {
    pub(crate) developer_console: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct DiagnosticCapabilities {
    pub(crate) media_playback: bool,
    pub(crate) audio_output_discovery: bool,
    pub(crate) active_output_changes: bool,
    pub(crate) aspect_lock: bool,
    pub(crate) click_through: bool,
    pub(crate) update_banner_pointer_exception: bool,
    pub(crate) screen_sampling: bool,
    pub(crate) tray: bool,
    pub(crate) global_shortcuts: bool,
    pub(crate) autostart: bool,
    pub(crate) updater: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DiagnosticSettings {
    pub(crate) last_mode: String,
    pub(crate) layout_mode: String,
    pub(crate) overlay_shape: String,
    pub(crate) text_align: String,
    pub(crate) line_padding_px: i32,
    pub(crate) anticipate_ms: i32,
    pub(crate) listening_mode: String,
    pub(crate) wired_delay_ms: i32,
    pub(crate) speakers_delay_ms: i32,
    pub(crate) bluetooth_delay_ms: i32,
    pub(crate) jitter_tolerance_ms: i32,
    pub(crate) font_family: String,
    pub(crate) font_size_px: f32,
    pub(crate) font_weight: i32,
    pub(crate) text_color: String,
    pub(crate) text_color_dim: String,
    pub(crate) bg_color: String,
    pub(crate) bg_opacity: f32,
    pub(crate) tint_bg_from_album_art: bool,
    pub(crate) blur_album_art_background: bool,
    pub(crate) window_backdrop: String,
    pub(crate) auto_contrast: bool,
    pub(crate) show_album_art: bool,
    pub(crate) show_translation: bool,
    pub(crate) show_artist_info_panel: bool,
    pub(crate) launch_on_startup: bool,
    pub(crate) bg_hidden: bool,
    pub(crate) show_media: bool,
    pub(crate) ad_break_promos_enabled: bool,
    pub(crate) onboarding_version: u8,
    pub(crate) streamer_enabled: bool,
    pub(crate) streamer_port: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct DiagnosticLicense {
    pub(crate) status: LicenseStatus,
    pub(crate) licensed: bool,
    pub(crate) device_limit: u8,
    pub(crate) days_until_action: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct DiagnosticCache {
    pub(crate) kind: String,
    pub(crate) exists: bool,
    pub(crate) item_count: u64,
    pub(crate) byte_total: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct DiagnosticReport {
    pub(crate) schema_version: u16,
    pub(crate) generated_at_unix_ms: i64,
    pub(crate) application: AboutInfo,
    pub(crate) capabilities: DiagnosticCapabilities,
    pub(crate) settings: DiagnosticSettings,
    pub(crate) license: DiagnosticLicense,
    pub(crate) caches: Vec<DiagnosticCache>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct DiagnosticExport {
    pub(crate) path: PathBuf,
}

fn trust_destination_url(destination: TrustDestination) -> &'static str {
    match destination {
        TrustDestination::Support => SUPPORT_DESTINATION,
        TrustDestination::Privacy => PRIVACY_DESTINATION,
    }
}

fn build_about_info(
    product_name: &str,
    version: &str,
    operating_system: &str,
    architecture: &str,
) -> AboutInfo {
    AboutInfo {
        product_name: product_name.to_string(),
        version: version.to_string(),
        operating_system: operating_system.to_string(),
        architecture: architecture.to_string(),
    }
}

fn capabilities_from_platform(info: &PlatformInfo) -> DiagnosticCapabilities {
    DiagnosticCapabilities {
        media_playback: info.media.playback,
        audio_output_discovery: info.audio_output.discovery,
        active_output_changes: info.audio_output.active_output_changes,
        aspect_lock: info.window.aspect_lock,
        click_through: info.window.click_through,
        update_banner_pointer_exception: info.window.update_banner_pointer_exception,
        screen_sampling: info.window.screen_sampling,
        tray: info.services.tray,
        global_shortcuts: info.services.global_shortcuts,
        autostart: info.services.autostart,
        updater: info.services.updater,
    }
}

fn build_info_for_profile(developer_console: bool) -> BuildInfo {
    BuildInfo { developer_console }
}

fn backdrop_name(settings: &Settings) -> String {
    serde_json::to_value(settings.window_backdrop)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "none".to_string())
}

fn build_diagnostic_report(
    generated_at_unix_ms: i64,
    application: AboutInfo,
    capabilities: DiagnosticCapabilities,
    settings: &Settings,
    license: &LicenseState,
    caches: Vec<DiagnosticCache>,
) -> DiagnosticReport {
    DiagnosticReport {
        schema_version: 1,
        generated_at_unix_ms,
        application,
        capabilities,
        settings: DiagnosticSettings {
            last_mode: settings.last_mode.as_str().to_string(),
            layout_mode: settings.layout_mode.clone(),
            overlay_shape: settings.overlay_shape.clone(),
            text_align: settings.text_align.clone(),
            line_padding_px: settings.line_padding_px,
            anticipate_ms: settings.anticipate_ms,
            listening_mode: settings.listening_mode.clone(),
            wired_delay_ms: settings.wired_delay_ms,
            speakers_delay_ms: settings.speakers_delay_ms,
            bluetooth_delay_ms: settings.bluetooth_delay_ms,
            jitter_tolerance_ms: settings.jitter_tolerance_ms,
            font_family: settings.font_family.clone(),
            font_size_px: settings.font_size_px,
            font_weight: settings.font_weight,
            text_color: settings.text_color.clone(),
            text_color_dim: settings.text_color_dim.clone(),
            bg_color: settings.bg_color.clone(),
            bg_opacity: settings.bg_opacity,
            tint_bg_from_album_art: settings.tint_bg_from_album_art,
            blur_album_art_background: settings.blur_album_art_background,
            window_backdrop: backdrop_name(settings),
            auto_contrast: settings.auto_contrast,
            show_album_art: settings.show_album_art,
            show_translation: settings.show_translation,
            show_artist_info_panel: settings.show_artist_info_panel,
            launch_on_startup: settings.launch_on_startup,
            bg_hidden: settings.bg_hidden,
            show_media: settings.show_media,
            ad_break_promos_enabled: settings.ad_break_promos_enabled,
            onboarding_version: settings.onboarding_version,
            streamer_enabled: settings.streamer_enabled,
            streamer_port: settings.streamer_port,
        },
        license: DiagnosticLicense {
            status: license.status,
            licensed: license.licensed,
            device_limit: license.device_limit,
            days_until_action: license.days_until_action,
        },
        caches,
    }
}

fn diagnostic_filename(generated_at_unix_ms: i64, collision: u16) -> String {
    let generated_at_unix_ms = generated_at_unix_ms.max(0);
    if collision == 0 {
        format!("Hum-diagnostics-{generated_at_unix_ms}.json")
    } else {
        format!("Hum-diagnostics-{generated_at_unix_ms}-{collision}.json")
    }
}

fn write_diagnostic_create_new(
    directory: &Path,
    generated_at_unix_ms: i64,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    for collision in 0..=999 {
        let path = directory.join(diagnostic_filename(generated_at_unix_ms, collision));
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("Hum could not create the diagnostic file.".to_string()),
        };
        if file.write_all(bytes).is_err() || file.sync_all().is_err() {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err("Hum could not finish writing the diagnostic file.".to_string());
        }
        return Ok(path);
    }
    Err("Hum could not choose a new diagnostic filename.".to_string())
}

fn json_item_count(bytes: &[u8]) -> u64 {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return 0;
    };
    if let Some(items) = value.get("promos").and_then(serde_json::Value::as_array) {
        return u64::try_from(items.len()).unwrap_or(u64::MAX);
    }
    value
        .as_object()
        .map(|items| u64::try_from(items.len()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn summarize_file_cache(kind: &str, path: &Path) -> DiagnosticCache {
    let Ok(metadata) = fs::metadata(path) else {
        return DiagnosticCache {
            kind: kind.to_string(),
            exists: false,
            item_count: 0,
            byte_total: 0,
        };
    };
    let item_count = fs::read(path)
        .ok()
        .map_or(0, |bytes| json_item_count(&bytes));
    DiagnosticCache {
        kind: kind.to_string(),
        exists: true,
        item_count,
        byte_total: metadata.len(),
    }
}

fn summarize_directory_cache(kind: &str, path: &Path) -> DiagnosticCache {
    let Ok(entries) = fs::read_dir(path) else {
        return DiagnosticCache {
            kind: kind.to_string(),
            exists: false,
            item_count: 0,
            byte_total: 0,
        };
    };
    let mut item_count = 0_u64;
    let mut byte_total = 0_u64;
    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() {
            item_count = item_count.saturating_add(1);
            byte_total = byte_total.saturating_add(metadata.len());
        }
    }
    DiagnosticCache {
        kind: kind.to_string(),
        exists: true,
        item_count,
        byte_total,
    }
}

fn about_from_app(app: &AppHandle<Wry>) -> AboutInfo {
    let package = app.package_info();
    build_about_info(
        &package.name,
        &package.version.to_string(),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

#[tauri::command]
pub(crate) fn get_about_info(app: AppHandle<Wry>) -> AboutInfo {
    about_from_app(&app)
}

#[tauri::command]
pub(crate) fn get_build_info() -> BuildInfo {
    build_info_for_profile(cfg!(debug_assertions))
}

#[tauri::command]
pub(crate) fn open_trust_destination(destination: TrustDestination) -> Result<(), String> {
    let label = match destination {
        TrustDestination::Support => "support email",
        TrustDestination::Privacy => "privacy policy",
    };
    opener::open(trust_destination_url(destination))
        .map_err(|_| format!("Hum could not open the {label}."))
}

#[tauri::command]
pub(crate) fn request_update_check(app: AppHandle<Wry>) -> Result<(), String> {
    app.emit("updater-check-requested", ())
        .map_err(|_| "Hum could not start the update check.".to_string())
}

#[tauri::command]
pub(crate) async fn export_diagnostics(
    app: AppHandle<Wry>,
    settings: tauri::State<'_, SharedSettings>,
    license_service: tauri::State<'_, Arc<LicenseService>>,
) -> Result<DiagnosticExport, String> {
    let platform_info = crate::platform::info::get_platform_info(app.clone())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "Hum could not resolve its application data folder.".to_string())?;
    let app_config_dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| app_data_dir.clone());
    let downloads_dir = app
        .path()
        .download_dir()
        .map_err(|_| "Hum could not find the Downloads folder.".to_string())?;
    let generated_at_unix_ms = current_unix_ms();
    let settings = settings.read().await.clone();
    let license = license_service.state().await;
    let report = build_diagnostic_report(
        generated_at_unix_ms,
        about_from_app(&app),
        capabilities_from_platform(&platform_info),
        &settings,
        &license,
        vec![
            summarize_file_cache("lyrics", &app_data_dir.join("lyrics-cache.json")),
            summarize_directory_cache("artist", &app_data_dir.join("cache").join("artist")),
            summarize_file_cache("promotions", &app_config_dir.join("promos.json")),
        ],
    );
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|_| "Hum could not prepare the diagnostic snapshot.".to_string())?;
    let path = write_diagnostic_create_new(&downloads_dir, generated_at_unix_ms, &bytes)?;
    Ok(DiagnosticExport { path })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::OverlayMode;
    use crate::platform::info::{
        AudioOutputCapabilities, MediaCapabilities, Platform, PlatformPaths, ServiceCapabilities,
        WindowCapabilities,
    };
    use crate::window_effects::backdrop::BackdropKind;
    use std::collections::BTreeSet;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn capabilities() -> DiagnosticCapabilities {
        DiagnosticCapabilities {
            media_playback: true,
            audio_output_discovery: true,
            active_output_changes: true,
            aspect_lock: true,
            click_through: true,
            update_banner_pointer_exception: true,
            screen_sampling: true,
            tray: true,
            global_shortcuts: true,
            autostart: true,
            updater: true,
        }
    }

    fn license_with_private_markers() -> LicenseState {
        LicenseState {
            status: LicenseStatus::Verified,
            licensed: true,
            display_key: Some("HUM-PRIVATE-MARKER".to_string()),
            device_limit: 3,
            verified_at_unix_ms: Some(1_111_111_111),
            verify_after_unix_ms: Some(2_222_222_222),
            grace_ends_unix_ms: Some(3_333_333_333),
            days_until_action: Some(24),
            message: "PRIVATE-PROVIDER-MESSAGE".to_string(),
            recovery: "PRIVATE-PROVIDER-RECOVERY".to_string(),
        }
    }

    fn object_keys(value: &serde_json::Value) -> BTreeSet<&str> {
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn diagnostic_json_contains_exact_allowlisted_reproduction_contract() {
        let settings = Settings {
            last_mode: OverlayMode::Ghost,
            layout_mode: "full_page".to_string(),
            overlay_shape: "square".to_string(),
            text_align: "center".to_string(),
            line_padding_px: 18,
            anticipate_ms: 125,
            listening_mode: "bluetooth".to_string(),
            wired_delay_ms: 25,
            speakers_delay_ms: 250,
            bluetooth_delay_ms: 475,
            jitter_tolerance_ms: 1800,
            font_family: "Geist".to_string(),
            font_size_px: 42.0,
            font_weight: 700,
            text_color: "#ffffff".to_string(),
            text_color_dim: "#aaaaaa".to_string(),
            bg_color: "#101010".to_string(),
            bg_opacity: 82.0,
            tint_bg_from_album_art: true,
            blur_album_art_background: true,
            window_backdrop: BackdropKind::Mica,
            auto_contrast: true,
            show_album_art: true,
            show_translation: true,
            show_artist_info_panel: true,
            launch_on_startup: true,
            bg_hidden: false,
            show_media: true,
            streamer_enabled: true,
            streamer_port: 4747,
            ..Settings::default()
        };
        let report = build_diagnostic_report(
            9_999,
            build_about_info("Hum", "1.2.3", "windows", "x86_64"),
            capabilities(),
            &settings,
            &license_with_private_markers(),
            vec![DiagnosticCache {
                kind: "lyrics".to_string(),
                exists: true,
                item_count: 7,
                byte_total: 4096,
            }],
        );
        let json = serde_json::to_value(report).unwrap();

        assert_eq!(
            object_keys(&json),
            BTreeSet::from([
                "application",
                "caches",
                "capabilities",
                "generated_at_unix_ms",
                "license",
                "schema_version",
                "settings",
            ])
        );
        assert_eq!(
            object_keys(&json["application"]),
            BTreeSet::from([
                "architecture",
                "operating_system",
                "product_name",
                "version",
            ])
        );
        assert_eq!(
            object_keys(&json["capabilities"]),
            BTreeSet::from([
                "active_output_changes",
                "aspect_lock",
                "audio_output_discovery",
                "autostart",
                "click_through",
                "global_shortcuts",
                "media_playback",
                "screen_sampling",
                "tray",
                "update_banner_pointer_exception",
                "updater",
            ])
        );
        assert_eq!(
            object_keys(&json["settings"]),
            BTreeSet::from([
                "ad_break_promos_enabled",
                "anticipate_ms",
                "auto_contrast",
                "bg_color",
                "bg_hidden",
                "bg_opacity",
                "bluetooth_delay_ms",
                "blur_album_art_background",
                "font_family",
                "font_size_px",
                "font_weight",
                "jitter_tolerance_ms",
                "last_mode",
                "launch_on_startup",
                "layout_mode",
                "line_padding_px",
                "listening_mode",
                "onboarding_version",
                "overlay_shape",
                "show_album_art",
                "show_artist_info_panel",
                "show_media",
                "show_translation",
                "speakers_delay_ms",
                "streamer_enabled",
                "streamer_port",
                "text_align",
                "text_color",
                "text_color_dim",
                "tint_bg_from_album_art",
                "window_backdrop",
                "wired_delay_ms",
            ])
        );
        assert_eq!(
            object_keys(&json["license"]),
            BTreeSet::from(["days_until_action", "device_limit", "licensed", "status",])
        );
        assert_eq!(
            object_keys(&json["caches"][0]),
            BTreeSet::from(["byte_total", "exists", "item_count", "kind"])
        );
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["application"]["product_name"], "Hum");
        assert_eq!(json["application"]["version"], "1.2.3");
        assert_eq!(json["capabilities"]["updater"], true);
        assert_eq!(json["settings"]["overlay_shape"], "square");
        assert_eq!(json["settings"]["bluetooth_delay_ms"], 475);
        assert_eq!(json["settings"]["streamer_port"], 4747);
        assert_eq!(json["license"]["status"], "verified");
        assert_eq!(json["license"]["days_until_action"], 24);
        assert_eq!(json["caches"][0]["item_count"], 7);
    }

    #[test]
    fn diagnostic_json_excludes_protected_personal_path_media_and_cache_fields() {
        let report = build_diagnostic_report(
            9_999,
            build_about_info("Hum", "1.2.3", "windows", "x86_64"),
            capabilities(),
            &Settings::default(),
            &license_with_private_markers(),
            vec![DiagnosticCache {
                kind: "artist".to_string(),
                exists: true,
                item_count: 1,
                byte_total: 512,
            }],
        );
        let json = serde_json::to_string(&report).unwrap();

        for forbidden in [
            "HUM-PRIVATE-MARKER",
            "PRIVATE-PROVIDER-MESSAGE",
            "PRIVATE-PROVIDER-RECOVERY",
            "display_key",
            "verified_at_unix_ms",
            "verify_after_unix_ms",
            "grace_ends_unix_ms",
            "activation_id",
            "license_key",
            "app_data_dir",
            "settings_file",
            "current_track",
            "artist_name",
            "lyrics_text",
            "cache_filename",
        ] {
            assert!(!json.contains(forbidden), "leaked {forbidden}: {json}");
        }
    }

    #[test]
    fn platform_mapper_copies_every_boolean_without_paths() {
        let private_marker = "PRIVATE-PLATFORM-PATH-MARKER";
        let info = PlatformInfo {
            platform: Platform::Windows,
            media: MediaCapabilities { playback: false },
            audio_output: AudioOutputCapabilities {
                discovery: true,
                active_output_changes: false,
            },
            window: WindowCapabilities {
                supported_backdrops: vec![BackdropKind::Acrylic],
                aspect_lock: true,
                click_through: false,
                update_banner_pointer_exception: true,
                screen_sampling: false,
            },
            services: ServiceCapabilities {
                tray: true,
                global_shortcuts: false,
                autostart: true,
                updater: false,
            },
            paths: PlatformPaths {
                app_data_dir: PathBuf::from(format!("C:/{private_marker}/data")),
                settings_file: PathBuf::from(format!("C:/{private_marker}/settings.json")),
            },
        };

        let json = serde_json::to_value(capabilities_from_platform(&info)).unwrap();

        assert_eq!(json["media_playback"], false);
        assert_eq!(json["audio_output_discovery"], true);
        assert_eq!(json["active_output_changes"], false);
        assert_eq!(json["aspect_lock"], true);
        assert_eq!(json["click_through"], false);
        assert_eq!(json["update_banner_pointer_exception"], true);
        assert_eq!(json["screen_sampling"], false);
        assert_eq!(json["tray"], true);
        assert_eq!(json["global_shortcuts"], false);
        assert_eq!(json["autostart"], true);
        assert_eq!(json["updater"], false);
        assert!(!serde_json::to_string(&json)
            .unwrap()
            .contains(private_marker));
    }

    #[test]
    fn build_info_uses_the_exact_rust_profile_flag() {
        assert_eq!(
            build_info_for_profile(false),
            BuildInfo {
                developer_console: false,
            }
        );
        assert_eq!(
            build_info_for_profile(true),
            BuildInfo {
                developer_console: true,
            }
        );
        assert_eq!(get_build_info().developer_console, cfg!(debug_assertions));
    }

    #[test]
    fn trust_destinations_are_fixed_without_caller_provided_urls() {
        assert_eq!(
            trust_destination_url(TrustDestination::Support),
            "mailto:info@syvr.dev?subject=Hum%20support"
        );
        assert_eq!(
            trust_destination_url(TrustDestination::Privacy),
            "https://humlyrics.com/privacy"
        );
        assert!(serde_json::from_str::<TrustDestination>(r#""support""#).is_ok());
        assert!(serde_json::from_str::<TrustDestination>(r#""privacy""#).is_ok());
        assert!(serde_json::from_str::<TrustDestination>(r#""https://evil.example""#).is_err());
    }

    #[test]
    fn about_metadata_uses_running_package_name_and_version() {
        assert_eq!(
            build_about_info("Hum Preview", "9.8.7", "windows", "aarch64"),
            AboutInfo {
                product_name: "Hum Preview".to_string(),
                version: "9.8.7".to_string(),
                operating_system: "windows".to_string(),
                architecture: "aarch64".to_string(),
            }
        );
    }

    #[test]
    fn diagnostic_filename_is_safe_and_create_new_never_overwrites() {
        assert_eq!(
            diagnostic_filename(1_725_000_123_456, 0),
            "Hum-diagnostics-1725000123456.json"
        );
        assert_eq!(
            diagnostic_filename(1_725_000_123_456, 2),
            "Hum-diagnostics-1725000123456-2.json"
        );

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("hum-trust-test-{unique}"));
        fs::create_dir(&directory).unwrap();
        let first = write_diagnostic_create_new(&directory, 42, b"first").unwrap();
        let second = write_diagnostic_create_new(&directory, 42, b"second").unwrap();

        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), b"first");
        assert_eq!(fs::read(&second).unwrap(), b"second");
        assert!(first.starts_with(&directory));
        assert!(second.starts_with(&directory));

        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn cache_summaries_never_expose_paths_filenames_or_payloads() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("hum-PATH-PRIVATE-MARKER-{unique}"));
        let artist_directory = directory.join("artist");
        fs::create_dir_all(&artist_directory).unwrap();
        let cache_file = directory.join("CACHE-FILENAME-MARKER.json");
        fs::write(&cache_file, br#"{"CACHE-PAYLOAD-MARKER":"private"}"#).unwrap();
        fs::write(
            artist_directory.join("ARTIST-FILENAME-MARKER.json"),
            br#"{"ARTIST-PAYLOAD-MARKER":"private"}"#,
        )
        .unwrap();

        let summaries = vec![
            summarize_file_cache("lyrics", &cache_file),
            summarize_directory_cache("artist", &artist_directory),
        ];
        let json = serde_json::to_string(&summaries).unwrap();

        assert!(json.contains("lyrics"));
        assert!(json.contains("artist"));
        for forbidden in [
            "PATH-PRIVATE-MARKER",
            "CACHE-FILENAME-MARKER",
            "CACHE-PAYLOAD-MARKER",
            "ARTIST-FILENAME-MARKER",
            "ARTIST-PAYLOAD-MARKER",
        ] {
            assert!(!json.contains(forbidden), "leaked {forbidden}: {json}");
        }

        fs::remove_file(cache_file).unwrap();
        fs::remove_file(artist_directory.join("ARTIST-FILENAME-MARKER.json")).unwrap();
        fs::remove_dir(artist_directory).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn diagnostic_export_surfaces_directory_failures_without_leaking_the_path() {
        let private_marker = "PRIVATE-DOWNLOAD-PATH-MARKER";
        let missing = std::env::temp_dir().join(private_marker).join("missing");
        let error = write_diagnostic_create_new(&missing, 42, b"snapshot").unwrap_err();

        assert_eq!(error, "Hum could not create the diagnostic file.");
        assert!(!error.contains(private_marker));
    }
}
