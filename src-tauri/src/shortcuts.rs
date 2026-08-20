use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Mutex, RwLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::settings::{self, Settings, SharedSettings};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ShortcutAction {
    CycleMode,
    TimingEarlier,
    TimingLater,
    ViewPrevious,
    ViewNext,
    ToggleBlur,
    ToggleTransparent,
    ToggleMedia,
}

impl ShortcutAction {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            "cycle_mode" => Some(Self::CycleMode),
            "timing_earlier" => Some(Self::TimingEarlier),
            "timing_later" => Some(Self::TimingLater),
            "view_previous" => Some(Self::ViewPrevious),
            "view_next" => Some(Self::ViewNext),
            "toggle_blur" => Some(Self::ToggleBlur),
            "toggle_transparent" => Some(Self::ToggleTransparent),
            "toggle_media" => Some(Self::ToggleMedia),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub(crate) struct ShortcutBindings {
    pub cycle_mode: String,
    pub timing_earlier: String,
    pub timing_later: String,
    pub view_previous: String,
    pub view_next: String,
    pub toggle_blur: String,
    pub toggle_transparent: String,
    pub toggle_media: String,
}

impl Default for ShortcutBindings {
    fn default() -> Self {
        Self {
            cycle_mode: "KeyL".to_string(),
            timing_earlier: "ArrowLeft".to_string(),
            timing_later: "ArrowRight".to_string(),
            view_previous: "ArrowUp".to_string(),
            view_next: "ArrowDown".to_string(),
            toggle_blur: "KeyB".to_string(),
            toggle_transparent: "KeyT".to_string(),
            toggle_media: "KeyH".to_string(),
        }
    }
}

impl ShortcutBindings {
    fn entries(&self) -> [(ShortcutAction, &str); 8] {
        [
            (ShortcutAction::CycleMode, &self.cycle_mode),
            (ShortcutAction::TimingEarlier, &self.timing_earlier),
            (ShortcutAction::TimingLater, &self.timing_later),
            (ShortcutAction::ViewPrevious, &self.view_previous),
            (ShortcutAction::ViewNext, &self.view_next),
            (ShortcutAction::ToggleBlur, &self.toggle_blur),
            (ShortcutAction::ToggleTransparent, &self.toggle_transparent),
            (ShortcutAction::ToggleMedia, &self.toggle_media),
        ]
    }

    fn set(&mut self, action: ShortcutAction, trigger: String) {
        match action {
            ShortcutAction::CycleMode => self.cycle_mode = trigger,
            ShortcutAction::TimingEarlier => self.timing_earlier = trigger,
            ShortcutAction::TimingLater => self.timing_later = trigger,
            ShortcutAction::ViewPrevious => self.view_previous = trigger,
            ShortcutAction::ViewNext => self.view_next = trigger,
            ShortcutAction::ToggleBlur => self.toggle_blur = trigger,
            ShortcutAction::ToggleTransparent => self.toggle_transparent = trigger,
            ShortcutAction::ToggleMedia => self.toggle_media = trigger,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParsedTrigger {
    Keyboard(Shortcut),
    Mouse4,
    Mouse5,
}

fn parse_trigger(trigger: &str) -> Result<ParsedTrigger, ()> {
    match trigger {
        "Mouse4" => return Ok(ParsedTrigger::Mouse4),
        "Mouse5" => return Ok(ParsedTrigger::Mouse5),
        _ => {}
    }
    if trigger.is_empty()
        || trigger.len() > 32
        || !trigger
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(());
    }
    Shortcut::from_str(&format!("control+alt+{trigger}"))
        .map(ParsedTrigger::Keyboard)
        .map_err(|_| ())
}

pub(crate) fn validate_bindings(bindings: &ShortcutBindings) -> Result<(), String> {
    let mut seen = HashSet::new();
    for (_, trigger) in bindings.entries() {
        let parsed = parse_trigger(trigger)
            .map_err(|()| format!("Ctrl+Alt+{trigger} is not a supported global shortcut."))?;
        let identity = match parsed {
            ParsedTrigger::Keyboard(shortcut) => format!("keyboard:{}", shortcut.id()),
            ParsedTrigger::Mouse4 => "mouse:4".to_string(),
            ParsedTrigger::Mouse5 => "mouse:5".to_string(),
        };
        if !seen.insert(identity) {
            return Err(format!(
                "Ctrl+Alt+{trigger} is already assigned to another action."
            ));
        }
    }
    Ok(())
}

pub(crate) fn sanitize_bindings(bindings: &mut ShortcutBindings) {
    if validate_bindings(bindings).is_err() {
        *bindings = ShortcutBindings::default();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ViewDirection {
    Previous,
    Next,
}

pub(crate) fn cycle_view(
    shape: &str,
    layout: &str,
    direction: ViewDirection,
) -> (&'static str, &'static str) {
    match (direction, shape, layout) {
        (ViewDirection::Next, "square", _) => ("ribbon", "three_line"),
        (ViewDirection::Next, "ribbon", "three_line") => ("ribbon", "single_line"),
        (ViewDirection::Next, "ribbon", "single_line") => ("ribbon", "full_page"),
        (ViewDirection::Next, "ribbon", "full_page") => ("square", "full_page"),
        (ViewDirection::Previous, "square", _) => ("ribbon", "full_page"),
        (ViewDirection::Previous, "ribbon", "three_line") => ("square", "three_line"),
        (ViewDirection::Previous, "ribbon", "single_line") => ("ribbon", "three_line"),
        (ViewDirection::Previous, "ribbon", "full_page") => ("ribbon", "single_line"),
        _ => ("ribbon", "three_line"),
    }
}

#[derive(Clone, Copy, Default)]
struct MouseActions {
    mouse4: Option<ShortcutAction>,
    mouse5: Option<ShortcutAction>,
}

pub(crate) struct ShortcutRuntime {
    keyboard_actions: RwLock<HashMap<u32, ShortcutAction>>,
    mouse_actions: std::sync::Arc<RwLock<MouseActions>>,
    current_bindings: Mutex<Option<ShortcutBindings>>,
    #[cfg(windows)]
    mouse_worker: Mutex<Option<MouseShortcutWorker>>,
}

impl Default for ShortcutRuntime {
    fn default() -> Self {
        Self {
            keyboard_actions: RwLock::new(HashMap::new()),
            mouse_actions: std::sync::Arc::new(RwLock::new(MouseActions::default())),
            current_bindings: Mutex::new(None),
            #[cfg(windows)]
            mouse_worker: Mutex::new(None),
        }
    }
}

impl ShortcutRuntime {
    fn keyboard_action(&self, id: u32) -> Option<ShortcutAction> {
        self.keyboard_actions.read().ok()?.get(&id).copied()
    }

    #[cfg(windows)]
    fn ensure_mouse_worker(&self, app: AppHandle<Wry>) {
        let Ok(mut worker) = self.mouse_worker.lock() else {
            return;
        };
        if worker.is_none() {
            *worker = Some(MouseShortcutWorker::start(app, self.mouse_actions.clone()));
        }
    }
}

struct ParsedBindings {
    keyboard: Vec<(ShortcutAction, Shortcut, String)>,
    mouse: MouseActions,
}

fn parse_bindings(bindings: &ShortcutBindings) -> Result<ParsedBindings, String> {
    validate_bindings(bindings)?;
    let mut keyboard = Vec::new();
    let mut mouse = MouseActions::default();
    for (action, trigger) in bindings.entries() {
        match parse_trigger(trigger)
            .map_err(|()| format!("Ctrl+Alt+{trigger} is not a supported global shortcut."))?
        {
            ParsedTrigger::Keyboard(shortcut) => {
                keyboard.push((action, shortcut, trigger.to_string()));
            }
            ParsedTrigger::Mouse4 => mouse.mouse4 = Some(action),
            ParsedTrigger::Mouse5 => mouse.mouse5 = Some(action),
        }
    }
    Ok(ParsedBindings { keyboard, mouse })
}

fn register_keyboard(
    app: &AppHandle<Wry>,
    parsed: &ParsedBindings,
) -> Result<HashMap<u32, ShortcutAction>, String> {
    let mut actions = HashMap::new();
    for (action, shortcut, trigger) in &parsed.keyboard {
        if let Err(error) = app.global_shortcut().register(*shortcut) {
            eprintln!("[shortcut] Ctrl+Alt+{trigger} registration failed: {error}");
            return Err(format!(
                "Hum could not register Ctrl+Alt+{trigger}. Another app may already be using it."
            ));
        }
        actions.insert(shortcut.id(), *action);
    }
    Ok(actions)
}

pub(crate) fn apply_bindings(
    app: &AppHandle<Wry>,
    bindings: &ShortcutBindings,
) -> Result<(), String> {
    let parsed = parse_bindings(bindings)?;
    #[cfg(not(windows))]
    if parsed.mouse.mouse4.is_some() || parsed.mouse.mouse5.is_some() {
        return Err(
            "Mouse 4 and Mouse 5 shortcuts are currently available on Windows.".to_string(),
        );
    }

    let runtime = app.state::<ShortcutRuntime>();
    let previous = runtime
        .current_bindings
        .lock()
        .map_err(|_| "Hum could not update shortcuts right now.".to_string())?
        .clone();

    app.global_shortcut()
        .unregister_all()
        .map_err(|error| format!("Hum could not replace the current shortcuts: {error}"))?;

    let keyboard_actions = match register_keyboard(app, &parsed) {
        Ok(actions) => actions,
        Err(message) => {
            let _ = app.global_shortcut().unregister_all();
            if let Some(previous) = &previous {
                if let Ok(previous_parsed) = parse_bindings(previous) {
                    if let Ok(previous_actions) = register_keyboard(app, &previous_parsed) {
                        if let Ok(mut actions) = runtime.keyboard_actions.write() {
                            *actions = previous_actions;
                        }
                    }
                }
            }
            return Err(message);
        }
    };

    if let Ok(mut actions) = runtime.keyboard_actions.write() {
        *actions = keyboard_actions;
    }
    if let Ok(mut mouse) = runtime.mouse_actions.write() {
        *mouse = parsed.mouse;
    }
    if let Ok(mut current) = runtime.current_bindings.lock() {
        *current = Some(bindings.clone());
    }
    #[cfg(windows)]
    runtime.ensure_mouse_worker(app.clone());

    Ok(())
}

pub(crate) fn handle_keyboard_shortcut(
    app: &AppHandle<Wry>,
    shortcut: &Shortcut,
    event: ShortcutEvent,
) {
    if event.state() != ShortcutState::Pressed {
        return;
    }
    let Some(runtime) = app.try_state::<ShortcutRuntime>() else {
        return;
    };
    if let Some(action) = runtime.keyboard_action(shortcut.id()) {
        crate::execute_shortcut_action(app, action);
    }
}

#[tauri::command]
pub(crate) async fn set_shortcut_binding(
    app: AppHandle<Wry>,
    state: tauri::State<'_, SharedSettings>,
    action: String,
    trigger: String,
) -> Result<Settings, String> {
    let action = ShortcutAction::from_id(&action)
        .ok_or_else(|| "That shortcut action is not supported.".to_string())?;
    parse_trigger(&trigger)
        .map_err(|()| format!("Ctrl+Alt+{trigger} is not a supported global shortcut."))?;

    let snapshot = {
        let mut settings = state.write().await;
        let mut candidate = settings.clone();
        candidate.shortcuts.set(action, trigger);
        validate_bindings(&candidate.shortcuts)?;
        apply_bindings(&app, &candidate.shortcuts)?;
        *settings = candidate.clone();
        candidate
    };
    settings::save_to_store(&app, &snapshot);
    let _ = app.emit("settings-changed", &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub(crate) async fn reset_shortcuts(
    app: AppHandle<Wry>,
    state: tauri::State<'_, SharedSettings>,
) -> Result<Settings, String> {
    let snapshot = {
        let mut settings = state.write().await;
        let mut candidate = settings.clone();
        candidate.shortcuts = ShortcutBindings::default();
        apply_bindings(&app, &candidate.shortcuts)?;
        *settings = candidate.clone();
        candidate
    };
    settings::save_to_store(&app, &snapshot);
    let _ = app.emit("settings-changed", &snapshot);
    Ok(snapshot)
}

#[derive(Default)]
struct MousePressTracker {
    mouse4_down: bool,
    mouse5_down: bool,
}

impl MousePressTracker {
    fn update(&mut self, ctrl_alt: bool, mouse4: bool, mouse5: bool) -> (bool, bool) {
        let pressed_mouse4 = ctrl_alt && mouse4 && !self.mouse4_down;
        let pressed_mouse5 = ctrl_alt && mouse5 && !self.mouse5_down;
        self.mouse4_down = mouse4;
        self.mouse5_down = mouse5;
        (pressed_mouse4, pressed_mouse5)
    }
}

#[cfg(windows)]
struct MouseShortcutWorker {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl MouseShortcutWorker {
    fn start(app: AppHandle<Wry>, actions: std::sync::Arc<RwLock<MouseActions>>) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stop = stop.clone();
        let join = std::thread::spawn(move || {
            let mut tracker = MousePressTracker::default();
            while !worker_stop.load(std::sync::atomic::Ordering::Acquire) {
                let ctrl_alt =
                    key_is_down(windows::Win32::UI::Input::KeyboardAndMouse::VK_CONTROL.0)
                        && key_is_down(windows::Win32::UI::Input::KeyboardAndMouse::VK_MENU.0);
                let mouse4 =
                    key_is_down(windows::Win32::UI::Input::KeyboardAndMouse::VK_XBUTTON1.0);
                let mouse5 =
                    key_is_down(windows::Win32::UI::Input::KeyboardAndMouse::VK_XBUTTON2.0);
                let (pressed_mouse4, pressed_mouse5) = tracker.update(ctrl_alt, mouse4, mouse5);

                let current = actions.read().ok().map(|value| *value).unwrap_or_default();
                if pressed_mouse4 {
                    if let Some(action) = current.mouse4 {
                        crate::execute_shortcut_action(&app, action);
                    }
                }
                if pressed_mouse5 {
                    if let Some(action) = current.mouse5 {
                        crate::execute_shortcut_action(&app, action);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
        Self {
            stop,
            join: Some(join),
        }
    }
}

#[cfg(windows)]
impl Drop for MouseShortcutWorker {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[cfg(windows)]
fn key_is_down(key: u16) -> bool {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(key.into()) < 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_timing_and_views_on_the_arrow_cluster() {
        let bindings = ShortcutBindings::default();

        assert_eq!(bindings.timing_earlier, "ArrowLeft");
        assert_eq!(bindings.timing_later, "ArrowRight");
        assert_eq!(bindings.view_previous, "ArrowUp");
        assert_eq!(bindings.view_next, "ArrowDown");
        assert!(validate_bindings(&bindings).is_ok());
    }

    #[test]
    fn duplicate_and_unsupported_triggers_are_rejected() {
        let mut duplicate = ShortcutBindings::default();
        duplicate.view_next = duplicate.view_previous.clone();
        assert_eq!(
            validate_bindings(&duplicate).unwrap_err(),
            "Ctrl+Alt+ArrowUp is already assigned to another action."
        );

        let unsupported = ShortcutBindings {
            toggle_blur: "NotARealKey".to_string(),
            ..ShortcutBindings::default()
        };
        assert_eq!(
            validate_bindings(&unsupported).unwrap_err(),
            "Ctrl+Alt+NotARealKey is not a supported global shortcut."
        );
    }

    #[test]
    fn keyboard_and_mouse_triggers_are_classified_separately() {
        assert!(matches!(
            parse_trigger("ArrowLeft").unwrap(),
            ParsedTrigger::Keyboard(_)
        ));
        assert_eq!(parse_trigger("Mouse4").unwrap(), ParsedTrigger::Mouse4);
        assert_eq!(parse_trigger("Mouse5").unwrap(), ParsedTrigger::Mouse5);
    }

    #[test]
    fn mouse_buttons_fire_once_per_press_and_only_with_the_required_chord() {
        let mut tracker = MousePressTracker::default();

        assert_eq!(tracker.update(false, true, false), (false, false));
        assert_eq!(tracker.update(true, true, false), (false, false));
        assert_eq!(tracker.update(true, false, false), (false, false));
        assert_eq!(tracker.update(true, true, false), (true, false));
        assert_eq!(tracker.update(true, true, false), (false, false));
        assert_eq!(tracker.update(true, false, true), (false, true));
    }

    #[test]
    fn view_cycle_covers_every_ribbon_layout_and_square() {
        assert_eq!(
            cycle_view("ribbon", "three_line", ViewDirection::Next),
            ("ribbon", "single_line")
        );
        assert_eq!(
            cycle_view("ribbon", "full_page", ViewDirection::Next),
            ("square", "full_page")
        );
        assert_eq!(
            cycle_view("square", "full_page", ViewDirection::Next),
            ("ribbon", "three_line")
        );
        assert_eq!(
            cycle_view("ribbon", "three_line", ViewDirection::Previous),
            ("square", "three_line")
        );
        assert_eq!(
            cycle_view("square", "three_line", ViewDirection::Previous),
            ("ribbon", "full_page")
        );
    }
}
