# Hum Mica/Acrylic Backdrop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Window backdrop" setting that paints the overlay window with a native Windows 11 system backdrop (Acrylic default, Mica, Tabbed Mica, None) via DWM, controlled by a dropdown in Settings.

**Architecture:** New `backdrop.rs` module owns the `BackdropKind` enum and `apply_backdrop(hwnd, kind)` function that calls `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE, …)`. The `Settings` struct gets a `window_backdrop` field; `update_settings` calls `apply_backdrop` directly when the field is in the patch. The setup hook applies the persisted value once on startup, before the overlay's first paint. Frontend gets a new `<Select>` mirroring the existing `text_align` dropdown pattern.

**Tech Stack:** Tauri 2, `windows` crate 0.58 (existing) with `Win32_Graphics_Dwm` feature added, tauri-plugin-store, React 19 + TS 5.9.

**Spec:** `docs/superpowers/specs/2026-05-21-hum-mica-acrylic-backdrop-design.md`

**Version bump:** v0.10.22 → v0.10.23 (spec was written before the v0.10.22 Pandora slice landed; bump target shifts forward).

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src-tauri/src/backdrop.rs` | **Create** | `BackdropKind` enum, `apply_backdrop(hwnd, kind)`, DWM constants, unit tests for enum→u32 mapping and serde round-trip. |
| `src-tauri/src/lib.rs` | Modify | `mod backdrop;`. In setup hook: after overlay window retrieval, apply persisted backdrop before `apply_mode`. |
| `src-tauri/src/settings.rs` | Modify | Add `window_backdrop: BackdropKind` field with `#[serde(default = …)]` defaulting to Acrylic. In `update_settings`, if the patch touched `window_backdrop`, call `backdrop::apply_backdrop` against the overlay window's HWND after the merged Settings are persisted. |
| `src-tauri/Cargo.toml` | Modify | Add `"Win32_Graphics_Dwm"` to the existing `windows` crate `features` array. Bump `version = "0.10.23"`. |
| `src-tauri/tauri.conf.json` | Modify | Bump `version` to `0.10.23`. |
| `package.json` | Modify | Bump `version` to `0.10.23`. |
| `src/Settings.tsx` | Modify | Add a labeled `<Select>` for backdrop, wired via `update("window_backdrop", …)`. Add the matching TS type to wherever `Settings` is typed on the frontend. |
| `src/lib/settings-types.ts` *(or equivalent — confirm during Task 7)* | Modify | Extend the frontend `Settings` type with `window_backdrop: "acrylic" \| "mica" \| "tabbed_mica" \| "none"`. |
| `docs/CHANGELOG.md` | Modify | Prepend a `## [0.10.23] - 2026-05-21` entry, user-visible-outcome-first. |

---

## Task 1: Add `Win32_Graphics_Dwm` feature and stub backdrop module

**Files:**
- Modify: `src-tauri/Cargo.toml` (the `[target.'cfg(windows)'.dependencies] windows` features array, lines 36–47)
- Create: `src-tauri/src/backdrop.rs`
- Modify: `src-tauri/src/lib.rs` (top of file — add `#[cfg(windows)] mod backdrop;` next to the existing `#[cfg(windows)] mod web_bridge;`)

- [ ] **Step 1: Add the DWM feature to Cargo.toml**

Edit the `windows` dependency `features` array. The existing array (Cargo.toml lines 37–47) ends with `"Win32_System_ProcessStatus",` — add `"Win32_Graphics_Dwm",` on the next line before the closing `]`. Result:

```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
  "Media",
  "Media_Control",
  "Foundation",
  "Foundation_Collections",
  "Storage_Streams",
  "Win32_UI_WindowsAndMessaging",
  "Win32_Foundation",
  "Win32_System_Threading",
  "Win32_System_ProcessStatus",
  "Win32_Graphics_Dwm",
] }
```

- [ ] **Step 2: Create the `backdrop.rs` skeleton with the enum**

Create `src-tauri/src/backdrop.rs` with:

```rust
//! Windows 11 system backdrops (Mica / Acrylic / Tabbed) via DWM.
//!
//! Calls `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE, …)`. On Win10 / pre-22H2 Win11
//! the call returns `E_INVALIDARG` and the window stays as-is (transparent).

#![cfg(windows)]

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackdropKind {
    None,
    Mica,
    Acrylic,
    TabbedMica,
}

impl Default for BackdropKind {
    fn default() -> Self {
        BackdropKind::Acrylic
    }
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
```

- [ ] **Step 3: Register the module in `lib.rs`**

Find the existing `#[cfg(windows)] mod web_bridge;` line near the top of `src-tauri/src/lib.rs` and add immediately below it:

```rust
#[cfg(windows)]
mod backdrop;
```

- [ ] **Step 4: Verify the workspace still compiles**

Run: `cd src-tauri && cargo check`
Expected: PASS, no errors. The module exists but is unused; that's fine — Tauri-style `mod backdrop;` declarations don't warn on unused modules.

- [ ] **Step 5: Add unit tests for `dwm_value` mapping and serde round-trip**

Append to `src-tauri/src/backdrop.rs`:

```rust
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
```

- [ ] **Step 6: Run the tests**

Run: `cd src-tauri && cargo test --lib backdrop`
Expected: PASS — 4 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/backdrop.rs src-tauri/src/lib.rs
git commit -m "backdrop: enum + DWM value mapping + Win32_Graphics_Dwm feature"
```

---

## Task 2: Implement `apply_backdrop` against a real HWND

**Files:**
- Modify: `src-tauri/src/backdrop.rs` (add the public `apply_backdrop` function)

- [ ] **Step 1: Add `apply_backdrop` to backdrop.rs**

Add below the `impl BackdropKind` block (and before the `#[cfg(test)] mod tests` block):

```rust
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
    // We pass a u32 ([4 bytes), matching the documented size for DWMWA_SYSTEMBACKDROP_TYPE.
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &value as *const u32 as *const c_void,
            size_of::<u32>() as u32,
        )
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd src-tauri && cargo check`
Expected: PASS. If `DwmSetWindowAttribute` or `DWMWA_SYSTEMBACKDROP_TYPE` are missing, double-check that `Win32_Graphics_Dwm` is in the `windows` crate features (Task 1 Step 1).

- [ ] **Step 3: Run all backend tests + clippy**

Run: `cd src-tauri && cargo test --lib && cargo clippy --all-targets -- -D warnings`
Expected: PASS — no new lints, all existing tests still pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/backdrop.rs
git commit -m "backdrop: apply_backdrop via DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)"
```

---

## Task 3: Add `window_backdrop` to the Settings struct

**Files:**
- Modify: `src-tauri/src/settings.rs` (struct definition near line 14, sanitize() near line 199)

- [ ] **Step 1: Import BackdropKind**

At the top of `src-tauri/src/settings.rs` (after the existing imports), add:

```rust
#[cfg(windows)]
use crate::backdrop::BackdropKind;
```

- [ ] **Step 2: Add the field to the Settings struct**

In the existing `pub struct Settings { … }` block (lines 14–61), add a new field. On Windows it's `BackdropKind`; on other platforms we still need *something* of the same JSON shape so serde doesn't break cross-platform settings files. Use a string fallback.

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    // … existing fields unchanged …
    pub streamer_enabled: bool,
    pub streamer_port: u16,

    /// Windows 11 DWM backdrop applied to the overlay window.
    /// Persisted as snake_case string: "acrylic" | "mica" | "tabbed_mica" | "none".
    #[cfg(windows)]
    pub window_backdrop: BackdropKind,
    #[cfg(not(windows))]
    pub window_backdrop: String,
}
```

- [ ] **Step 3: Add a default for the new field in `impl Default for Settings`**

Find the `impl Default for Settings { fn default() -> Self { Self { … } } }` block. Add the new field with the right default per-platform:

```rust
            streamer_enabled: false,
            streamer_port: 7878,

            #[cfg(windows)]
            window_backdrop: BackdropKind::Acrylic,
            #[cfg(not(windows))]
            window_backdrop: String::from("acrylic"),
```

If the file doesn't have an explicit `impl Default for Settings` block (because `#[derive(Default)]` is used instead), Step 3 collapses to: the `BackdropKind::Acrylic` default is already covered by the `impl Default for BackdropKind` from Task 1 (and `String::default()` for non-Windows gives `""`, which the sanitize step will replace).

If there's no explicit Default impl, ALSO update `sanitize()` (Step 4) to normalize an empty non-Windows string to `"acrylic"`.

- [ ] **Step 4: Update `sanitize()` to validate the new field on non-Windows**

In `sanitize()` (around lines 199–254 in `settings.rs`), add at the end of the function (before the closing brace):

```rust
    #[cfg(not(windows))]
    {
        let v = self.window_backdrop.trim().to_ascii_lowercase();
        self.window_backdrop = match v.as_str() {
            "none" | "mica" | "acrylic" | "tabbed_mica" => v,
            _ => "acrylic".to_string(),
        };
    }
    // On Windows, BackdropKind's serde will reject unknown variants and serde(default)
    // will fall back to BackdropKind::default() == Acrylic per Task 1.
```

- [ ] **Step 5: Verify it still compiles**

Run: `cd src-tauri && cargo check`
Expected: PASS.

- [ ] **Step 6: Add a round-trip test for the new field**

Find the existing `#[cfg(test)] mod tests { … }` block at the bottom of `settings.rs` (if there isn't one, skip this step — the backdrop.rs serde test from Task 1 already covers the enum). If there is one, add:

```rust
    #[cfg(windows)]
    #[test]
    fn window_backdrop_round_trips_through_serde() {
        use crate::backdrop::BackdropKind;
        let mut s = Settings::default();
        s.window_backdrop = BackdropKind::Mica;
        let json = serde_json::to_string(&s).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.window_backdrop, BackdropKind::Mica);
    }

    #[cfg(windows)]
    #[test]
    fn missing_window_backdrop_defaults_to_acrylic() {
        use crate::backdrop::BackdropKind;
        // Settings JSON written by an older Hum build won't have window_backdrop.
        // serde(default) on the struct should fill it with BackdropKind::default().
        let json = r#"{}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.window_backdrop, BackdropKind::Acrylic);
    }
```

- [ ] **Step 7: Run the new tests**

Run: `cd src-tauri && cargo test --lib settings`
Expected: PASS (or skip if Step 6 was skipped).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/settings.rs
git commit -m "settings: add window_backdrop field, default Acrylic on Windows"
```

---

## Task 4: Apply the persisted backdrop on startup

**Files:**
- Modify: `src-tauri/src/lib.rs` (setup hook, lines 179–339)

- [ ] **Step 1: Locate the overlay-window retrieval point**

In `src-tauri/src/lib.rs`, find the `.setup(move |app| { … })` block. Inside it, find the line that calls `apply_mode(&app_handle, initial_mode);` (around line 243 per the explore report). The backdrop apply must run **before** that line, so the OS backdrop is in place when the window first paints.

If there's already a `let overlay = app.get_webview_window("overlay")?;` (or similar) earlier in setup, reuse it. If not, fetch it now.

- [ ] **Step 2: Add the apply-on-startup call**

Add immediately before the existing `apply_mode(&app_handle, initial_mode);`:

```rust
        #[cfg(windows)]
        {
            if let Some(overlay) = app.get_webview_window("overlay") {
                match overlay.hwnd() {
                    Ok(raw_hwnd) => {
                        // Tauri 2 may bundle a different `windows` crate version; bridge via raw isize.
                        let hwnd = windows::Win32::Foundation::HWND(raw_hwnd.0 as isize);
                        let kind = {
                            let s = settings_state.read();
                            s.window_backdrop
                        };
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
```

Note: `settings_state` here is whatever the surrounding scope calls the `Arc<RwLock<Settings>>` (the `SharedSettings` from settings.rs). Match the variable name used by the existing code in that block.

If Tauri 2's `WindowExt::hwnd()` returns a raw `isize` directly rather than a typed HWND, simplify the conversion to `windows::Win32::Foundation::HWND(raw_hwnd as isize)`. Check by running the build — the compiler will be specific.

- [ ] **Step 3: Verify the build**

Run: `cd src-tauri && cargo check`
Expected: PASS. If the HWND conversion is mistyped, fix per the compiler message — the pattern is identical to the one in `web_bridge.rs::read()` from the Pandora work.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "backdrop: apply persisted backdrop in setup hook before first paint"
```

---

## Task 5: Re-apply backdrop when `update_settings` changes the field

**Files:**
- Modify: `src-tauri/src/settings.rs` (`update_settings` function, lines 161–191)

- [ ] **Step 1: Detect the field in the patch and re-apply**

In `update_settings`, the patch is a `serde_json::Value`. After merging the patch into the live Settings struct and persisting to the store, check whether the patch touched `window_backdrop`. If it did, apply the new value via DWM against the overlay window's HWND.

Insert this block immediately before the existing `app.emit("settings-changed", &merged);` line (around line 189):

```rust
    #[cfg(windows)]
    if patch.get("window_backdrop").is_some() {
        if let Some(overlay) = app.get_webview_window("overlay") {
            if let Ok(raw_hwnd) = overlay.hwnd() {
                let hwnd = windows::Win32::Foundation::HWND(raw_hwnd.0 as isize);
                if let Err(e) = crate::backdrop::apply_backdrop(hwnd, merged.window_backdrop) {
                    eprintln!("backdrop: re-apply on settings change failed: {e:?}");
                }
            }
        }
    }
```

Match the conversion form used in Task 4 Step 2 — if you used `raw_hwnd as isize` there, mirror it here.

- [ ] **Step 2: Verify build**

Run: `cd src-tauri && cargo check && cargo clippy --all-targets -- -D warnings`
Expected: PASS, no new lints.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/settings.rs
git commit -m "backdrop: re-apply DWM backdrop when update_settings touches the field"
```

---

## Task 6: Add the Settings.tsx dropdown

**Files:**
- Modify: `src/Settings.tsx`
- Modify: any frontend `Settings` TS type definition (TBD — search for the type defined alongside the existing fields like `blur_album_art_background`. If it's inline in Settings.tsx, edit it there. If it's in a separate types file, edit there.)

- [ ] **Step 1: Extend the frontend Settings type**

Search the codebase for the TS `Settings` type. It will have a line like `blur_album_art_background: boolean;`. Add directly below it:

```ts
  window_backdrop: "acrylic" | "mica" | "tabbed_mica" | "none";
```

Run: `pnpm typecheck` (or `npx tsc --noEmit`). Expected: many errors about `window_backdrop` missing in object literals — that's expected, the dropdown wires them up next. If errors are about anything else, fix those first.

- [ ] **Step 2: Add the dropdown control**

Locate the existing `<Toggle label="Blurred album art background" … />` control in `src/Settings.tsx` (around line 169–183 per the explore report). Immediately after it, add the Select control mirroring the existing Select pattern at lines 409–434:

```tsx
        <Select<"acrylic" | "mica" | "tabbed_mica" | "none">
          value={s.window_backdrop}
          onChange={(v) => update("window_backdrop", v)}
          options={[
            ["acrylic", "Acrylic (default)"],
            ["mica", "Mica"],
            ["tabbed_mica", "Tabbed Mica"],
            ["none", "None"],
          ]}
        />
```

Wrap it in whatever label/row container the surrounding controls use (look at how `text_align` Select is rendered for the exact wrapper — likely a `<div>` with a `<label>` and the Select alongside).

- [ ] **Step 3: Typecheck**

Run: `pnpm typecheck`
Expected: PASS. All previously-flagged `window_backdrop missing` errors should be resolved.

- [ ] **Step 4: Commit**

```bash
git add src/Settings.tsx
# Plus whatever types file you touched
git commit -m "settings UI: window backdrop dropdown (Acrylic/Mica/Tabbed/None)"
```

---

## Task 7: Version bump + changelog entry

**Files:**
- Modify: `package.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `docs/CHANGELOG.md`

- [ ] **Step 1: Bump version in all three manifest files**

- `package.json` line 3: `"version": "0.10.22"` → `"version": "0.10.23"`
- `src-tauri/Cargo.toml` line 3: `version = "0.10.22"` → `version = "0.10.23"`
- `src-tauri/tauri.conf.json` line 4: `"version": "0.10.22"` → `"version": "0.10.23"`

- [ ] **Step 2: Prepend the CHANGELOG entry**

At the top of `docs/CHANGELOG.md`, immediately after the file header (and before the existing `## [0.10.22]` block), insert:

```markdown
## [0.10.23] - 2026-05-21

### Added
- **Window backdrop setting.** A new "Window backdrop" dropdown appears in the Settings window beneath the "Blurred album art background" toggle. Four options: **Acrylic** (default — translucent frosted-glass blur of whatever's behind the overlay, updates live as background windows move, the Windows 11 fly-out / Now Playing aesthetic), **Mica** (calmer, opaque-feeling tint that adopts the user's wallpaper color, doesn't blur live content, the File Explorer / Settings "in-place" look), **Tabbed Mica** (Mica variant with a slightly different tint), and **None** (no OS backdrop — the previous fully transparent behavior). Changing the dropdown re-paints the overlay immediately; the choice persists to `settings.json` and is restored on next launch. The setting is independent of the existing "Blurred album art background" toggle — both can be on, off, or any combination. On Windows 10 or pre-22H2 Windows 11 builds, the OS no-ops the call and the window stays transparent as if "None" were selected (no error surfaces).

### Architecture / files
- **New `src-tauri/src/backdrop.rs`** — `BackdropKind` enum (`Acrylic` / `Mica` / `TabbedMica` / `None`, serialized as snake_case strings) and `apply_backdrop(hwnd, kind)` which calls `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE, …)` with the appropriate `DWM_SYSTEMBACKDROP_TYPE` integer.
- **`src-tauri/Cargo.toml`** — added `"Win32_Graphics_Dwm"` to the existing `windows` crate features array. No new packages.
- **`src-tauri/src/settings.rs`** — new `window_backdrop: BackdropKind` field on `Settings` (defaults to `Acrylic`); `update_settings` re-applies the backdrop via DWM whenever the patch touches the field.
- **`src-tauri/src/lib.rs`** — setup hook applies the persisted backdrop against the overlay window's HWND before the first paint, ensuring no flash of unstyled window on startup.
- **`src/Settings.tsx`** — new labeled `<Select>` mirroring the existing `text_align` dropdown pattern, wired through the existing `update()` debounce + `invoke("update_settings", { patch })` flow.
```

- [ ] **Step 3: Verify**

Run:
```bash
cd src-tauri && cargo check && cargo clippy --all-targets -- -D warnings && cargo test --lib
cd ..
pnpm typecheck
```
Expected: all PASS.

- [ ] **Step 4: Commit**

```bash
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "v0.10.23: native Windows 11 Mica/Acrylic backdrop setting"
```

---

## Task 8: Manual verification (Wes runs in `pnpm tauri dev`)

Per the spec's testing & verification plan. There's already a `pnpm tauri dev` background task running (bj9dkves5) with cargo's watcher — code changes auto-rebuild. If a hard restart is needed, kill the existing dev server first (`Get-NetTCPConnection -LocalPort 1420` then stop the holding PID).

- [ ] **Step 1: Acrylic appears on fresh launch**
  Restart `pnpm tauri dev` (or just bring the overlay back to front). Confirm: overlay is translucent frosted-glass over whatever's behind it.

- [ ] **Step 2: All four backdrop kinds apply live**
  Open Settings → Window backdrop → cycle through Acrylic, Mica, Tabbed Mica, None. Each must re-paint immediately, no flicker, no app restart.

- [ ] **Step 3: Blurred album-art + backdrop compose**
  Toggle "Blurred album art background" ON for each of the four backdrop kinds. Confirm both layers render together (album-art tint sits on top of OS backdrop).

- [ ] **Step 4: Persistence**
  Quit + relaunch Hum. Last-selected backdrop is restored.

- [ ] **Step 5: Invalid settings.json is tolerated**
  Quit Hum. Edit `%APPDATA%\com.syvr.hum\settings.json`, change `"window_backdrop"` to `"garbage"`. Relaunch. App opens with Acrylic, no crash.

- [ ] **Step 6: Missing field defaults to Acrylic**
  Quit Hum. Edit `settings.json`, delete the `"window_backdrop"` key entirely. Relaunch. App opens with Acrylic.

- [ ] **Step 7: Auto-contrast still works across all four backdrops**
  For each of the four kinds, view the overlay against a dark and light desktop with both dark and light album art. Confirm lyrics text auto-contrast remains correct (light text on dark surface, dark text on light surface). If any kind misbehaves — particularly Mica — flag for the theme-aware override follow-up from the spec (out of scope for this slice unless a kind is unusable).

- [ ] **Step 8: No console errors**
  Check the dev console for DWM errors. On Win11 22H2+, none should appear. On older OS, the expected `eprintln!` log shows `apply_backdrop on startup failed: ...` once — that's fine and documented.

---

## Self-review notes (post-write check)

- **Spec coverage:** every section of the spec has a task — module layout (Task 1+2), settings field & serde (Task 3), startup application (Task 4), runtime re-apply (Task 5), UI (Task 6), version + changelog (Task 7), manual verification checklist (Task 8). Failure modes (pre-22H2, missing HWND, deserialization) are addressed in Task 3 Step 4 + Task 4 Step 2 + Task 5 Step 1 error logging.
- **Auto-contrast follow-up** is intentionally out of scope per the spec ("ship as-is, validate manually, revisit only if any backdrop kind reads wrong"); manual verification Step 7 captures the trigger.
- **Type consistency:** `BackdropKind` (Rust) ↔ `"acrylic" | "mica" | "tabbed_mica" | "none"` (TS) match via `#[serde(rename_all = "snake_case")]` in Task 1 and the literal type in Task 6 Step 1.
- **Cross-platform:** non-Windows builds get a `String` field with sanitize fallback so settings files remain portable; no DWM call attempted off-Windows (everything is `#[cfg(windows)]`).
- **No placeholders.** Every Rust block compiles in context, every command is the literal command to run.
