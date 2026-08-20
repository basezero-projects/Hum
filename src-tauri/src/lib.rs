use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use tauri::image::Image;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

#[cfg(windows)]
mod itunes;

mod media;

#[cfg(windows)]
mod smtc;

#[cfg(windows)]
mod web_bridge;

#[cfg(windows)]
mod pandora_desktop;

#[cfg(windows)]
mod youtube_bridge;

mod artist_info;
mod artist_window;
mod audio_output;
#[cfg(windows)]
mod contrast;
pub mod license;
mod lyrics;
mod mode;
mod onboarding;
mod platform;
mod promos;
mod settings;
mod streamer;
mod trust;
mod update_status;
pub mod window_effects;

use artist_info::{clear_artist_info_cache, get_artist_info, ArtistInfoCache};
use artist_window::{close_artist_panel_cmd, open_artist_panel_cmd, open_ticket_url};
use audio_output::{get_active_audio_output, get_audio_outputs, new_shared_state};
#[cfg(windows)]
use audio_output::{
    shutdown_managed_runtime, AudioOutputBackend, AudioOutputBackendContext, AudioOutputPublisher,
    ManagedAudioOutputRuntime, SharedAudioOutputState,
};
use license::{current_unix_ms, LicenseService};
use lyrics::{CurrentLyrics, SharedLyrics};
use media::{AlbumArtPayload, CurrentTrack, SharedAlbumArt, SharedSnapshot};
#[cfg(windows)]
use media::{MediaBackend, MediaBackendContext};
use mode::{
    apply_mode, cycle_overlay_mode, get_overlay_mode, icon_for, set_overlay_mode, ModeMenuItems,
    OverlayMode, SharedMode, TRAY_ID,
};
use onboarding::{
    apply_customer_windows, complete_onboarding, get_onboarding_state, open_onboarding_session,
};
use platform::info::get_platform_info;
use settings::{
    get_settings, open_settings_window, reset_settings, update_settings, SharedSettings,
};
use trust::{
    export_diagnostics, get_about_info, get_build_info, open_trust_destination,
    request_update_check,
};
use window_effects::{SystemWindowEffects, WindowEffects};

async fn read_current_track(state: &SharedSnapshot) -> CurrentTrack {
    state.read().await.clone()
}

#[cfg(windows)]
#[tauri::command]
async fn get_current_track(
    state: tauri::State<'_, SharedSnapshot>,
    bridge: tauri::State<'_, crate::web_bridge::SharedWebBridge>,
) -> Result<CurrentTrack, String> {
    let mut snap = read_current_track(state.inner()).await;
    crate::web_bridge::blend_bridge_into_snapshot(&mut snap, &bridge).await;
    Ok(snap)
}

#[cfg(not(windows))]
#[tauri::command]
async fn get_current_track(
    state: tauri::State<'_, SharedSnapshot>,
) -> Result<CurrentTrack, String> {
    Ok(read_current_track(state.inner()).await)
}

#[tauri::command]
async fn get_current_lyrics(
    state: tauri::State<'_, SharedLyrics>,
) -> Result<CurrentLyrics, String> {
    let s = state.read().await;
    Ok(s.clone())
}

/// Frontend invokes this once on mount, after the `album-art-loaded`
/// listener has been registered. Closes the startup race: the backend
/// may have emitted `album-art-loaded` before the listener was attached
/// (Tauri events are fire-and-forget; no replay for late subscribers).
/// Returns `None` if no art has been fetched yet (no active session, or
/// the current source doesn't expose a thumbnail).
#[tauri::command]
async fn get_current_album_art(
    state: tauri::State<'_, SharedAlbumArt>,
) -> Result<Option<AlbumArtPayload>, String> {
    let a = state.read().await;
    Ok(a.clone())
}

#[tauri::command]
fn set_update_status(
    app: tauri::AppHandle,
    status: update_status::UpdateStatus,
) -> Result<(), String> {
    let item = match app.try_state::<UpdateMenuItem>() {
        Some(s) => s.0.clone(),
        None => return Err("update menu item not registered".into()),
    };
    let projection = update_status::tray_projection(&status);
    item.set_text(projection.text).map_err(|e| e.to_string())?;
    item.set_enabled(projection.enabled)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Managed state handle for the dynamic update tray item.
struct UpdateMenuItem(MenuItem<tauri::Wry>);

/// Frontend tells us when the update banner is visible / hidden so the
/// ghost-mode cursor-poll worker knows whether to poke a clickable
/// hole in the click-through region.
#[tauri::command]
fn set_update_banner_visible(app: tauri::AppHandle, visible: bool) -> Result<(), String> {
    if let Some(s) = app.try_state::<Arc<AtomicBool>>() {
        s.store(visible, Ordering::Release);
    } else {
        return Err("update banner state not registered".into());
    }
    Ok(())
}

struct SharedShellState {
    snapshot: SharedSnapshot,
    album_art: SharedAlbumArt,
    lyrics_state: SharedLyrics,
    smtc_active: Arc<AtomicBool>,
    mode_state: SharedMode,
    audio_output_state: audio_output::model::SharedAudioOutputState,
}

impl SharedShellState {
    fn new() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(CurrentTrack::default())),
            album_art: Arc::new(RwLock::new(None)),
            lyrics_state: Arc::new(RwLock::new(CurrentLyrics::default())),
            smtc_active: Arc::new(AtomicBool::new(false)),
            mode_state: Arc::new(AtomicU8::new(OverlayMode::default() as u8)),
            audio_output_state: new_shared_state(),
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let SharedShellState {
        snapshot,
        album_art,
        lyrics_state,
        smtc_active,
        mode_state,
        audio_output_state,
    } = SharedShellState::new();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        // Launch-on-PC-startup. Plugin owns the Windows registry Run key /
        // macOS LaunchAgent / Linux .desktop entry. The setting itself is
        // toggled via the Settings UI (settings.rs::update_settings syncs
        // the plugin state on every save).
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // Save / restore position + size for the OVERLAY window only.
        // Dev console and settings windows are not tracked, so they always
        // open at the position tauri.conf.json declares (centered).
        // VISIBLE flag is excluded so saved state can never re-show a
        // window that conf says should start hidden.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .with_filter(|label| label == "overlay")
                .build(),
        )
        .plugin(build_global_shortcut_plugin())
        .manage(snapshot)
        .manage(album_art)
        .manage(lyrics_state)
        .manage(mode_state)
        .manage(audio_output_state)
        .setup(move |app| {
            let snap = app.state::<SharedSnapshot>().inner().clone();
            let art_state = app.state::<SharedAlbumArt>().inner().clone();
            let lyrics_shared = app.state::<SharedLyrics>().inner().clone();

            // Load persisted settings (if any) from the store BEFORE building
            // the tray, so the initial mode + tooltip + check items reflect
            // the user's last choice rather than always Edit.
            let loaded_settings = settings::load_from_store(app.handle());
            let initial_mode = loaded_settings.last_mode;
            let initial_listening_mode = loaded_settings.listening_mode.clone();
            let initial_onboarding_version = loaded_settings.onboarding_version;
            // Capture streamer fields before move so we can apply after manage.
            let streamer_enabled_at_start = loaded_settings.streamer_enabled;
            let streamer_port_at_start = loaded_settings.streamer_port;
            // Reconcile OS autostart with saved setting on every launch. This picks
            // up any external drift (e.g. user removed Hum from Windows Startup
            // Apps via OS settings while the file still says launch_on_startup = true).
            settings::sync_autostart(app.handle(), loaded_settings.launch_on_startup);
            app.manage::<SharedSettings>(Arc::new(RwLock::new(loaded_settings)));

            #[cfg(any(debug_assertions, not(windows)))]
            let license_service = Arc::new(LicenseService::development());
            #[cfg(all(not(debug_assertions), windows))]
            let license_service = {
                let path = app
                    .path()
                    .app_data_dir()
                    .map_err(std::io::Error::other)?
                    .join("license.bin");
                let store = Arc::new(platform::windows::WindowsLicenseStore::new(path));
                let provider =
                    option_env!("HUM_POLAR_ORGANIZATION_ID").and_then(|organization_id| {
                        license::PolarLicenseProvider::new(organization_id).ok()
                    });
                let service = match provider {
                    Some(provider) => LicenseService::release(store, Arc::new(provider)),
                    None => LicenseService::release_offline(store),
                };
                Arc::new(service)
            };
            app.manage(license_service.clone());
            let app_for_license = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let state = match license_service.bootstrap(current_unix_ms()).await {
                    Ok(state) => state,
                    Err(error) => {
                        eprintln!("license bootstrap failed: {error}");
                        license_service.state().await
                    }
                };
                apply_customer_windows(&app_for_license, state.status, initial_onboarding_version);
                let _ = app_for_license.emit("license-state-changed", &state);
            });

            let artist_cache = ArtistInfoCache::new(app.handle().clone());
            app.manage(artist_cache);

            // Prune the per-artist disk cache to its size cap in the
            // background. Runs once per launch; non-blocking on cold start.
            let handle_for_sweep = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                artist_info::sweep_disk_cache(&handle_for_sweep).await;
            });

            #[cfg(windows)]
            {
                platform::windows::WindowsMediaBackend.start(MediaBackendContext {
                    app: app.handle().clone(),
                    snapshot: snap,
                    album_art: art_state,
                    lyrics: lyrics_shared,
                    smtc_playing: smtc_active.clone(),
                });
                let output_cache = app.state::<SharedAudioOutputState>().inner().clone();
                let output_runtime = platform::windows::WindowsAudioOutputBackend
                    .start(AudioOutputBackendContext {
                        publisher: AudioOutputPublisher::new(app.handle().clone(), output_cache),
                    })
                    .map_err(std::io::Error::other)?;
                app.manage(ManagedAudioOutputRuntime::new(output_runtime));
            }
            #[cfg(not(windows))]
            {
                let _ = &smtc_active;
                let _ = &art_state;
                lyrics::start(app.handle().clone(), lyrics_shared, snap);
            }
            // Promo rotation: bootstrap from disk cache (or bundled fallback)
            // synchronously so the first ad break of the session has something
            // to show, then spawn the background refresh.
            let cache_dir = app
                .path()
                .app_config_dir()
                .or_else(|_| app.path().app_data_dir())
                .expect("app config or data dir must resolve");
            let promo_source = std::sync::Arc::new(crate::promos::SyvrRemoteSource::new(cache_dir));
            promo_source.bootstrap_load();
            {
                let src = promo_source.clone();
                tauri::async_runtime::spawn(async move {
                    src.run_refresh_loop().await;
                });
            }
            app.manage(promo_source.clone());
            // Shared "last shown" promo ID for cooldown across ad breaks.
            app.manage(
                std::sync::Arc::new(tokio::sync::RwLock::new(Option::<String>::None))
                    as std::sync::Arc<tokio::sync::RwLock<Option<String>>>,
            );

            #[cfg(windows)]
            contrast::start(app.handle().clone());

            // Streamer / OBS browser-source HTTP server. Managed via the
            // StreamerSupervisor in app state; toggled by the
            // `streamer_enabled` setting. Apply initial settings here so
            // a user who had it on at last close gets it back on start.
            app.manage::<std::sync::Arc<streamer::StreamerSupervisor>>(std::sync::Arc::new(
                streamer::StreamerSupervisor::new(),
            ));

            // Tray + mode submenu. We hold onto the CheckMenuItem handles via
            // managed state so apply_mode() can keep the checked indicator in
            // sync no matter how the mode was changed.
            let app_handle = app.handle().clone();
            streamer::apply_settings(
                &app_handle,
                streamer_enabled_at_start,
                streamer_port_at_start,
            );
            build_tray(&app_handle, initial_mode, &initial_listening_mode)?;

            // Apply the effective backdrop before installing the aspect
            // subclass so first paint and resize behavior keep their order.
            if let Some(overlay) = app.get_webview_window("overlay") {
                let window_effects = SystemWindowEffects;
                let kind = settings::effective_backdrop(
                    &app.state::<SharedSettings>().inner().blocking_read(),
                );
                if let Err(error) = window_effects.apply_backdrop(&overlay, kind) {
                    eprintln!("backdrop: apply_backdrop on startup failed: {error}");
                }
                if let Err(error) = window_effects.install_aspect(&overlay) {
                    eprintln!("[aspect_lock] {error}");
                }
            }

            // Apply the loaded mode at startup so tray icon + tooltip + window
            // cursor flag + check items all line up before first paint.
            apply_mode(&app_handle, initial_mode);

            // Ctrl+Alt+L cycles edit -> locked -> ghost -> edit.
            register_hotkey(&app_handle)?;

            // Belt + suspenders: tauri.conf.json sets `visible: false` on
            // the main (dev console) window, but Tauri dev hot-reload paths
            // and the window-state plugin have both been observed to leave
            // it visible in practice. Explicitly hide on every startup so
            // it only appears when the user clicks the tray menu item.
            if let Some(main) = app.get_webview_window("main") {
                let _ = main.hide();
            }

            // Settings + dev-console are HIDE-on-close, not destroy-on-close.
            // Both are pre-declared in tauri.conf.json with visible:false and
            // re-shown via the tray ("Settings…" / "Show / Hide dev console")
            // + open_settings_window. All of these rely on get_webview_window()
            // still resolving. The native title-bar X fires Tauri's default
            // CloseRequested, which DESTROYS the window unless we intercept it.
            // Destroying breaks two things: (1) reopening fails because
            // get_webview_window() now returns None, and (2) any in-flight
            // debounced settings write (Settings.tsx coalesces slider drags on a
            // 200ms timer) is lost when the webview's JS context is torn down
            // before the write lands. Prevent the close and hide instead so the
            // window survives and the pending write fires. The overlay has no
            // decorations (no X) and artist-info is created-on-demand + meant to
            // be destroyed, so neither needs this.
            for label in ["main", "settings", "activation", "setup"] {
                if let Some(win) = app.get_webview_window(label) {
                    let win_for_event = win.clone();
                    win.on_window_event(move |event| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                            api.prevent_close();
                            let _ = win_for_event.hide();
                        }
                    });
                }
            }

            // Window height auto-follows content via the frontend's
            // ResizeObserver in Overlay.tsx, so no empty vertical space appears
            // possible. Width is user-controllable (drag the right edge);
            // dragging text bigger via wider window. Vertical drag is
            // effectively a no-op since the next ResizeObserver fire
            // snaps height back to content.

            // Ghost-mode "click hole" for the update banner. In ghost
            // mode the whole overlay is click-through; this worker polls
            // the OS cursor position and toggles set_ignore_cursor_events
            // on/off so the small top-right banner area receives clicks
            // even though the rest of the overlay still passes them
            // through. No-op in edit / locked mode (mode.rs owns the
            // ignore_cursor_events state there).
            let banner_visible: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
            app.manage(banner_visible.clone());
            // mode_state was moved into Builder::manage above; grab the
            // managed copy back out for the poll worker's closure.
            let mode_state_clone = app.state::<SharedMode>().inner().clone();
            let app_for_poll = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                #[cfg(windows)]
                {
                    use std::time::Duration;
                    use tokio::time::sleep;
                    use window_effects::pointer::{
                        should_ignore_cursor_events, NativePoint, SystemPointerLocator,
                    };
                    let pointer_locator = SystemPointerLocator;
                    loop {
                        sleep(Duration::from_millis(80)).await;
                        let mode = OverlayMode::from_u8(mode_state_clone.load(Ordering::Acquire));
                        if !matches!(mode, OverlayMode::Ghost) {
                            continue;
                        }
                        let overlay = match app_for_poll.get_webview_window("overlay") {
                            Some(w) => w,
                            None => continue,
                        };
                        let pos = match overlay.outer_position() {
                            Ok(p) => p,
                            Err(_) => continue,
                        };
                        let visible = banner_visible.load(Ordering::Acquire);
                        let ignore_cursor_events = should_ignore_cursor_events(
                            &pointer_locator,
                            visible,
                            NativePoint { x: pos.x, y: pos.y },
                        );
                        // ignore_cursor_events = true → click passes through.
                        // We want the banner zone to receive clicks, so flip
                        // to false when cursor is over it.
                        let _ = overlay.set_ignore_cursor_events(ignore_cursor_events);
                    }
                }
                #[cfg(not(windows))]
                {
                    let _ = (mode_state_clone, app_for_poll, banner_visible);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_current_track,
            get_current_lyrics,
            get_current_album_art,
            get_overlay_mode,
            set_overlay_mode,
            cycle_overlay_mode,
            get_audio_outputs,
            get_active_audio_output,
            get_platform_info,
            get_settings,
            update_settings,
            reset_settings,
            open_settings_window,
            set_update_status,
            set_update_banner_visible,
            get_artist_info,
            clear_artist_info_cache,
            open_artist_panel_cmd,
            close_artist_panel_cmd,
            open_ticket_url,
            license::commands::get_license_state,
            license::commands::activate_license,
            license::commands::refresh_license,
            license::commands::deactivate_license,
            license::commands::open_license_window,
            license::commands::open_license_checkout,
            license::commands::open_license_portal,
            get_onboarding_state,
            onboarding::open_onboarding_window,
            complete_onboarding,
            get_about_info,
            get_build_info,
            open_trust_destination,
            request_update_check,
            export_diagnostics,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(windows)]
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(runtime) = app_handle.try_state::<ManagedAudioOutputRuntime>() {
                shutdown_managed_runtime(runtime.inner());
            }
        }
        #[cfg(not(windows))]
        let _ = (app_handle, event);
    });
}

struct ListeningModeMenuItems {
    wired: tauri::menu::CheckMenuItem<tauri::Wry>,
    speakers: tauri::menu::CheckMenuItem<tauri::Wry>,
    bluetooth: tauri::menu::CheckMenuItem<tauri::Wry>,
}

pub(crate) fn sync_listening_mode_menu(app: &tauri::AppHandle, mode: &str) {
    let Some(items) = app.try_state::<ListeningModeMenuItems>() else {
        return;
    };
    let _ = items.wired.set_checked(mode == "wired");
    let _ = items.speakers.set_checked(mode == "speakers");
    let _ = items.bluetooth.set_checked(mode == "bluetooth");
}

fn set_listening_mode_from_tray(app: &tauri::AppHandle, mode: &'static str) {
    let Some(state) = app.try_state::<SharedSettings>() else {
        return;
    };
    let state = state.inner().clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut settings = state.write().await;
        settings.listening_mode = mode.to_string();
        let snapshot = settings.clone();
        drop(settings);
        settings::save_to_store(&app, &snapshot);
        sync_listening_mode_menu(&app, mode);
        let _ = app.emit("settings-changed", &snapshot);
    });
}

fn toggle_overlay_from_tray(app: &tauri::AppHandle) {
    let Some(service) = app.try_state::<Arc<LicenseService>>() else {
        return;
    };
    let service = service.inner().clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = service.state().await;
        if !state.licensed {
            apply_customer_windows(&app, state.status, 0);
            return;
        }
        let onboarding_version = match app.try_state::<SharedSettings>() {
            Some(settings) => settings.read().await.onboarding_version,
            None => 0,
        };
        if !onboarding::onboarding_completed(onboarding_version) {
            apply_customer_windows(&app, state.status, onboarding_version);
            return;
        }
        if let Some(window) = app.get_webview_window("overlay") {
            let _ = match window.is_visible() {
                Ok(true) => window.hide(),
                _ => window.show(),
            };
        }
    });
}

fn build_tray(
    app: &tauri::AppHandle,
    initial_mode: OverlayMode,
    initial_listening_mode: &str,
) -> tauri::Result<()> {
    let toggle_overlay =
        MenuItemBuilder::with_id("toggle-overlay", "Show / Hide overlay").build(app)?;
    let mode_edit = CheckMenuItemBuilder::with_id("mode-edit", "Edit")
        .checked(matches!(initial_mode, OverlayMode::Edit))
        .build(app)?;
    let mode_locked = CheckMenuItemBuilder::with_id("mode-locked", "Locked")
        .checked(matches!(initial_mode, OverlayMode::Locked))
        .build(app)?;
    let mode_ghost = CheckMenuItemBuilder::with_id("mode-ghost", "Ghost (click-through)")
        .checked(matches!(initial_mode, OverlayMode::Ghost))
        .build(app)?;

    let mode_submenu = SubmenuBuilder::new(app, "Mode")
        .item(&mode_edit)
        .item(&mode_locked)
        .item(&mode_ghost)
        .build()?;

    let listening_wired = CheckMenuItemBuilder::with_id("listening-wired", "Wired")
        .checked(initial_listening_mode == "wired")
        .build(app)?;
    let listening_speakers = CheckMenuItemBuilder::with_id("listening-speakers", "Speakers")
        .checked(initial_listening_mode == "speakers")
        .build(app)?;
    let listening_bluetooth = CheckMenuItemBuilder::with_id("listening-bluetooth", "Bluetooth")
        .checked(initial_listening_mode == "bluetooth")
        .build(app)?;
    let listening_submenu = SubmenuBuilder::new(app, "Listening mode")
        .item(&listening_wired)
        .item(&listening_speakers)
        .item(&listening_bluetooth)
        .build()?;

    let settings_item = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
    let setup_item = MenuItemBuilder::with_id("setup", "Run setup...").build(app)?;
    let license_item = MenuItemBuilder::with_id("license", "License…").build(app)?;
    let check_updates_item =
        MenuItemBuilder::with_id("check-updates", "Check for updates").build(app)?;
    let toggle_console = if get_build_info().developer_console {
        Some(MenuItemBuilder::with_id("toggle-console", "Show / Hide dev console").build(app)?)
    } else {
        None
    };
    let quit_item = MenuItemBuilder::with_id("quit", "Quit Hum").build(app)?;

    let mut menu_builder = MenuBuilder::new(app)
        .item(&toggle_overlay)
        .separator()
        .item(&mode_submenu)
        .item(&listening_submenu)
        .separator()
        .item(&license_item)
        .item(&setup_item)
        .item(&settings_item)
        .item(&check_updates_item);
    if let Some(toggle_console) = toggle_console.as_ref() {
        menu_builder = menu_builder.item(toggle_console);
    }
    let menu = menu_builder.separator().item(&quit_item).build()?;

    app.manage(ModeMenuItems {
        edit: mode_edit,
        locked: mode_locked,
        ghost: mode_ghost,
    });
    app.manage(ListeningModeMenuItems {
        wired: listening_wired,
        speakers: listening_speakers,
        bluetooth: listening_bluetooth,
    });
    app.manage(UpdateMenuItem(check_updates_item.clone()));

    let initial_icon = Image::from_bytes(icon_for(initial_mode))?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(initial_icon)
        .icon_as_template(false)
        .tooltip(format!(
            "Hum: {} mode",
            match initial_mode {
                OverlayMode::Edit => "edit",
                OverlayMode::Locked => "locked",
                OverlayMode::Ghost => "ghost",
            }
        ))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "toggle-overlay" => {
                toggle_overlay_from_tray(app);
            }
            "mode-edit" => apply_mode(app, OverlayMode::Edit),
            "mode-locked" => apply_mode(app, OverlayMode::Locked),
            "mode-ghost" => apply_mode(app, OverlayMode::Ghost),
            "listening-wired" => set_listening_mode_from_tray(app, "wired"),
            "listening-speakers" => set_listening_mode_from_tray(app, "speakers"),
            "listening-bluetooth" => set_listening_mode_from_tray(app, "bluetooth"),
            "settings" => {
                if let Err(e) = settings::open_settings_window(app.clone()) {
                    eprintln!("[tray] open settings failed: {e}");
                }
            }
            "setup" => {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let Some(service) = app.try_state::<Arc<LicenseService>>() else {
                        return;
                    };
                    let state = service.state().await;
                    if state.licensed {
                        let _ = open_onboarding_session(&app, state.status);
                    } else {
                        apply_customer_windows(&app, state.status, 0);
                    }
                });
            }
            "license" => {
                if let Err(error) = license::commands::open_license_window(app.clone()) {
                    eprintln!("[tray] open license failed: {error}");
                }
            }
            "check-updates" => {
                // Single tray click handles both jobs:
                // - If the frontend already has an Update available,
                //   it'll install + relaunch on receiving this event.
                // - Otherwise it runs a fresh check().
                // The menu item's LABEL ("Check for updates" vs "Install
                // update vX") tells the user which it's about to do.
                if let Err(error) = request_update_check(app.clone()) {
                    eprintln!("[tray] update check failed: {error}");
                }
            }
            #[cfg(debug_assertions)]
            "toggle-console" => {
                if let Some(w) = app.get_webview_window("main") {
                    match w.is_visible() {
                        Ok(true) => {
                            let _ = w.hide();
                        }
                        _ => {
                            let _ = w.show();
                            let _ = w.set_focus();
                            let _ = w.unminimize();
                        }
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn build_global_shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri::Emitter;
    use tauri_plugin_global_shortcut::{Builder, Code, Modifiers, Shortcut, ShortcutState};

    let cycle_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyL);
    let nudge_back = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::BracketLeft);
    let nudge_fwd = Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT),
        Code::BracketRight,
    );
    let toggle_blur = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyB);
    let toggle_transparent = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyT);
    let toggle_media = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyH);

    Builder::new()
        .with_handler(move |app, shortcut, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if shortcut == &cycle_shortcut {
                if let Some(state) = app.try_state::<SharedMode>() {
                    let next = OverlayMode::from_u8(state.load(Ordering::Acquire)).next();
                    apply_mode(app, next);
                }
            } else if shortcut == &nudge_back {
                // Pull lyrics earlier (audio is ahead of lyrics).
                let _ = app.emit("lyric-offset-nudge", -250i32);
            } else if shortcut == &nudge_fwd {
                // Push lyrics later (lyrics are running ahead of audio).
                let _ = app.emit("lyric-offset-nudge", 250i32);
            } else if shortcut == &toggle_blur {
                // Toggle the blurred album-art background. Handler is sync;
                // settings.write() is async, so the flip + persist + emit
                // chain runs on the async runtime. Mirrors the pattern in
                // settings::persist_last_mode.
                if let Some(state) = app.try_state::<SharedSettings>() {
                    let state = state.inner().clone();
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let mut s = state.write().await;
                        s.blur_album_art_background = !s.blur_album_art_background;
                        let snapshot = s.clone();
                        drop(s);
                        settings::save_to_store(&app2, &snapshot);
                        let _ = app2.emit("settings-changed", &snapshot);
                    });
                }
            } else if shortcut == &toggle_transparent {
                // Transparent mode: flip bg_hidden, persist, drop/restore the
                // DWM backdrop, and notify the overlay to suppress every
                // background layer.
                if let Some(state) = app.try_state::<SharedSettings>() {
                    let state = state.inner().clone();
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let mut s = state.write().await;
                        s.bg_hidden = !s.bg_hidden;
                        let snapshot = s.clone();
                        drop(s);
                        settings::save_to_store(&app2, &snapshot);
                        reapply_effective_backdrop(&app2, &snapshot);
                        let _ = app2.emit("settings-changed", &snapshot);
                    });
                }
            } else if shortcut == &toggle_media {
                // Show/hide the metadata column ("media player").
                if let Some(state) = app.try_state::<SharedSettings>() {
                    let state = state.inner().clone();
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let mut s = state.write().await;
                        s.show_media = !s.show_media;
                        let snapshot = s.clone();
                        drop(s);
                        settings::save_to_store(&app2, &snapshot);
                        let _ = app2.emit("settings-changed", &snapshot);
                    });
                }
            }
        })
        .build()
}

/// Apply the backdrop that matches the current settings (None in transparent
/// mode, otherwise the configured backdrop) to the overlay window.
fn reapply_effective_backdrop(app: &tauri::AppHandle, s: &settings::Settings) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let window_effects = SystemWindowEffects;
        if let Err(error) = window_effects.apply_backdrop(&overlay, settings::effective_backdrop(s))
        {
            eprintln!("backdrop: re-apply on transparent toggle failed: {error}");
        }
    }
}

fn register_hotkey(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
    let cycle_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyL);
    let nudge_back = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::BracketLeft);
    let nudge_fwd = Shortcut::new(
        Some(Modifiers::CONTROL | Modifiers::ALT),
        Code::BracketRight,
    );
    let toggle_blur = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyB);
    let toggle_transparent = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyT);
    let toggle_media = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyH);
    for (name, sc) in [
        ("Ctrl+Alt+L", cycle_shortcut),
        ("Ctrl+Alt+[", nudge_back),
        ("Ctrl+Alt+]", nudge_fwd),
        ("Ctrl+Alt+B", toggle_blur),
        ("Ctrl+Alt+T", toggle_transparent),
        ("Ctrl+Alt+H", toggle_media),
    ] {
        if let Err(e) = app.global_shortcut().register(sc) {
            eprintln!("[hotkey] failed to register {name}: {e}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_shell_state_smoke_preserves_neutral_defaults() {
        let state = SharedShellState::new();

        assert_eq!(
            serde_json::to_value(state.snapshot.try_read().unwrap().clone()).unwrap(),
            serde_json::to_value(CurrentTrack::default()).unwrap()
        );
        assert!(state.album_art.try_read().unwrap().is_none());
        assert_eq!(
            serde_json::to_value(state.lyrics_state.try_read().unwrap().clone()).unwrap(),
            serde_json::to_value(CurrentLyrics::default()).unwrap()
        );
        assert_eq!(
            OverlayMode::from_u8(state.mode_state.load(Ordering::Acquire)),
            OverlayMode::Edit
        );
        assert!(!state.smtc_active.load(Ordering::Acquire));
        let audio_output = state.audio_output_state.read().unwrap();
        assert!(audio_output.outputs.is_empty());
        assert!(audio_output.active.is_none());
    }

    #[tokio::test]
    async fn raw_current_track_read_preserves_the_shared_snapshot() {
        let expected = CurrentTrack {
            title: "Portable track".into(),
            artist: "Portable artist".into(),
            duration_ms: 123_000,
            state: crate::media::PlaybackState::Paused,
            ..Default::default()
        };
        let state = Arc::new(RwLock::new(expected.clone()));

        assert_eq!(
            serde_json::to_value(read_current_track(&state).await).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    #[test]
    fn tray_console_plan_matches_the_rust_build_profile() {
        assert_eq!(get_build_info().developer_console, cfg!(debug_assertions));
    }
}
