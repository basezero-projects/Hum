use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_store::StoreExt;
use tokio::sync::RwLock;

use crate::mode::OverlayMode;
#[cfg(windows)]
use crate::backdrop::BackdropKind;

const SETTINGS_STORE_FILE: &str = "settings.json";
const SETTINGS_STORE_KEY: &str = "settings";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub last_mode: OverlayMode,

    pub anticipate_ms: i32,
    pub jitter_tolerance_ms: i32,

    pub font_family: String,
    pub font_size_px: f32,
    pub font_weight: i32,
    pub text_color: String,
    pub text_color_dim: String,
    pub bg_color: String,
    pub bg_opacity: f32,
    pub text_align: String,
    pub line_padding_px: i32,

    pub layout_mode: String,

    pub show_album_art: bool,
    pub show_translation: bool,
    /// When on, the overlay's background blends a tint of the dominant color
    /// extracted from the current track's album art. No-op when album art
    /// isn't available for the track. Defaults off so existing users aren't
    /// surprised by a color change after upgrading.
    pub tint_bg_from_album_art: bool,
    /// When on, the overlay paints a heavily blurred, dimmed copy of the
    /// current track's album art as the window background — Apple Music
    /// "Now Playing" style. The user's bg_color is rendered on top so the
    /// regular opacity slider still tints the result. No-op when album art
    /// isn't available. Defaults ON because this is the visual identity of
    /// the overlay now; existing users see a much richer background after
    /// upgrading.
    pub blur_album_art_background: bool,
    /// When on, the overlay samples a small strip of pixels just outside
    /// the window every ~2s and inverts the lyric text color based on the
    /// background's luminance — light desktop → dark text, dark desktop →
    /// light text — for readability over any background. Off by default
    /// because it overrides the user's `text_color` setting while active.
    pub auto_contrast: bool,
    /// When on, spins up a local HTTP server on `streamer_port` that
    /// serves `/state` (JSON snapshot) and `/overlay` (self-contained
    /// HTML page) so OBS / browser-source streamers can embed the
    /// lyrics in their stream. Off by default — opens a TCP port.
    pub streamer_enabled: bool,
    pub streamer_port: u16,
    /// When true, clicking album art (or the "•••" fallback dot) opens the
    /// artist-info panel window.
    pub show_artist_info_panel: bool,
    /// Windows 11 DWM backdrop applied to the overlay window.
    /// Persisted as snake_case string: "acrylic" | "mica" | "tabbed_mica" | "none".
    #[cfg(windows)]
    pub window_backdrop: BackdropKind,
    #[cfg(not(windows))]
    pub window_backdrop: String,
    /// When true, the overlay shows a rotating SYVR Studios product promo card
    /// in the lyric area during ad breaks (Spotify, Pandora, YouTube). When false,
    /// a neutral "Ad break" text is shown instead. Default true.
    #[serde(default = "default_ad_break_promos_enabled")]
    pub ad_break_promos_enabled: bool,
}

fn default_ad_break_promos_enabled() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_mode: OverlayMode::default(),
            anticipate_ms: 500,
            jitter_tolerance_ms: 2000,
            font_family: "Inter".to_string(),
            font_size_px: 26.0,
            font_weight: 600,
            text_color: "#ffffff".to_string(),
            text_color_dim: "rgba(255,255,255,0.45)".to_string(),
            bg_color: "#000000".to_string(),
            bg_opacity: 0.0,
            text_align: "left".to_string(),
            line_padding_px: 6,
            layout_mode: "three_line".to_string(),
            show_album_art: true,
            show_translation: false,
            tint_bg_from_album_art: false,
            blur_album_art_background: true,
            // Default ON — the whole point of this app is "show lyrics
            // over whatever you're doing", which means the background is
            // unpredictable. Auto-contrast keeps the text readable
            // everywhere by default. Users who want fixed colors can
            // turn it off in Settings → Extras.
            auto_contrast: true,
            streamer_enabled: false,
            // 38247 chosen as an unused-by-known-services port. Users
            // can change in Settings if it conflicts with anything local.
            streamer_port: 38247,
            show_artist_info_panel: true,
            #[cfg(windows)]
            window_backdrop: BackdropKind::Acrylic,
            #[cfg(not(windows))]
            window_backdrop: String::from("acrylic"),
            ad_break_promos_enabled: true,
        }
    }
}

pub type SharedSettings = Arc<RwLock<Settings>>;

pub fn load_from_store(app: &AppHandle) -> Settings {
    let store = match app.store(SETTINGS_STORE_FILE) {
        Ok(s) => s,
        Err(_) => return Settings::default(),
    };
    let mut loaded: Settings = match store.get(SETTINGS_STORE_KEY) {
        Some(value) => serde_json::from_value::<Settings>(value).unwrap_or_default(),
        None => Settings::default(),
    };
    // Validate on load too — protects against a hand-edited / tampered
    // settings.json that bypasses the update_settings sanitize() path.
    sanitize(&mut loaded);
    loaded
}

pub fn save_to_store(app: &AppHandle, settings: &Settings) {
    let store = match app.store(SETTINGS_STORE_FILE) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[settings] open store failed: {e}");
            return;
        }
    };
    match serde_json::to_value(settings) {
        Ok(value) => {
            store.set(SETTINGS_STORE_KEY, value);
            if let Err(e) = store.save() {
                eprintln!("[settings] save failed: {e}");
            }
        }
        Err(e) => eprintln!("[settings] serialize failed: {e}"),
    }
}

// Helper used by mode.rs so toggling mode also persists last_mode without
// the caller having to construct a full Settings or duplicate save logic.
pub fn persist_last_mode(app: &AppHandle, mode: OverlayMode) {
    let state = match app.try_state::<SharedSettings>() {
        Some(s) => s.inner().clone(),
        None => return,
    };
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut s = state.write().await;
        if s.last_mode == mode {
            return;
        }
        s.last_mode = mode;
        let snapshot = s.clone();
        drop(s);
        save_to_store(&app2, &snapshot);
    });
}

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, SharedSettings>) -> Result<Settings, String> {
    Ok(state.read().await.clone())
}

// Accepts a JSON patch (any subset of Settings fields). Merges into current
// settings, validates / clamps each field, persists, and emits
// settings-changed. Returns the new settings.
#[tauri::command]
pub async fn update_settings(
    app: AppHandle<Wry>,
    state: tauri::State<'_, SharedSettings>,
    patch: Value,
) -> Result<Settings, String> {
    #[cfg(windows)]
    let backdrop_changed = patch.get("window_backdrop").is_some();
    let merged = {
        let current = state.read().await.clone();
        let mut current_value = serde_json::to_value(&current).map_err(|e| e.to_string())?;
        if let (Value::Object(target), Value::Object(updates)) = (&mut current_value, patch) {
            for (k, v) in updates {
                target.insert(k, v);
            }
        }
        let mut parsed: Settings =
            serde_json::from_value(current_value).map_err(|e| e.to_string())?;
        sanitize(&mut parsed);
        parsed
    };

    {
        let mut s = state.write().await;
        *s = merged.clone();
    }
    save_to_store(&app, &merged);
    // React to streamer-enabled / port changes by starting or stopping the
    // local HTTP server. Idempotent if no streamer fields changed.
    crate::streamer::apply_settings(&app, merged.streamer_enabled, merged.streamer_port);
    #[cfg(windows)]
    if backdrop_changed {
        if let Some(overlay) = app.get_webview_window("overlay") {
            match overlay.hwnd() {
                Ok(raw_hwnd) => {
                    let hwnd = windows::Win32::Foundation::HWND(raw_hwnd.0);
                    if let Err(e) = crate::backdrop::apply_backdrop(hwnd, merged.window_backdrop) {
                        eprintln!("backdrop: re-apply on settings change failed: {e:?}");
                    }
                }
                Err(e) => {
                    eprintln!("backdrop: overlay.hwnd() failed on settings change: {e:?}");
                }
            }
        }
    }
    let _ = app.emit("settings-changed", &merged);
    Ok(merged)
}

// Defensive validation. Settings are user-mutable from the frontend (and from
// a hand-edited settings.json), and several fields land in inline CSS in the
// overlay. React's CSSOM assignment prevents script injection, but we still
// don't want exotic strings (semicolons, quotes, control chars) leaking into
// `font_family` / color values where they could enable CSS-side-channel
// shenanigans. Invalid values silently fall back to safe defaults.
fn sanitize(s: &mut Settings) {
    let defaults = Settings::default();

    // font_family: allow letters, digits, spaces, dashes, dots, commas. Strip
    // anything else. Empty after stripping → fall back.
    s.font_family = s
        .font_family
        .chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '.' | ',' | '_' | '\'')
        })
        .collect::<String>()
        .trim()
        .to_string();
    if s.font_family.is_empty() {
        s.font_family = defaults.font_family.clone();
    }
    if s.font_family.len() > 80 {
        s.font_family.truncate(80);
    }

    // Hex colors: must be #rrggbb. Anything else falls back.
    if !is_valid_hex_color(&s.text_color) {
        s.text_color = defaults.text_color.clone();
    }
    if !is_valid_hex_color(&s.bg_color) {
        s.bg_color = defaults.bg_color.clone();
    }

    // text_color_dim accepts hex OR rgba(...). Reject anything that contains
    // characters used in CSS expressions (`url(`, `;`, `}`, etc.).
    if !is_valid_color_string(&s.text_color_dim) {
        s.text_color_dim = defaults.text_color_dim.clone();
    }

    // Enum fields: only the known values are acceptable.
    if !matches!(s.text_align.as_str(), "left" | "center" | "right") {
        s.text_align = defaults.text_align.clone();
    }
    if !matches!(s.layout_mode.as_str(), "three_line" | "single_line" | "full_page") {
        s.layout_mode = defaults.layout_mode.clone();
    }

    // Numeric clamps to keep the UI sensible.
    s.anticipate_ms = s.anticipate_ms.clamp(-2_000, 5_000);
    s.jitter_tolerance_ms = s.jitter_tolerance_ms.clamp(0, 10_000);
    s.font_size_px = s.font_size_px.clamp(8.0, 96.0);
    s.font_weight = s.font_weight.clamp(100, 900);
    s.bg_opacity = s.bg_opacity.clamp(0.0, 100.0);
    s.line_padding_px = s.line_padding_px.clamp(0, 64);
    // Streamer port — keep above 1024 to avoid privileged-port issues,
    // below 65535 obviously. 0 → fallback to default.
    if s.streamer_port < 1024 {
        s.streamer_port = defaults.streamer_port;
    }
    #[cfg(not(windows))]
    {
        let v = s.window_backdrop.trim().to_ascii_lowercase();
        s.window_backdrop = match v.as_str() {
            "none" | "mica" | "acrylic" | "tabbed_mica" => v,
            _ => "acrylic".to_string(),
        };
    }
    // On Windows: BackdropKind's serde rejects unknown variants, and serde(default)
    // on the struct falls back to BackdropKind::default() == Acrylic. No runtime check needed.
}

fn is_valid_hex_color(s: &str) -> bool {
    if s.len() != 7 || !s.starts_with('#') {
        return false;
    }
    s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

fn is_valid_color_string(s: &str) -> bool {
    if is_valid_hex_color(s) {
        return true;
    }
    // Allow rgba(r,g,b,a) / rgb(r,g,b) — letters/digits/dots/commas/parens
    // and a leading `rgb` or `rgba` keyword. Reject any other characters
    // that could enable CSS expressions.
    let lower = s.trim().to_lowercase();
    if !(lower.starts_with("rgb(") || lower.starts_with("rgba(")) {
        return false;
    }
    if !lower.ends_with(')') {
        return false;
    }
    lower.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, ' ' | ',' | '.' | '(' | ')' | '%' | '/')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn window_backdrop_round_trips_through_serde() {
        use crate::backdrop::BackdropKind;
        let s = Settings { window_backdrop: BackdropKind::Mica, ..Default::default() };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.window_backdrop, BackdropKind::Mica);
    }

    #[cfg(windows)]
    #[test]
    fn missing_window_backdrop_defaults_to_acrylic() {
        use crate::backdrop::BackdropKind;
        let json = r#"{}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.window_backdrop, BackdropKind::Acrylic);
    }
}

#[tauri::command]
pub async fn reset_settings(
    app: AppHandle<Wry>,
    state: tauri::State<'_, SharedSettings>,
) -> Result<Settings, String> {
    let defaults = Settings::default();
    {
        let mut s = state.write().await;
        *s = defaults.clone();
    }
    save_to_store(&app, &defaults);
    let _ = app.emit("settings-changed", &defaults);
    Ok(defaults)
}

// Open / focus the settings window. Lazy-creates if not in tauri.conf.json
// pre-declared windows, or shows + focuses if already created.
#[tauri::command]
pub fn open_settings_window(app: AppHandle<Wry>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.unminimize();
        return Ok(());
    }
    // Window pre-declared in tauri.conf.json with visible:false should always
    // be retrievable above. This branch is defensive.
    Err("settings window not registered".to_string())
}

