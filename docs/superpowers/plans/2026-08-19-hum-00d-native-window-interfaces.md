# HUM-00D native window interfaces plan

Date: 2026-08-19
Status: Complete in v0.13.57
Related phase: [HUM-00](../../roadmap/1.0/HUM-00-portable-product-core.md)
Progress ledger: [HUM-00 ledger](../../roadmap/1.0/HUM-00-progress-ledger.md)

## Outcome

Backdrop, aspect handling, Ghost pointer lookup, banner hit testing, and auto-contrast capture cross small platform boundaries. Current Windows geometry, cadence, trigger rules, and failure behavior remain unchanged.

## Planned production files

Add:

- `src-tauri/src/window_effects/mod.rs`
- `src-tauri/src/window_effects/backdrop.rs`
- `src-tauri/src/window_effects/aspect.rs`
- `src-tauri/src/window_effects/pointer.rs`
- `src-tauri/src/window_effects/screen_sampler.rs`

Modify:

- `src-tauri/src/lib.rs`
- `src-tauri/src/settings.rs`
- `src-tauri/src/artist_window.rs`
- `src-tauri/src/contrast.rs`
- `src-tauri/src/streamer.rs`, added during review to keep the OBS backdrop projection target-neutral after `BackdropKind` became portable

Remove after their code and tests move:

- `src-tauri/src/backdrop.rs`
- `src-tauri/src/aspect_lock.rs`

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, `BUGS.md`, this plan, the HUM-00 roadmap and ledger, plus Hum brain records.

## Locked behavior

- Overlay startup applies the effective backdrop before installing the aspect subclass.
- Settings reapply the overlay backdrop only when `window_backdrop` or `bg_hidden` changes.
- The transparent-mode shortcut reapplies the same effective backdrop.
- The artist panel applies the configured backdrop directly. It does not use `effective_backdrop`.
- Aspect sizing derives the ratio from the current native window rectangle for every `WM_SIZING` message.
- Left, right, bottom-left, and bottom-right resize bottom from requested width.
- Top and bottom resize right from requested height.
- Top-left and top-right resize top from requested width.
- Ghost polling sleeps 80 ms before every attempt and acts only in Ghost mode.
- The update exception remains the physical-pixel rectangle `[x, x + 360)` by `[y, y + 48)` from the overlay's top-left corner.
- Pointer or position failures preserve the prior click-through state until a later successful tick.
- Auto-contrast first samples after two seconds, then every two seconds.
- Disabled auto-contrast performs no capture or emission.
- Sampling remains centered horizontally, at most 240 by 30 pixels, with a 20-pixel gap below and an equivalent above fallback.
- A successful below sample never attempts the above region.
- Capture failures retry forever and log only the first failure per process.
- The luminance payload and formula remain unchanged.
- Resetting all settings does not gain a new backdrop reapply side effect in this slice.

## Implementation steps

1. Add red-first pure tests for every current aspect edge group, zero-sized current rectangles, Ghost banner boundaries, sample placement, and below-then-above fallback order.
2. Move `BackdropKind` and its existing serde and DWM mapping tests into `window_effects/backdrop.rs`. Use the same enum on every target and preserve unknown persisted-value fallback with focused deserialization coverage.
3. Add a portable aspect rectangle and pure adjustment function, then wrap the current Windows subclass through `WindowEffects`.
4. Add `PointerLocator`, a neutral point type, the pure banner hit-test, and the Windows `GetCursorPos` adapter.
5. Add `ScreenSampler`, `SampleRegion`, `BgLuminance`, pure region selection and fallback orchestration, and the Windows `xcap` adapter.
6. Route overlay startup, live settings changes, shortcut reapplication, and artist-panel backdrop application through `WindowEffects` while preserving each call site's current configured-versus-effective choice.
7. Extract the Ghost worker to use `PointerLocator` plus the pure hit-test without changing its 80 ms cadence or failure semantics.
8. Route auto-contrast through `ScreenSampler` without changing settings checks, cadence, event payload, one-time logging, or retry behavior.
9. Remove the stale no-op `set_overlay_aspect` command and handler entry. Remove the old backdrop and aspect source files after their behavior moves.
10. Run the full gate: frontend typecheck and build, all Rust tests, all-target Cargo check, Clippy with warnings denied, full Rust formatting, and diff checks.
11. Run an independent review focused on exact native behavior, cfg boundaries, settings compatibility, and test strength.
12. Audit every changed file, bump to v0.13.57 across all manifests, update the changelog, log the reset backdrop deferral in `BUGS.md`, update the roadmap, ledger, and Hum brain records, commit, and push.

## Required red-first tests

- [x] Aspect math matches all three existing resize-edge groups.
- [x] Aspect math derives its ratio from the current rectangle and leaves the request unchanged for zero width or height.
- [x] Banner hit testing is false while hidden, includes left and top, excludes right and bottom, and stays exactly 360 by 48.
- [x] Sample placement handles narrow, wide, and negative-coordinate overlays with the exact 240 maximum width, 30 height, and 20 gap.
- [x] A successful below sample skips above.
- [x] A failed below sample tries above in order.
- [x] Two failed samples return an error for the existing retry and log path.
- [x] Backdrop wire values, default, DWM mapping, and unknown-value fallback remain compatible.

## Acceptance checks

- [x] No direct `windows`, `HWND`, `GetCursorPos`, or `xcap` use remains in `lib.rs`, `settings.rs`, `artist_window.rs`, or `contrast.rs`.
- [x] Native access crosses `WindowEffects`, `PointerLocator`, or `ScreenSampler`.
- [x] The stale aspect command is gone.
- [x] Windows trigger rules, geometry, cadence, sample order, payloads, and failure behavior are unchanged.
- [x] HUM-00-AC10 is complete.
- [x] HUM-00D is committed and pushed before HUM-00E begins.

## Amendment gate

Stop for approval if implementation needs a broad native-service container, changes the banner origin or DPI behavior, changes aspect semantics, changes auto-contrast cadence or placement, changes artist-panel backdrop selection, changes reset behavior, or expands beyond roughly twice this production file list.
