use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::image::Image;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Emitter, Manager, Wry};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum OverlayMode {
    Edit = 0,
    Locked = 1,
    Ghost = 2,
}

impl Default for OverlayMode {
    fn default() -> Self {
        Self::Edit
    }
}

impl OverlayMode {
    pub fn next(self) -> Self {
        match self {
            Self::Edit => Self::Locked,
            Self::Locked => Self::Ghost,
            Self::Ghost => Self::Edit,
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Locked,
            2 => Self::Ghost,
            _ => Self::Edit,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Edit => "edit",
            Self::Locked => "locked",
            Self::Ghost => "ghost",
        }
    }
}

pub type SharedMode = Arc<AtomicU8>;

// Holds the three "Mode" submenu CheckMenuItem handles so apply_mode can
// reflect the current mode in the tray menu without rebuilding it.
pub struct ModeMenuItems {
    pub edit: CheckMenuItem<Wry>,
    pub locked: CheckMenuItem<Wry>,
    pub ghost: CheckMenuItem<Wry>,
}

pub const TRAY_ID: &str = "main-tray";

const TRAY_EDIT_PNG: &[u8] = include_bytes!("../icons/tray-edit.png");
const TRAY_LOCKED_PNG: &[u8] = include_bytes!("../icons/tray-locked.png");
const TRAY_GHOST_PNG: &[u8] = include_bytes!("../icons/tray-ghost.png");

pub fn current_mode(state: &SharedMode) -> OverlayMode {
    OverlayMode::from_u8(state.load(Ordering::Acquire))
}

pub fn icon_for(mode: OverlayMode) -> &'static [u8] {
    match mode {
        OverlayMode::Edit => TRAY_EDIT_PNG,
        OverlayMode::Locked => TRAY_LOCKED_PNG,
        OverlayMode::Ghost => TRAY_GHOST_PNG,
    }
}

// Single source of truth for "switch to mode X". Called from menu clicks,
// global hotkey, and the frontend invoke. Updates state, applies the
// click-through window flag, swaps the tray icon + tooltip, syncs the menu
// check items, and emits `mode-changed` for the overlay UI to react to.
pub fn apply_mode(app: &AppHandle, mode: OverlayMode) {
    if let Some(state) = app.try_state::<SharedMode>() {
        state.store(mode as u8, Ordering::Release);
    }

    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.set_ignore_cursor_events(matches!(mode, OverlayMode::Ghost));
    }

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Ok(img) = Image::from_bytes(icon_for(mode)) {
            let _ = tray.set_icon(Some(img));
        }
        let _ = tray.set_tooltip(Some(format!("Hum — {} mode", mode.as_str())));
    }

    if let Some(items) = app.try_state::<ModeMenuItems>() {
        let _ = items.edit.set_checked(matches!(mode, OverlayMode::Edit));
        let _ = items.locked.set_checked(matches!(mode, OverlayMode::Locked));
        let _ = items.ghost.set_checked(matches!(mode, OverlayMode::Ghost));
    }

    let _ = app.emit("mode-changed", mode);

    // Persist last_mode so the next cold start restores the user's choice
    // instead of always defaulting to Edit. Best-effort, async, no panic
    // path — if the settings store isn't ready yet (very early startup),
    // this is a no-op and the next mode change will succeed.
    crate::settings::persist_last_mode(app, mode);
}

#[tauri::command]
pub fn get_overlay_mode(state: tauri::State<'_, SharedMode>) -> OverlayMode {
    current_mode(&state)
}

#[tauri::command]
pub fn set_overlay_mode(app: AppHandle, mode: OverlayMode) {
    apply_mode(&app, mode);
}

#[tauri::command]
pub fn cycle_overlay_mode(app: AppHandle, state: tauri::State<'_, SharedMode>) -> OverlayMode {
    let next = current_mode(&state).next();
    apply_mode(&app, next);
    next
}
