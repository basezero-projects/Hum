# HUM-00E platform information and React plan

Date: 2026-08-19
Status: Complete in v0.13.58
Related phase: [HUM-00](../../roadmap/1.0/HUM-00-portable-product-core.md)
Progress ledger: [HUM-00 ledger](../../roadmap/1.0/HUM-00-progress-ledger.md)

## Outcome

React receives one tested Rust payload describing the features this build actually supports and the paths it actually uses. Settings hides unavailable native controls, shows resolved storage paths, and keeps the current Windows experience unchanged.

## Planned production files

- Add `src-tauri/src/platform/info.rs`
- Modify `src-tauri/src/platform/mod.rs`
- Modify `src-tauri/src/settings.rs`
- Modify `src-tauri/src/lib.rs`
- Modify `src/types.ts`
- Modify `src/Settings.tsx`
- Modify `src/Overlay.tsx`
- Modify `src/DevConsole.tsx`

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, `BUGS.md` when needed, this plan, the HUM-00 roadmap and ledger, plus Hum brain records.

## Payload contract

`PlatformInfo` contains:

- `platform`: `windows`, `macos`, or `linux`
- `media.playback`
- `audio_output.discovery` and `audio_output.active_output_changes`
- `window.supported_backdrops`, `aspect_lock`, `click_through`, `update_banner_pointer_exception`, and `screen_sampling`
- `services.tray`, `global_shortcuts`, `autostart`, and `updater`
- `paths.app_data_dir` and `paths.settings_file`

## Capability facts

- Windows reports current playback, all four backdrop values, aspect locking, click-through, the update-banner pointer exception, screen sampling, tray, shortcuts, autostart, and updater support.
- macOS reports no media backend, audio-output discovery, backdrop, aspect, screen sampling, pointer exception, or updater yet. It reports click-through, tray, shortcuts, and autostart support from the initialized Tauri plugins and APIs.
- Linux reports no media backend, audio-output discovery, backdrop, aspect, screen sampling, pointer exception, click-through, or updater yet. It reports tray and autostart. Global shortcuts report false under Wayland and true otherwise.
- Audio-output discovery and active-output changes remain false on every platform until HUM-00F.
- Paths come from Tauri's resolved application data directory and the canonical `settings.json` filename. No path is fabricated on resolution failure.

## Locked Windows behavior

- Every current Settings control, value, order, label, and default remains visible on Windows.
- Manual Wired, Speakers, and Bluetooth timing profiles remain visible on every platform and do not depend on discovery support.
- Windows still runs its updater check and listener wiring.
- Tray contents, shortcut registration, mode persistence, autostart behavior, updater packaging, and source labels do not change.

## React behavior

- Settings loads settings and platform information together.
- A failed load shows a visible retryable error instead of an endless loading state.
- The backdrop row is built from `supported_backdrops` and hidden when the list is empty.
- Auto-contrast is hidden when `screen_sampling` is false.
- Ghost is hidden from the mode selector when `click_through` is false.
- Autostart is hidden when unsupported and uses platform-neutral sign-in copy when shown.
- Shortcut hints appear only when global shortcuts are supported.
- The footer shows both resolved application data and settings-file paths.
- Overlay waits for platform information before updater setup and skips the updater path when unsupported.
- Dev console copy becomes platform-neutral.
- React does not use `navigator.platform` or infer capabilities from the browser runtime.

## Implementation steps

1. Add red-first exact payload tests for Windows, macOS, Linux X11, and Linux Wayland plus resolved path construction and failure.
2. Add the target-neutral platform models, pure capability builder, and `get_platform_info` command.
3. Expose the canonical settings filename within Rust and resolve paths from Tauri.
4. Register the command and add the exact TypeScript contract.
5. Load settings and platform information together in Settings, add the visible failure state, and gate only the controls and hints named above.
6. Gate the updater setup in Overlay through `services.updater` while keeping the Windows path unchanged.
7. Replace Windows-only dev-console presentation copy with platform-neutral text.
8. Run the full gate: frontend typecheck and build, all Rust tests, all-target Cargo check, Clippy with warnings denied, full Rust formatting, and diff checks.
9. Run an independent review focused on capability truth, Windows parity, path correctness, loading errors, and updater effect cleanup.
10. Audit every changed file, bump to v0.13.58 across all manifests, update changelog and roadmap records, log out-of-scope findings, update the ledger and Hum brain records, commit, and push.

## Required red-first tests

- [x] Windows payload is exact, including four backdrop values and audio-output fields false.
- [x] macOS payload is exact and does not claim an updater or media backend.
- [x] Linux X11 and Wayland differ only where current shortcut support differs.
- [x] Linux does not claim unproven click-through support.
- [x] Settings file equals the resolved app-data directory joined with the canonical filename.
- [x] Path-resolution failure returns an error.

## Acceptance checks

- [x] `get_platform_info` is React's only operating-system capability source.
- [x] No frontend platform sniffing is added.
- [x] Windows Settings and updater behavior remain unchanged.
- [x] Unsupported native controls are absent.
- [x] Settings shows Rust-resolved application-data and settings-file paths.
- [x] Audio-output discovery remains false and no endpoint enumeration is added.
- [x] HUM-00-AC5 and HUM-00-AC6 are complete.
- [x] HUM-00E is committed and pushed before HUM-00F begins.

## Amendment gate

Stop for approval if implementation needs a new frontend test framework, changes tray or shortcut registration, changes updater packaging, adds audio endpoint discovery, adds frontend OS sniffing, duplicates store path rules outside Rust, or expands beyond roughly twice this production file list.
