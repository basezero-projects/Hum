//! Windows 11 system backdrops (Mica / Acrylic / Tabbed) via DWM.
//!
//! Calls `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE, …)`. On Win10 / pre-22H2 Win11
//! the call returns `E_INVALIDARG` and the window stays as-is (transparent).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackdropKind {
    None,
    Mica,
    #[default]
    Acrylic,
    TabbedMica,
}

impl BackdropKind {
    /// Maps to the `DWM_SYSTEMBACKDROP_TYPE` enum value passed to `DwmSetWindowAttribute`.
    pub(crate) fn dwm_value(self) -> u32 {
        // Microsoft's DWM_SYSTEMBACKDROP_TYPE: Auto=0, None=1, MainWindow(Mica)=2,
        // TransientWindow(Acrylic)=3, TabbedWindow(TabbedMica)=4.
        match self {
            BackdropKind::None => 1,
            BackdropKind::Mica => 2,
            BackdropKind::Acrylic => 3,
            BackdropKind::TabbedMica => 4,
        }
    }
}

use std::ffi::c_void;
use std::mem::size_of;
use windows::core::Result as WinResult;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE};

/// Apply the given backdrop kind to `hwnd` via DWM.
///
/// On Win11 22H2+ the OS paints the requested backdrop. On older builds DWM returns
/// `E_INVALIDARG`; we surface it so the caller can log + continue.
pub fn apply_backdrop(hwnd: HWND, kind: BackdropKind) -> WinResult<()> {
    let value: u32 = kind.dwm_value();
    // SAFETY: DwmSetWindowAttribute requires a pointer to the value and its size in bytes.
    // We pass a u32 (4 bytes), matching the documented size for DWMWA_SYSTEMBACKDROP_TYPE.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &value as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dwm_values_match_microsoft_enum() {
        assert_eq!(BackdropKind::None.dwm_value(), 1);
        assert_eq!(BackdropKind::Mica.dwm_value(), 2);
        assert_eq!(BackdropKind::Acrylic.dwm_value(), 3);
        assert_eq!(BackdropKind::TabbedMica.dwm_value(), 4);
    }

    #[test]
    fn default_is_acrylic() {
        assert_eq!(BackdropKind::default(), BackdropKind::Acrylic);
    }

    #[test]
    fn serde_round_trips_snake_case() {
        let cases = [
            (BackdropKind::None, "\"none\""),
            (BackdropKind::Mica, "\"mica\""),
            (BackdropKind::Acrylic, "\"acrylic\""),
            (BackdropKind::TabbedMica, "\"tabbed_mica\""),
        ];
        for (kind, json) in cases {
            assert_eq!(serde_json::to_string(&kind).unwrap(), json);
            assert_eq!(serde_json::from_str::<BackdropKind>(json).unwrap(), kind);
        }
    }

    #[test]
    fn unknown_variant_fails_cleanly() {
        let result: Result<BackdropKind, _> = serde_json::from_str("\"garbage\"");
        assert!(result.is_err());
    }
}
