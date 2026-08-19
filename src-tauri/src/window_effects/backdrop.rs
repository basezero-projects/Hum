use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackdropKind {
    None,
    Mica,
    #[default]
    Acrylic,
    TabbedMica,
}

impl BackdropKind {
    #[cfg(any(windows, test))]
    pub(crate) fn dwm_value(self) -> u32 {
        match self {
            Self::None => 1,
            Self::Mica => 2,
            Self::Acrylic => 3,
            Self::TabbedMica => 4,
        }
    }
}

impl<'de> Deserialize<'de> for BackdropKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BackdropVisitor;

        impl Visitor<'_> for BackdropVisitor {
            type Value = BackdropKind;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a backdrop kind string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(match value {
                    "none" => BackdropKind::None,
                    "mica" => BackdropKind::Mica,
                    "acrylic" => BackdropKind::Acrylic,
                    "tabbed_mica" => BackdropKind::TabbedMica,
                    _ => BackdropKind::default(),
                })
            }
        }

        deserializer.deserialize_str(BackdropVisitor)
    }
}

#[cfg(windows)]
pub(super) fn apply_native(
    hwnd: windows::Win32::Foundation::HWND,
    kind: BackdropKind,
) -> windows::core::Result<()> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE};

    let value = kind.dwm_value();
    // SAFETY: DwmSetWindowAttribute receives a valid pointer to a u32 and its exact size.
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
    fn wire_values_and_default_remain_compatible() {
        let cases = [
            (BackdropKind::None, "\"none\"", 1),
            (BackdropKind::Mica, "\"mica\"", 2),
            (BackdropKind::Acrylic, "\"acrylic\"", 3),
            (BackdropKind::TabbedMica, "\"tabbed_mica\"", 4),
        ];
        for (kind, json, dwm_value) in cases {
            assert_eq!(serde_json::to_string(&kind).unwrap(), json);
            assert_eq!(serde_json::from_str::<BackdropKind>(json).unwrap(), kind);
            assert_eq!(kind.dwm_value(), dwm_value);
        }
        assert_eq!(BackdropKind::default(), BackdropKind::Acrylic);
    }

    #[test]
    fn unknown_wire_value_falls_back_to_acrylic() {
        assert_eq!(
            serde_json::from_str::<BackdropKind>("\"future_backdrop\"").unwrap(),
            BackdropKind::Acrylic
        );
    }
}
