pub mod aspect;
pub mod backdrop;
pub mod pointer;
pub mod screen_sampler;

use backdrop::BackdropKind;

pub trait WindowEffects {
    fn apply_backdrop(
        &self,
        window: &tauri::WebviewWindow,
        kind: BackdropKind,
    ) -> Result<(), String>;

    fn install_aspect(&self, window: &tauri::WebviewWindow) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemWindowEffects;

impl WindowEffects for SystemWindowEffects {
    fn apply_backdrop(
        &self,
        window: &tauri::WebviewWindow,
        kind: BackdropKind,
    ) -> Result<(), String> {
        #[cfg(windows)]
        {
            let raw = window.hwnd().map_err(|error| error.to_string())?;
            let hwnd = windows::Win32::Foundation::HWND(raw.0);
            backdrop::apply_native(hwnd, kind).map_err(|error| error.to_string())
        }
        #[cfg(not(windows))]
        {
            let _ = (window, kind);
            Ok(())
        }
    }

    fn install_aspect(&self, window: &tauri::WebviewWindow) -> Result<(), String> {
        #[cfg(windows)]
        {
            let raw = window.hwnd().map_err(|error| error.to_string())?;
            let hwnd = windows::Win32::Foundation::HWND(raw.0);
            aspect::install_native(hwnd)
        }
        #[cfg(not(windows))]
        {
            let _ = window;
            Ok(())
        }
    }
}
