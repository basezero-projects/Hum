use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_store::StoreExt;
use tokio::sync::RwLock;

use crate::mode::OverlayMode;
use crate::window_effects::backdrop::BackdropKind;
use crate::window_effects::{SystemWindowEffects, WindowEffects};

pub(crate) const SETTINGS_STORE_FILE: &str = "settings.json";
const SETTINGS_STORE_KEY: &str = "settings";
const CURRENT_PROMO_POLICY_VERSION: u8 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub last_mode: OverlayMode,
    #[serde(default)]
    pub onboarding_version: u8,

    pub anticipate_ms: i32,
    /// Active audio-output timing profile. Stored as a string so unknown
    /// values from hand-edited or future settings files can be sanitized
    /// without discarding the rest of the user's settings.
    pub listening_mode: String,
    pub wired_delay_ms: i32,
    pub speakers_delay_ms: i32,
    pub bluetooth_delay_ms: i32,
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
    pub overlay_shape: String,

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
    pub window_backdrop: BackdropKind,
    /// When true, the overlay shows an optional Hum product card in the lyric
    /// area during ad breaks. Paid-safe defaults and migrations keep this off
    /// until the customer chooses to enable it.
    #[serde(default)]
    pub ad_break_promos_enabled: bool,
    /// Version of the paid-product promo default applied to this saved state.
    /// Version zero is the legacy on-by-default policy.
    #[serde(default = "legacy_promo_policy_version")]
    pub promo_policy_version: u8,
    /// When true, Hum launches automatically when the user signs into their PC.
    /// Off by default — opt-in. The actual OS-level registration is handled by
    /// tauri-plugin-autostart (Windows registry Run key, macOS LaunchAgent,
    /// Linux .desktop). Settings is the source of truth; `update_settings`
    /// syncs the plugin state on every save.
    #[serde(default)]
    pub launch_on_startup: bool,
    /// Transparent mode (Ctrl+Alt+T): suppress every background layer in the
    /// overlay AND drop the DWM window backdrop to None, so only the lyrics /
    /// art / metadata float over the desktop. Default off.
    #[serde(default)]
    pub bg_hidden: bool,
    /// Show the right-hand metadata column ("media player"). Toggle: Ctrl+Alt+M.
    /// Default on.
    #[serde(default = "default_true")]
    pub show_media: bool,
}

fn legacy_promo_policy_version() -> u8 {
    0
}

fn default_true() -> bool {
    true
}

/// The DWM backdrop that should actually be applied to the overlay window for
/// a given settings state: None while transparent mode is on (so the window is
/// truly see-through), otherwise the user's configured backdrop.
pub fn effective_backdrop(s: &Settings) -> BackdropKind {
    if s.bg_hidden {
        BackdropKind::None
    } else {
        s.window_backdrop
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_mode: OverlayMode::default(),
            onboarding_version: 0,
            anticipate_ms: 0,
            listening_mode: "wired".to_string(),
            wired_delay_ms: 0,
            speakers_delay_ms: 250,
            bluetooth_delay_ms: 350,
            jitter_tolerance_ms: 2000,
            font_family: "Inter".to_string(),
            font_size_px: 26.0,
            font_weight: 600,
            text_color: "#ffffff".to_string(),
            text_color_dim: "#c8c8c8".to_string(),
            bg_color: "#000000".to_string(),
            bg_opacity: 0.0,
            text_align: "left".to_string(),
            line_padding_px: 6,
            layout_mode: "three_line".to_string(),
            overlay_shape: "ribbon".to_string(),
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
            window_backdrop: BackdropKind::Acrylic,
            ad_break_promos_enabled: false,
            promo_policy_version: CURRENT_PROMO_POLICY_VERSION,
            launch_on_startup: false,
            bg_hidden: false,
            show_media: true,
        }
    }
}

pub type SharedSettings = Arc<RwLock<Settings>>;

pub fn selected_profile_delay_ms(s: &Settings) -> i32 {
    match s.listening_mode.as_str() {
        "speakers" => s.speakers_delay_ms,
        "bluetooth" => s.bluetooth_delay_ms,
        _ => s.wired_delay_ms,
    }
}

/// Offset applied to the interpolated player position before lyric lookup.
/// Positive anticipation looks ahead, while an output-device delay looks
/// back because the listener hears that audio later than the player reports.
pub fn effective_timing_offset_ms(s: &Settings) -> i32 {
    s.anticipate_ms - selected_profile_delay_ms(s)
}

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
    let promo_policy_migrated = sanitize(&mut loaded);
    if promo_policy_migrated {
        save_to_store(app, &loaded);
    }
    loaded
}

/// Reconcile the OS-level autostart registration with the saved setting.
/// Called from `update_settings` on every save and from app setup on launch
/// so externally-edited registry / LaunchAgent state can't drift out of sync.
/// Errors are logged but non-fatal — the user can still use Hum if registry
/// writes fail (e.g. locked-down work machine).
pub fn sync_autostart(app: &AppHandle, enabled: bool) {
    let manager = app.autolaunch();
    let is_enabled = match manager.is_enabled() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[autostart] is_enabled() failed: {e}");
            // Try the requested action anyway — best-effort.
            false
        }
    };
    if is_enabled == enabled {
        return;
    }
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    if let Err(e) = result {
        eprintln!(
            "[autostart] {} failed: {e}",
            if enabled { "enable" } else { "disable" }
        );
    }
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
    // bg_hidden also drives the effective backdrop (None vs configured), so a
    // change to either must re-apply it.
    let backdrop_changed =
        patch.get("window_backdrop").is_some() || patch.get("bg_hidden").is_some();
    // Hold the write lock for the full read-merge-write so two windows
    // (Overlay + Settings) calling `update_settings` concurrently can't
    // clobber each other. Previously the read-clone happened under a
    // released read lock, the merge ran lock-free, and the write lock was
    // re-acquired at the end — leaving a window where a parallel call
    // could read the same baseline and lose this update on its own write.
    let merged = {
        let mut s = state.write().await;
        let mut current_value = serde_json::to_value(&*s).map_err(|e| e.to_string())?;
        if let (Value::Object(target), Value::Object(updates)) = (&mut current_value, patch) {
            for (k, v) in updates {
                target.insert(k, v);
            }
        }
        let mut parsed: Settings =
            serde_json::from_value(current_value).map_err(|e| e.to_string())?;
        sanitize(&mut parsed);
        *s = parsed.clone();
        parsed
    };

    save_to_store(&app, &merged);
    crate::sync_listening_mode_menu(&app, &merged.listening_mode);
    // React to streamer-enabled / port changes by starting or stopping the
    // local HTTP server. Idempotent if no streamer fields changed.
    crate::streamer::apply_settings(&app, merged.streamer_enabled, merged.streamer_port);
    // Sync the OS-level autostart registration with the saved setting. Idempotent
    // — calling enable/disable when already in that state is a no-op for the
    // plugin. Errors are non-fatal (e.g. user without write access to the
    // registry); we log and move on so the settings save itself still succeeds.
    sync_autostart(&app, merged.launch_on_startup);
    if backdrop_changed {
        if let Some(overlay) = app.get_webview_window("overlay") {
            let window_effects = SystemWindowEffects;
            if let Err(error) = window_effects.apply_backdrop(&overlay, effective_backdrop(&merged))
            {
                eprintln!("backdrop: re-apply on settings change failed: {error}");
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
fn sanitize(s: &mut Settings) -> bool {
    let defaults = Settings::default();

    // The free preview defaulted promos on and stored no provenance for that
    // value. Apply the paid-safe default once, then preserve any later opt-in.
    let promo_policy_migrated = s.promo_policy_version < CURRENT_PROMO_POLICY_VERSION;
    if promo_policy_migrated {
        s.ad_break_promos_enabled = false;
        s.promo_policy_version = CURRENT_PROMO_POLICY_VERSION;
    }

    // font_family: allow letters, digits, spaces, dashes, dots, commas. Strip
    // anything else. Empty after stripping → fall back.
    s.font_family = s
        .font_family
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '.' | ',' | '_' | '\''))
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
    // One-shot migration: the old default was `rgba(255,255,255,0.45)`, which
    // washed out on bright album-art backgrounds because the background bled
    // through the alpha and tinted the dim text. New default is a solid light
    // gray. Users who explicitly customized it keep their value.
    if s.text_color_dim == "rgba(255,255,255,0.45)" {
        s.text_color_dim = defaults.text_color_dim.clone();
    }

    // Enum fields: only the known values are acceptable.
    if !matches!(s.text_align.as_str(), "left" | "center" | "right") {
        s.text_align = defaults.text_align.clone();
    }
    if !matches!(
        s.layout_mode.as_str(),
        "three_line" | "single_line" | "full_page"
    ) {
        s.layout_mode = defaults.layout_mode.clone();
    }
    if !matches!(s.overlay_shape.as_str(), "ribbon" | "square") {
        s.overlay_shape = defaults.overlay_shape.clone();
    }
    if !matches!(
        s.listening_mode.as_str(),
        "wired" | "speakers" | "bluetooth"
    ) {
        s.listening_mode = defaults.listening_mode.clone();
    }

    // Numeric clamps to keep the UI sensible.
    s.anticipate_ms = s.anticipate_ms.clamp(-2_000, 5_000);
    s.wired_delay_ms = s.wired_delay_ms.clamp(0, 2_000);
    s.speakers_delay_ms = s.speakers_delay_ms.clamp(0, 2_000);
    s.bluetooth_delay_ms = s.bluetooth_delay_ms.clamp(0, 2_000);
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

    promo_policy_migrated
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
    lower
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | ',' | '.' | '(' | ')' | '%' | '/'))
}

fn reset_defaults(current: &Settings) -> Settings {
    Settings {
        onboarding_version: current.onboarding_version,
        ..Settings::default()
    }
}

fn reset_in_place(current: &mut Settings) -> Settings {
    let defaults = reset_defaults(current);
    *current = defaults.clone();
    defaults
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_shape_defaults_to_ribbon_when_missing() {
        let settings: Settings = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(settings.overlay_shape, "ribbon");
    }

    #[test]
    fn overlay_shape_sanitizes_unknown_values() {
        let mut settings = Settings {
            overlay_shape: "portrait".to_string(),
            ..Default::default()
        };

        sanitize(&mut settings);

        assert_eq!(settings.overlay_shape, "ribbon");
    }

    #[test]
    fn overlay_shape_preserves_square() {
        let mut settings = Settings {
            overlay_shape: "square".to_string(),
            ..Default::default()
        };

        sanitize(&mut settings);

        assert_eq!(settings.overlay_shape, "square");
    }

    #[test]
    fn listening_profiles_default_when_missing() {
        let settings: Settings = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(settings.listening_mode, "wired");
        assert_eq!(settings.wired_delay_ms, 0);
        assert_eq!(settings.speakers_delay_ms, 250);
        assert_eq!(settings.bluetooth_delay_ms, 350);
    }

    #[test]
    fn invalid_listening_mode_falls_back_to_wired() {
        let mut settings = Settings {
            listening_mode: "car_stereo".to_string(),
            ..Default::default()
        };

        sanitize(&mut settings);

        assert_eq!(settings.listening_mode, "wired");
    }

    #[test]
    fn listening_profile_delays_are_clamped() {
        let mut settings = Settings {
            wired_delay_ms: -1,
            speakers_delay_ms: 2_001,
            bluetooth_delay_ms: 9_000,
            ..Default::default()
        };

        sanitize(&mut settings);

        assert_eq!(settings.wired_delay_ms, 0);
        assert_eq!(settings.speakers_delay_ms, 2_000);
        assert_eq!(settings.bluetooth_delay_ms, 2_000);
    }

    #[test]
    fn effective_offset_uses_selected_profile() {
        let settings = Settings {
            anticipate_ms: 100,
            listening_mode: "bluetooth".to_string(),
            bluetooth_delay_ms: 350,
            ..Default::default()
        };

        assert_eq!(selected_profile_delay_ms(&settings), 350);
        assert_eq!(effective_timing_offset_ms(&settings), -250);
    }

    #[test]
    fn window_backdrop_round_trips_through_serde() {
        let s = Settings {
            window_backdrop: BackdropKind::Mica,
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.window_backdrop, BackdropKind::Mica);
    }

    #[test]
    fn missing_window_backdrop_defaults_to_acrylic() {
        let json = r#"{}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.window_backdrop, BackdropKind::Acrylic);
    }

    #[test]
    fn unknown_persisted_window_backdrop_defaults_to_acrylic() {
        let json = r#"{"window_backdrop":"future_backdrop"}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.window_backdrop, BackdropKind::Acrylic);
    }

    #[test]
    fn missing_onboarding_version_defaults_to_zero() {
        let settings: Settings = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(settings.onboarding_version, 0);
    }

    #[test]
    fn reset_defaults_preserve_onboarding_completion() {
        let current = Settings {
            onboarding_version: 7,
            overlay_shape: "square".to_string(),
            ..Default::default()
        };

        let reset = reset_defaults(&current);

        assert_eq!(reset.onboarding_version, 7);
        assert_eq!(reset.overlay_shape, "ribbon");
    }

    #[test]
    fn reset_replaces_preferences_without_releasing_the_settings_lock() {
        let mut current = Settings {
            onboarding_version: 4,
            listening_mode: "bluetooth".to_string(),
            ..Default::default()
        };

        let saved = reset_in_place(&mut current);

        assert_eq!(current, saved);
        assert_eq!(current.onboarding_version, 4);
        assert_eq!(current.listening_mode, "wired");
    }

    #[test]
    fn paid_promo_policy_starts_fresh_and_reset_settings_off() {
        let fresh = Settings::default();
        assert!(!fresh.ad_break_promos_enabled);

        let reset = reset_defaults(&Settings {
            ad_break_promos_enabled: true,
            ..Settings::default()
        });
        assert!(!reset.ad_break_promos_enabled);
    }

    #[test]
    fn paid_promo_policy_migrates_legacy_on_once() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"ad_break_promos_enabled":true}"#).unwrap();

        assert!(sanitize(&mut settings));

        assert!(!settings.ad_break_promos_enabled);
        assert_eq!(
            serde_json::to_value(&settings).unwrap()["promo_policy_version"],
            1
        );

        settings.ad_break_promos_enabled = true;
        assert!(!sanitize(&mut settings));
        assert!(settings.ad_break_promos_enabled);
    }

    #[test]
    fn paid_promo_policy_preserves_current_explicit_opt_in() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"promo_policy_version":1,"ad_break_promos_enabled":true}"#)
                .unwrap();

        assert!(!sanitize(&mut settings));

        assert!(settings.ad_break_promos_enabled);
        assert_eq!(
            serde_json::to_value(&settings).unwrap()["promo_policy_version"],
            1
        );
    }
}

#[tauri::command]
pub async fn reset_settings(
    app: AppHandle<Wry>,
    state: tauri::State<'_, SharedSettings>,
) -> Result<Settings, String> {
    let defaults = {
        let mut current = state.write().await;
        reset_in_place(&mut current)
    };
    save_to_store(&app, &defaults);
    crate::sync_listening_mode_menu(&app, &defaults.listening_mode);
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
