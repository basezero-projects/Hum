use std::fmt::Display;
use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, Wry};

use crate::settings::SETTINGS_STORE_FILE;
use crate::window_effects::backdrop::BackdropKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum Platform {
    Windows,
    Macos,
    Linux,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MediaCapabilities {
    pub playback: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AudioOutputCapabilities {
    pub discovery: bool,
    pub active_output_changes: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WindowCapabilities {
    pub supported_backdrops: Vec<BackdropKind>,
    pub aspect_lock: bool,
    pub click_through: bool,
    pub update_banner_pointer_exception: bool,
    pub screen_sampling: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ServiceCapabilities {
    pub tray: bool,
    pub global_shortcuts: bool,
    pub autostart: bool,
    pub updater: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlatformPaths {
    pub app_data_dir: PathBuf,
    pub settings_file: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PlatformInfo {
    pub platform: Platform,
    pub media: MediaCapabilities,
    pub audio_output: AudioOutputCapabilities,
    pub window: WindowCapabilities,
    pub services: ServiceCapabilities,
    pub paths: PlatformPaths,
}

fn resolve_paths<E: Display>(app_data_dir: Result<PathBuf, E>) -> Result<PlatformPaths, String> {
    let app_data_dir = app_data_dir
        .map_err(|error| format!("failed to resolve the application data directory: {error}"))?;
    let settings_file = app_data_dir.join(SETTINGS_STORE_FILE);
    Ok(PlatformPaths {
        app_data_dir,
        settings_file,
    })
}

fn build_platform_info(platform: Platform, wayland: bool, paths: PlatformPaths) -> PlatformInfo {
    let (media, window, services) = match platform {
        Platform::Windows => (
            MediaCapabilities { playback: true },
            WindowCapabilities {
                supported_backdrops: vec![
                    BackdropKind::Acrylic,
                    BackdropKind::Mica,
                    BackdropKind::TabbedMica,
                    BackdropKind::None,
                ],
                aspect_lock: true,
                click_through: true,
                update_banner_pointer_exception: true,
                screen_sampling: true,
            },
            ServiceCapabilities {
                tray: true,
                global_shortcuts: true,
                autostart: true,
                updater: true,
            },
        ),
        Platform::Macos => (
            MediaCapabilities { playback: false },
            WindowCapabilities {
                supported_backdrops: vec![],
                aspect_lock: false,
                click_through: true,
                update_banner_pointer_exception: false,
                screen_sampling: false,
            },
            ServiceCapabilities {
                tray: true,
                global_shortcuts: true,
                autostart: true,
                updater: false,
            },
        ),
        Platform::Linux => (
            MediaCapabilities { playback: false },
            WindowCapabilities {
                supported_backdrops: vec![],
                aspect_lock: false,
                click_through: false,
                update_banner_pointer_exception: false,
                screen_sampling: false,
            },
            ServiceCapabilities {
                tray: true,
                global_shortcuts: !wayland,
                autostart: true,
                updater: false,
            },
        ),
    };

    PlatformInfo {
        platform,
        media,
        audio_output: AudioOutputCapabilities {
            discovery: false,
            active_output_changes: false,
        },
        window,
        services,
        paths,
    }
}

#[cfg(windows)]
fn current_platform() -> Platform {
    Platform::Windows
}

#[cfg(target_os = "macos")]
fn current_platform() -> Platform {
    Platform::Macos
}

#[cfg(target_os = "linux")]
fn current_platform() -> Platform {
    Platform::Linux
}

#[cfg(target_os = "linux")]
fn is_wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .is_ok_and(|session| session.eq_ignore_ascii_case("wayland"))
}

#[cfg(not(target_os = "linux"))]
fn is_wayland_session() -> bool {
    false
}

#[tauri::command]
pub fn get_platform_info(app: AppHandle<Wry>) -> Result<PlatformInfo, String> {
    let paths = resolve_paths(app.path().app_data_dir())?;
    Ok(build_platform_info(
        current_platform(),
        is_wayland_session(),
        paths,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::path::PathBuf;

    fn test_paths(base: &str) -> PlatformPaths {
        resolve_paths(Ok::<_, io::Error>(PathBuf::from(base))).unwrap()
    }

    #[test]
    fn windows_payload_is_exact() {
        let info = build_platform_info(Platform::Windows, false, test_paths("C:/Hum"));
        let expected_settings_file = PathBuf::from("C:/Hum").join(SETTINGS_STORE_FILE);

        assert_eq!(
            info,
            PlatformInfo {
                platform: Platform::Windows,
                media: MediaCapabilities { playback: true },
                audio_output: AudioOutputCapabilities {
                    discovery: false,
                    active_output_changes: false,
                },
                window: WindowCapabilities {
                    supported_backdrops: vec![
                        BackdropKind::Acrylic,
                        BackdropKind::Mica,
                        BackdropKind::TabbedMica,
                        BackdropKind::None,
                    ],
                    aspect_lock: true,
                    click_through: true,
                    update_banner_pointer_exception: true,
                    screen_sampling: true,
                },
                services: ServiceCapabilities {
                    tray: true,
                    global_shortcuts: true,
                    autostart: true,
                    updater: true,
                },
                paths: PlatformPaths {
                    app_data_dir: PathBuf::from("C:/Hum"),
                    settings_file: expected_settings_file.clone(),
                },
            }
        );
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({
                "platform": "windows",
                "media": { "playback": true },
                "audio_output": {
                    "discovery": false,
                    "active_output_changes": false
                },
                "window": {
                    "supported_backdrops": ["acrylic", "mica", "tabbed_mica", "none"],
                    "aspect_lock": true,
                    "click_through": true,
                    "update_banner_pointer_exception": true,
                    "screen_sampling": true
                },
                "services": {
                    "tray": true,
                    "global_shortcuts": true,
                    "autostart": true,
                    "updater": true
                },
                "paths": {
                    "app_data_dir": "C:/Hum",
                    "settings_file": expected_settings_file.to_string_lossy().into_owned()
                }
            })
        );
    }

    #[test]
    fn macos_payload_is_exact_without_media_or_updater() {
        let info = build_platform_info(Platform::Macos, false, test_paths("/Hum"));

        assert_eq!(
            info,
            PlatformInfo {
                platform: Platform::Macos,
                media: MediaCapabilities { playback: false },
                audio_output: AudioOutputCapabilities {
                    discovery: false,
                    active_output_changes: false,
                },
                window: WindowCapabilities {
                    supported_backdrops: vec![],
                    aspect_lock: false,
                    click_through: true,
                    update_banner_pointer_exception: false,
                    screen_sampling: false,
                },
                services: ServiceCapabilities {
                    tray: true,
                    global_shortcuts: true,
                    autostart: true,
                    updater: false,
                },
                paths: test_paths("/Hum"),
            }
        );
    }

    #[test]
    fn linux_x11_and_wayland_differ_only_in_shortcut_support() {
        let x11 = build_platform_info(Platform::Linux, false, test_paths("/Hum"));
        let wayland = build_platform_info(Platform::Linux, true, test_paths("/Hum"));

        assert_eq!(
            x11,
            PlatformInfo {
                platform: Platform::Linux,
                media: MediaCapabilities { playback: false },
                audio_output: AudioOutputCapabilities {
                    discovery: false,
                    active_output_changes: false,
                },
                window: WindowCapabilities {
                    supported_backdrops: vec![],
                    aspect_lock: false,
                    click_through: false,
                    update_banner_pointer_exception: false,
                    screen_sampling: false,
                },
                services: ServiceCapabilities {
                    tray: true,
                    global_shortcuts: true,
                    autostart: true,
                    updater: false,
                },
                paths: test_paths("/Hum"),
            }
        );
        let mut expected_wayland = x11;
        expected_wayland.services.global_shortcuts = false;
        assert_eq!(wayland, expected_wayland);
    }

    #[test]
    fn linux_does_not_claim_click_through_support() {
        let info = build_platform_info(Platform::Linux, false, test_paths("/Hum"));

        assert!(!info.window.click_through);
    }

    #[test]
    fn settings_path_uses_the_canonical_store_filename() {
        let base = PathBuf::from("/var/lib/hum");
        let paths = resolve_paths(Ok::<_, io::Error>(base.clone())).unwrap();

        assert_eq!(paths.app_data_dir, base);
        assert_eq!(
            paths.settings_file,
            PathBuf::from("/var/lib/hum").join(crate::settings::SETTINGS_STORE_FILE)
        );
    }

    #[test]
    fn path_resolution_failure_is_returned() {
        let error = resolve_paths(Err::<PathBuf, _>(io::Error::new(
            io::ErrorKind::NotFound,
            "app data unavailable",
        )))
        .unwrap_err();

        assert!(error.contains("app data unavailable"));
    }
}
