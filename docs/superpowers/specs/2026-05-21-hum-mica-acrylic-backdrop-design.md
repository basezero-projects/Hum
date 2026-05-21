# Hum — Native Windows 11 Mica / Acrylic backdrop

- **Date:** 2026-05-21
- **Author:** Claude Opus 4.7 (1M context), with Wes
- **Status:** Approved for implementation
- **Project:** Hum (`D:\Work\App_Projects\All_Projects\lyric-overlay\`)
- **Slice:** First item from the v0.10.20 "What's left" visual-polish menu
- **Predecessor session:** `docs/summaries/2026-05-21_1149_hum-visual-polish-plus-lyric-resolver-rebuild.md`

---

## Goal

Give the overlay window a real Windows 11 system backdrop — Acrylic by default, Mica and Tabbed Mica as alternatives, None to disable. The overlay window is already `transparent: true` + `decorations: false` + `shadow: false` in `tauri.conf.json` — the slot is literally waiting to be filled. This replaces the all-CSS imitation surface with a native, theme-aware, live-blurring OS layer.

This is the first slice of the post-v0.10.20 visual polish arc. Subsequent slices (mode-swap springs, cinema/ticker layouts, accent shift) will be designed against the new backdrop surface rather than against a flat transparent window.

## Non-goals

- macOS / Linux backdrops. Hum is Windows-only via SMTC; the rest of the platform surface doesn't exist.
- Per-mode backdrop variation. Edit / locked / ghost modes all use the same backdrop.
- A keyboard hotkey to cycle backdrops. Settings-only v1; we add a hotkey if the toggle frequency justifies it.
- Tuning the existing blurred-album-art tint opacity to compose better with Acrylic. Tracked as a possible follow-up only if visual review shows a conflict.
- Treating any Windows version older than 11 22H2 as a special case. The DWM call no-ops on unsupported builds — we don't gate on version explicitly.

## User-visible outcome

A new **"Window backdrop"** dropdown appears in the existing settings panel alongside the blur-album-art toggle and bg color. Four options:

| Option | What the user sees |
|---|---|
| **Acrylic** (default) | Translucent frosted-glass blur of whatever sits behind the overlay window. Updates live as background windows move. The Windows 11 fly-out / Now Playing aesthetic. |
| **Mica** | Calmer, opaque-feeling tint that adopts wallpaper color. Doesn't blur live content. The File Explorer / Settings "in-place" look. |
| **Tabbed Mica** | Mica variant with slightly different tint. Provided for parity with the DWM enum; visually similar to Mica. |
| **None** | Current behavior — fully transparent window with no OS backdrop. Existing blurred-album-art layer (if enabled) is the only background surface. |

Default is **Acrylic** on first run. Changing the dropdown re-applies the backdrop immediately; no app restart required. Persists to `settings.json` in the standard store location (`%APPDATA%\com.syvr.hum\settings.json`).

The setting is independent of the blurred-album-art toggle — both can be on, off, or any combination. Both on means: OS Acrylic at the bottom, blurred album-art div on top of that, lyrics content above.

On Windows builds older than 11 22H2 (build 22621), the OS no-ops the call and the window stays transparent as if backdrop were None. No error surfaces.

## Architecture

### Layer composition (bottom-up, when both surfaces enabled)

1. Desktop content behind the overlay window
2. **OS backdrop** (Acrylic / Mica / Tabbed) — applied via DWM, this slice's addition
3. Blurred album-art div + `bgRgba` tint — existing `BlurredAlbumBg` component
4. Lyrics content + chrome (drag-region, edit-mode dashed border, etc.)

(2) is OS-level, painted into the window's transparent regions by DWM. (3) is CSS inside the WebView. They don't fight because they're in different rendering domains; the WebView just keeps its existing transparent body and DWM fills the backdrop slot underneath.

### Module layout

- **New: `src-tauri/src/backdrop.rs`**
  - Defines `enum BackdropKind { None, Mica, Acrylic, TabbedMica }` with serde derives (`Serialize`, `Deserialize`, `Clone`, `Copy`, `PartialEq`).
  - Owns the `apply_backdrop(hwnd: HWND, kind: BackdropKind) -> windows::core::Result<()>` function.
  - Internally calls `DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &value as *const _ as *const c_void, size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32)` with the value mapped from `BackdropKind`:
    - `None → DWMSBT_NONE (1)`
    - `Mica → DWMSBT_MAINWINDOW (2)`
    - `Acrylic → DWMSBT_TRANSIENTWINDOW (3)`
    - `TabbedMica → DWMSBT_TABBEDWINDOW (4)`
  - Returns errors quietly; the caller logs them and continues.

- **Edit: `src-tauri/src/settings.rs`**
  - Add field `window_backdrop: BackdropKind`, default `BackdropKind::Acrylic`.
  - Serde tag must round-trip cleanly through `tauri-plugin-store` — use `#[serde(rename_all = "snake_case")]` on the enum so `"acrylic" / "mica" / "tabbed_mica" / "none"` are the persisted values.

- **Edit: `src-tauri/src/lib.rs`**
  - In the setup hook, after the overlay window handle is available and before `window.show()`:
    1. Read current `settings.window_backdrop`.
    2. Resolve `hwnd` via `window.hwnd()?` (Tauri 2 exposes this on Windows).
    3. Call `backdrop::apply_backdrop(hwnd, kind)`; log on Err, ignore.
  - Wire the existing `settings-changed` event chain (or its equivalent — the file already has hot-reload for `bg_color` and `blur_album_art_background`) to also re-apply backdrop when `window_backdrop` changes.

- **Edit: `src-tauri/Cargo.toml`**
  - Extend the existing `windows` crate's `features` array with `"Win32_Graphics_Dwm"`. The crate is already a dependency at 0.58 for SMTC; no new packages.

- **Edit: settings UI surface** (file location confirmed during implementation — likely `src/SettingsPanel.tsx` or wherever the existing `bg_color` / `blur_album_art_background` controls live)
  - Add a labeled `<select>` (or equivalent) bound to `window_backdrop`. Calls the existing settings-update IPC command on change.

- **Edit: `src-tauri/src/main.rs` or wherever Tauri commands are registered**
  - If the existing `update_settings`-style command takes the full Settings struct, no change needed (the new field flows through). If individual fields have setter commands, add `set_window_backdrop(kind: BackdropKind)`.

### Data flow

```
[user changes dropdown]
    → IPC: invoke("update_settings", { window_backdrop: "mica" })
    → Rust: settings.write().window_backdrop = Mica
    → Rust: persist to store, emit "settings-changed"
    → Rust: settings-changed listener calls backdrop::apply_backdrop(hwnd, Mica)
    → DWM: re-paints the window's backdrop
    → user sees the new surface immediately
```

On startup, the same `apply_backdrop` call happens once in the setup hook with the persisted value.

## Failure modes

### Pre-Win11-22H2 builds

`DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE, ...)` returns `E_INVALIDARG` on builds where the attribute isn't recognized. We log the HRESULT to console and return; the window stays transparent as if backdrop were None. The dropdown still works in the UI (selection persists) — it just has no visible effect until the user upgrades. We do not add a "your Windows is too old" warning v1; if this turns out to confuse users, we add an OS-version probe later.

### Hwnd not available

`window.hwnd()` should never fail on Windows for a created window, but if it does we log + skip. The overlay is still functional.

### Settings deserialization failure

If `settings.json` contains an unknown backdrop string (e.g., user hand-edited or older field rename), the serde default kicks in and `window_backdrop` becomes `Acrylic`. No crash.

## Auto-contrast interaction (v0.10.18)

The surface-luminance composite added in v0.10.18 reads "screen pixels" via the `bg-luminance` Rust event and composites blurred album art + `bg_color` alpha on top to produce the final `surfaceIsLight` decision. **Whether the sampler reads the desktop pixels behind the window or the final composited image including any DWM backdrop is not verified in this spec** — it depends on the Win32 capture path the existing sampler uses, which the implementation plan will check.

Two possible outcomes per backdrop kind:

- If the sampler reads the **final composited image** (Mica/Acrylic pixels included), all four backdrop kinds auto-contrast against what the user actually sees. No further work needed.
- If the sampler reads **desktop only** (under any DWM backdrop), then Acrylic mostly works (its blur preserves desktop luminance roughly) and Mica is wrong (its opaque tint hides the desktop the sampler is reading). Lyrics would auto-contrast against an invisible surface for Mica.

**Decision: ship as-is, validate manually, revisit only if any backdrop kind reads wrong.** The manual verification checklist includes a swap across all four kinds against both light and dark desktops with light and dark album art. If a backdrop kind misbehaves, the follow-up is to read Windows theme (light / dark) via `DwmGetColorizationColor` or registry and override `surfaceIsLight` to a fixed value (~0.85 light, ~0.18 dark) for that kind. Out of scope unless visual review finds a real problem.

## Other-window behavior

- **Overlay window** — gets the backdrop.
- **Dev-console window** (`label: "main"`) — keep default Tauri chrome. Backdrop is for the lyrics overlay only; the dev console is a utility window.
- **Settings window** (`label: "settings"`) — keep default Tauri chrome. Standard decorated window. The settings UI must remain legible regardless of backdrop choice.

Apply target is selected by window label inside the setup hook.

## Persistence

`window_backdrop: "acrylic" | "mica" | "tabbed_mica" | "none"` lands in `settings.json` alongside other settings. Persists via `tauri-plugin-store`'s standard save flow. Defaults to `"acrylic"` on a fresh install or a missing field.

## Testing & verification plan

Hum has no automated UI tests for the overlay — verification is manual (consistent with the existing v0.10.x slice cadence).

**Manual verification checklist (run before commit + push):**

1. `cargo check` — backend compiles, no warnings on the new file.
2. `cargo clippy --workspace --all-targets -- -D warnings` — no new lints.
3. `pnpm typecheck` — TS clean.
4. `pnpm tauri dev` — app launches, overlay appears with Acrylic backdrop (default) on fresh launch.
5. Open settings, select Mica → overlay re-paints with Mica immediately, no flicker.
6. Cycle through all four options. Each applies live.
7. Toggle blurred album-art ON with each backdrop kind — confirm both layers render together (album-art tint on top of backdrop).
8. Quit + relaunch — last-selected backdrop persists.
9. Edit `settings.json` to an invalid value (`"window_backdrop": "garbage"`) → app launches with Acrylic, no crash.
10. Edit `settings.json` to remove the field entirely → defaults to Acrylic.
11. **Auto-contrast cross-check.** For each of the four backdrop kinds, view the overlay against (a) a dark desktop with dark album art, (b) a light desktop with dark album art, (c) a dark desktop with light album art, (d) a light desktop with light album art. Confirm lyrics auto-contrast remains correct in all 16 combinations. Any failure flags the theme-aware override follow-up from the auto-contrast section.

**Pass criteria:** all 11 above behave as described, no console errors related to DWM (except the expected log line on older OS builds, which is fine).

If a Win10 machine is available, run check #4 there: app launches with no visible backdrop, no crash, log shows `DwmSetWindowAttribute returned 0x80070057` (or similar `E_INVALIDARG`). If not available, skip — the failure path is well-defined by docs.

## Open questions / decisions deferred

- **Hotkey to cycle backdrops** — deferred. Add `Ctrl+Alt+G` later if Wes finds himself toggling.
- **Per-mode backdrop** — explicitly out of scope. All modes share one backdrop.
- **Album-art tint opacity cap when Acrylic is on** — defer. Ship and see.
- **Theme-aware surface luminance override for Mica** — deferred. Ship and see.
- **Settings UI placement specifics** — confirmed during implementation when reading the settings file. The current implementation file isn't known precisely; the writing-plans skill will check.

## Files this slice touches (expected)

- `src-tauri/src/backdrop.rs` (new)
- `src-tauri/src/settings.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/Cargo.toml`
- `src-tauri/src/main.rs` (only if individual setter commands are the pattern; likely no change)
- Settings UI file (TBD during plan)
- `package.json` + `src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json` — version bump to v0.10.21
- `docs/CHANGELOG.md` — user-visible-outcome-first entry per the format in CLAUDE.md

## Out-of-scope follow-up candidates (from v0.10.20 "What's left")

Once this slice ships:

1. Mode-swap spring transitions (edit ↔ locked ↔ ghost)
2. Cinema mode + ticker mode layout toggles
3. Color-extracted accent shift (lowest priority per palette-restraint memory)

Each of those gets its own design + spec when picked up.
