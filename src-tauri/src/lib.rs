use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use tauri::image::Image;
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;
use tokio::sync::RwLock;

#[cfg(windows)]
mod smtc;

#[cfg(windows)]
mod itunes;

mod lyrics;
mod mode;
mod settings;

#[cfg(windows)]
use smtc::{CurrentTrack, SharedSnapshot};

#[cfg(not(windows))]
mod smtc {
    use serde::Serialize;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[derive(Clone, Serialize, Debug, Default)]
    pub struct CurrentTrack {}

    pub type SharedSnapshot = Arc<RwLock<CurrentTrack>>;
}

#[cfg(not(windows))]
use smtc::{CurrentTrack, SharedSnapshot};

use lyrics::{CurrentLyrics, SharedLyrics};
use mode::{
    apply_mode, cycle_overlay_mode, get_overlay_mode, icon_for, set_overlay_mode, ModeMenuItems,
    OverlayMode, SharedMode, TRAY_ID,
};
use settings::{
    get_settings, open_settings_window, reset_settings, update_settings, SharedSettings,
};

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
    let lyrics_state: SharedLyrics = Arc::new(RwLock::new(CurrentLyrics::default()));
    let smtc_active: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
    let mode_state: SharedMode = Arc::new(AtomicU8::new(OverlayMode::default() as u8));

    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(build_global_shortcut_plugin())
        .manage(snapshot)
        .manage(lyrics_state)
        .manage(mode_state)
        .setup(move |app| {
            let snap = app.state::<SharedSnapshot>().inner().clone();
            let lyrics_shared = app.state::<SharedLyrics>().inner().clone();

            // Load persisted settings (if any) from the store BEFORE building
            // the tray, so the initial mode + tooltip + check items reflect
            // the user's last choice rather than always Edit.
            let loaded_settings = settings::load_from_store(&app.handle());
            let initial_mode = loaded_settings.last_mode;
            app.manage::<SharedSettings>(Arc::new(RwLock::new(loaded_settings)));

            #[cfg(windows)]
            {
                smtc::start(app.handle().clone(), snap.clone(), smtc_active.clone());
                itunes::start(app.handle().clone(), snap.clone(), smtc_active.clone());
            }
            #[cfg(not(windows))]
            {
                let _ = &smtc_active;
            }

            lyrics::start(app.handle().clone(), lyrics_shared, snap);

            // Tray + mode submenu. We hold onto the CheckMenuItem handles via
            // managed state so apply_mode() can keep the checked indicator in
            // sync no matter how the mode was changed.
            let app_handle = app.handle().clone();
            build_tray(&app_handle, initial_mode)?;

            // Apply the loaded mode at startup so tray icon + tooltip + window
            // cursor flag + check items all line up before first paint.
            apply_mode(&app_handle, initial_mode);

            // Ctrl+Alt+L cycles edit -> locked -> ghost -> edit.
            register_hotkey(&app_handle)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_current_track,
            get_current_lyrics,
            get_overlay_mode,
            set_overlay_mode,
            cycle_overlay_mode,
            toggle_overlay_visibility,
            get_settings,
            update_settings,
            reset_settings,
            open_settings_window,
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
    let quit_item = MenuItemBuilder::with_id("quit", "Quit Lyric Overlay").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&toggle_overlay)
        .separator()
        .item(&mode_submenu)
        .separator()
        .item(&settings_item)
        .separator()
        .item(&quit_item)
        .build()?;

    app.manage(ModeMenuItems {
        edit: mode_edit,
        locked: mode_locked,
        ghost: mode_ghost,
    });

    let initial_icon = Image::from_bytes(icon_for(initial_mode))?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(initial_icon)
        .icon_as_template(false)
        .tooltip(format!(
            "Lyric Overlay — {} mode",
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
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn build_global_shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_global_shortcut::{Builder, Code, Modifiers, Shortcut, ShortcutState};

    let cycle_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyL);

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
            }
        })
        .build()
}

fn register_hotkey(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
    let cycle_shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyL);
    if let Err(e) = app.global_shortcut().register(cycle_shortcut) {
        eprintln!("[hotkey] failed to register Ctrl+Alt+L: {e}");
    }
    Ok(())
}
