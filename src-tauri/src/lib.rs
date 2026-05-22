use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use tauri::image::Image;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tokio::sync::RwLock;

#[cfg(windows)]
mod itunes;

#[cfg(windows)]
mod smtc;

#[cfg(windows)]
mod web_bridge;

#[cfg(windows)]
mod pandora_desktop;

#[cfg(windows)]
mod backdrop;

mod contrast;
mod lyrics;
mod mode;
mod settings;
mod streamer;
mod artist_info;
mod artist_window;

#[cfg(windows)]
use smtc::{AlbumArtPayload, CurrentTrack, SharedAlbumArt, SharedSnapshot};

#[cfg(not(windows))]
mod smtc {
    use serde::Serialize;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[derive(Clone, Serialize, Debug, Default)]
    pub struct CurrentTrack {}

    #[derive(Clone, Serialize, Debug)]
    pub struct AlbumArtPayload {
        pub title: String,
        pub artist: String,
        pub data_url: String,
    }

    pub type SharedSnapshot = Arc<RwLock<CurrentTrack>>;
    pub type SharedAlbumArt = Arc<RwLock<Option<AlbumArtPayload>>>;
}

#[cfg(not(windows))]
use smtc::{AlbumArtPayload, CurrentTrack, SharedAlbumArt, SharedSnapshot};

use lyrics::{CurrentLyrics, SharedLyrics};
use mode::{
    apply_mode, cycle_overlay_mode, get_overlay_mode, icon_for, set_overlay_mode, ModeMenuItems,
    OverlayMode, SharedMode, TRAY_ID,
};
use settings::{
    get_settings, open_settings_window, reset_settings, update_settings, SharedSettings,
};
use artist_info::{ArtistInfoCache, clear_artist_info_cache, get_artist_info};
use artist_window::{close_artist_panel_cmd, open_artist_panel_cmd, open_ticket_url};

#[tauri::command]
async fn get_current_track(
    state: tauri::State<'_, SharedSnapshot>,
) -> Result<CurrentTrack, String> {
    let s = state.read().await;
    Ok(s.clone())
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

/// Frontend calls this when a tray-relevant update is detected (or
/// cleared) so the "Check for updates" menu item can flip its label
/// to "Install update vX.Y.Z" — the tray becomes the actionable
/// surface; the overlay banner is just a pointer.
#[tauri::command]
fn set_update_indicator(
    app: tauri::AppHandle,
    pending_version: Option<String>,
) -> Result<(), String> {
    let item = match app.try_state::<UpdateMenuItem>() {
        Some(s) => s.0.clone(),
        None => return Err("update menu item not registered".into()),
    };
    let new_text = match pending_version {
        Some(v) => format!("Install update v{v}"),
        None => "Check for updates".to_string(),
    };
    item.set_text(new_text).map_err(|e| e.to_string())?;
    Ok(())
}

/// Managed state — handle to the dynamic-label "Check for updates" /
/// "Install update vX" tray menu item. Held in Tauri state so
/// `set_update_indicator` can find it.
struct UpdateMenuItem(MenuItem<tauri::Wry>);

/// Frontend tells us when the update banner is visible / hidden so the
/// ghost-mode cursor-poll worker knows whether to poke a clickable
/// hole in the click-through region.
#[tauri::command]
fn set_update_banner_visible(
    app: tauri::AppHandle,
    visible: bool,
) -> Result<(), String> {
    if let Some(s) = app.try_state::<Arc<AtomicBool>>() {
        s.store(visible, Ordering::Release);
    } else {
        return Err("update banner state not registered".into());
    }
    Ok(())
}

#[tauri::command]
fn toggle_overlay_visibility(app: tauri::AppHandle) -> Result<bool, String> {
    let window = app
        .get_webview_window("overlay")
        .ok_or_else(|| "overlay window missing".to_string())?;
    let visible = window.is_visible().map_err(|e| e.to_string())?;
    if visible {
        window.hide().map_err(|e| e.to_string())?;
        Ok(false)
    } else {
        window.show().map_err(|e| e.to_string())?;
        Ok(true)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let snapshot: SharedSnapshot = Arc::new(RwLock::new(CurrentTrack::default()));
    let album_art: SharedAlbumArt = Arc::new(RwLock::new(None));
    let lyrics_state: SharedLyrics = Arc::new(RwLock::new(CurrentLyrics::default()));
    let smtc_active: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let mode_state: SharedMode = Arc::new(AtomicU8::new(OverlayMode::default() as u8));

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        // Save / restore position + size for the OVERLAY window only.
        // Dev console and settings windows are not tracked — they always
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
        .setup(move |app| {
            let snap = app.state::<SharedSnapshot>().inner().clone();
            let art_state = app.state::<SharedAlbumArt>().inner().clone();
            let lyrics_shared = app.state::<SharedLyrics>().inner().clone();

            // Load persisted settings (if any) from the store BEFORE building
            // the tray, so the initial mode + tooltip + check items reflect
            // the user's last choice rather than always Edit.
            let loaded_settings = settings::load_from_store(app.handle());
            let initial_mode = loaded_settings.last_mode;
            // Capture streamer fields before move so we can apply after manage.
            let streamer_enabled_at_start = loaded_settings.streamer_enabled;
            let streamer_port_at_start = loaded_settings.streamer_port;
            app.manage::<SharedSettings>(Arc::new(RwLock::new(loaded_settings)));
                let artist_cache = ArtistInfoCache::new(app.handle().clone());
                app.manage(artist_cache);

            #[cfg(windows)]
            {
                smtc::start(
                    app.handle().clone(),
                    snap.clone(),
                    art_state.clone(),
                    smtc_active.clone(),
                );
                itunes::start(
                    app.handle().clone(),
                    snap.clone(),
                    art_state.clone(),
                    smtc_active.clone(),
                );
                let shared_bridge: web_bridge::SharedWebBridge =
                    std::sync::Arc::new(tokio::sync::RwLock::new(None));
                app.manage(shared_bridge.clone());
                web_bridge::start(app.handle().clone(), snap.clone(), shared_bridge.clone());
                lyrics::start(app.handle().clone(), lyrics_shared, snap, shared_bridge);
            }
            #[cfg(not(windows))]
            {
                let _ = &smtc_active;
                let _ = &art_state;
                lyrics::start(app.handle().clone(), lyrics_shared, snap);
            }
            contrast::start(app.handle().clone());

            // Streamer / OBS browser-source HTTP server. Managed via the
            // StreamerSupervisor in app state; toggled by the
            // `streamer_enabled` setting. Apply initial settings here so
            // a user who had it on at last close gets it back on start.
            app.manage::<std::sync::Arc<streamer::StreamerSupervisor>>(
                std::sync::Arc::new(streamer::StreamerSupervisor::new()),
            );

            // Tray + mode submenu. We hold onto the CheckMenuItem handles via
            // managed state so apply_mode() can keep the checked indicator in
            // sync no matter how the mode was changed.
            let app_handle = app.handle().clone();
            streamer::apply_settings(
                &app_handle,
                streamer_enabled_at_start,
                streamer_port_at_start,
            );
            build_tray(&app_handle, initial_mode)?;

            // Apply the persisted DWM backdrop before first paint so the OS
            // compositor effect is in place when the overlay window renders.
            #[cfg(windows)]
            {
                if let Some(overlay) = app.get_webview_window("overlay") {
                    match overlay.hwnd() {
                        Ok(raw_hwnd) => {
                            // Tauri may bundle a different windows-crate version internally;
                            // bridge via raw isize. Mirrors web_bridge.rs::PandoraProbe::read().
                            let hwnd = windows::Win32::Foundation::HWND(raw_hwnd.0);
                            let kind = app.state::<SharedSettings>().inner().blocking_read().window_backdrop;
                            if let Err(e) = backdrop::apply_backdrop(hwnd, kind) {
                                eprintln!("backdrop: apply_backdrop on startup failed: {e:?}");
                            }
                        }
                        Err(e) => {
                            eprintln!("backdrop: overlay.hwnd() failed: {e:?}");
                        }
                    }
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

            // Window height auto-follows content via the frontend's
            // ResizeObserver in Overlay.tsx — no empty vertical space
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
                    use windows::Win32::Foundation::POINT;
                    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
                    const BANNER_ZONE_W: i32 = 360;
                    loop {
                        sleep(Duration::from_millis(80)).await;
                        let mode = OverlayMode::from_u8(
                            mode_state_clone.load(Ordering::Acquire),
                        );
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
                        let in_zone = if visible {
                            let mut pt = POINT { x: 0, y: 0 };
                            // SAFETY: GetCursorPos writes to the POINT we
                            // own on the stack; no aliasing.
                            if unsafe { GetCursorPos(&mut pt) }.is_ok() {
                                // Banner now sits at the very top of the
                                // overlay's content area (the outer-stack
                                // column lays it out above the art+lyrics
                                // row). Click hole = top-left, ~360px wide
                                // by 48px tall, which covers the
                                // container's 12px top padding + ~24px of
                                // banner content + a few px of buffer.
                                const BANNER_ZONE_H: i32 = 48;
                                let left = pos.x;
                                let top = pos.y;
                                pt.x >= left
                                    && pt.x < left + BANNER_ZONE_W
                                    && pt.y >= top
                                    && pt.y < top + BANNER_ZONE_H
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        // ignore_cursor_events = true → click passes through.
                        // We want the banner zone to receive clicks, so flip
                        // to false when cursor is over it.
                        let _ = overlay.set_ignore_cursor_events(!in_zone);
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
            toggle_overlay_visibility,
            get_settings,
            update_settings,
            reset_settings,
            open_settings_window,
            set_update_indicator,
            set_update_banner_visible,
            get_artist_info,
            clear_artist_info_cache,
            open_artist_panel_cmd,
            close_artist_panel_cmd,
            open_ticket_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_tray(app: &tauri::AppHandle, initial_mode: OverlayMode) -> tauri::Result<()> {
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

    let settings_item = MenuItemBuilder::with_id("settings", "Settings…").build(app)?;
    let check_updates_item =
        MenuItemBuilder::with_id("check-updates", "Check for updates").build(app)?;
    let toggle_console =
        MenuItemBuilder::with_id("toggle-console", "Show / Hide dev console").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "Quit Hum").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&toggle_overlay)
        .separator()
        .item(&mode_submenu)
        .separator()
        .item(&settings_item)
        .item(&check_updates_item)
        .item(&toggle_console)
        .separator()
        .item(&quit_item)
        .build()?;

    app.manage(ModeMenuItems {
        edit: mode_edit,
        locked: mode_locked,
        ghost: mode_ghost,
    });
    app.manage(UpdateMenuItem(check_updates_item.clone()));

    let initial_icon = Image::from_bytes(icon_for(initial_mode))?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(initial_icon)
        .icon_as_template(false)
        .tooltip(format!(
            "Hum — {} mode",
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
                if let Some(w) = app.get_webview_window("overlay") {
                    let _ = match w.is_visible() {
                        Ok(true) => w.hide(),
                        _ => w.show(),
                    };
                }
            }
            "mode-edit" => apply_mode(app, OverlayMode::Edit),
            "mode-locked" => apply_mode(app, OverlayMode::Locked),
            "mode-ghost" => apply_mode(app, OverlayMode::Ghost),
            "settings" => {
                if let Err(e) = settings::open_settings_window(app.clone()) {
                    eprintln!("[tray] open settings failed: {e}");
                }
            }
            "check-updates" => {
                use tauri::Emitter;
                // Single tray click handles both jobs:
                // - If the frontend already has an Update available,
                //   it'll install + relaunch on receiving this event.
                // - Otherwise it runs a fresh check().
                // The menu item's LABEL ("Check for updates" vs "Install
                // update vX") tells the user which it's about to do.
                let _ = app.emit("updater-check-requested", ());
            }
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
    let nudge_fwd = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::BracketRight);
    let toggle_blur = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyB);

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
            }
        })
        .build()
}

fn register_hotkey(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
    let cycle_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyL);
    let nudge_back = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::BracketLeft);
    let nudge_fwd = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::BracketRight);
    let toggle_blur = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyB);
    for (name, sc) in [
        ("Ctrl+Alt+L", cycle_shortcut),
        ("Ctrl+Alt+[", nudge_back),
        ("Ctrl+Alt+]", nudge_fwd),
        ("Ctrl+Alt+B", toggle_blur),
    ] {
        if let Err(e) = app.global_shortcut().register(sc) {
            eprintln!("[hotkey] failed to register {name}: {e}");
        }
    }
    Ok(())
}
