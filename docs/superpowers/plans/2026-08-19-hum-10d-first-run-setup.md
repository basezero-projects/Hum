# HUM-10D first-run setup plan

Date: 2026-08-19
Status: Complete in v0.13.65
Related phase: [HUM-10](../../roadmap/1.0/HUM-10-purchase-trust-first-run.md)
Progress ledger: [HUM-10 ledger](../../roadmap/1.0/HUM-10-progress-ledger.md)

## Outcome

A licensed customer sees a short setup the first time Hum opens. The setup keeps the real overlay visible while it helps the customer place it, choose the output they are hearing, pick a starting appearance, and understand Edit, Locked, and Ghost modes. Finishing or skipping setup saves a versioned completion marker and locks the overlay.

The tray keeps a `Run setup...` action so the customer can return without resetting preferences.

## Planned production files

- Add `src-tauri/src/onboarding.rs`
- Modify `src-tauri/src/settings.rs`
- Modify `src-tauri/src/license/commands.rs`
- Modify `src-tauri/src/license/mod.rs`
- Modify `src-tauri/src/lib.rs`
- Modify `src-tauri/tauri.conf.json`
- Modify `src-tauri/capabilities/default.json`
- Add `src/Setup.tsx`
- Add `src/setup.css`
- Modify `src/main.tsx`
- Modify `src/types.ts`
- Modify `src/Overlay.tsx` for the complete default Settings contract

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, this plan, the HUM-10 roadmap and ledger, `BUGS.md` when needed, plus Hum brain records.

## Durable state and customer window policy

- `Settings.onboarding_version` defaults to zero for clean installs and existing settings files.
- Version one is complete when `onboarding_version >= 1`. Future versions remain valid instead of being clamped backward.
- Reset Settings preserves the completion version. A preference reset must not surprise the customer with another first-run flow.
- Unlicensed states show only Activation.
- Licensed states with incomplete setup show Setup and the live overlay.
- Licensed states with complete setup show only the overlay.
- Activation, refresh, deactivation, cold start, and the tray all use the same pure window plan.
- Setup cannot mark itself complete unless the Rust license state is licensed.

## Guided flow

1. Place it: Hum switches the live overlay to Edit mode. The customer drags it and resizes it while a short visual explains the edit border.
2. Match the sound: Hum shows the detected Windows output when available. Wired, Speakers, and Bluetooth choices apply their saved delays immediately. The selected profile has a simple delay control for final correction.
3. Make it yours: the customer chooses Ribbon or Square and one of three deliberate appearance starts: Album atmosphere, Clean panel, or Lyrics only. Every choice updates the real overlay.
4. Take control: Hum explains Edit, Locked, and Ghost, then lists the timing and appearance shortcuts. Finish locks the overlay and closes setup.

Setup has Back, Continue, Finish, and Skip setup actions. Keyboard focus, live status, busy states, reduced motion, and window sizes down to the configured minimum are part of the implementation.

## Required red-first tests

- Every license status produces the correct Activation, Setup, and Overlay visibility plan for incomplete and complete setup.
- Missing persisted setup state defaults to version zero.
- Future completion versions remain complete.
- Reset Settings preserves the current completion version.
- Completion writes the current version and cannot succeed for an unlicensed state.
- The listening delay update targets only the selected Wired, Speakers, or Bluetooth profile.
- Detected wired, speaker, and Bluetooth routes map to the matching listening profile. HDMI maps to Speakers, while Unknown does not override a saved choice.
- Frontend typecheck and production build include the setup route, state contract, and styles.

## Acceptance checks

- HUM-10-AC4 can be checked after a clean licensed state opens Setup and the live overlay, every step changes the real product, Finish persists completion, and the next launch opens the overlay without Setup.
- The tray can reopen setup after completion without clearing preferences.
- Unlicensed startup and deactivation never expose setup or the overlay.
- A native visual pass covers the default setup size, minimum size, keyboard navigation, skip, finish, close and reopen, and the available detected-output states.
- The full Windows scaling matrix remains a physical HUM-10H proof item.

## Amendment gate

Stop for approval if this slice needs a new package dependency, a second settings store, changes to media-source arbitration, automatic audio-device switching after setup, more than twenty production files, or a new window system beyond one predeclared Setup window.

## Completion record

- Version: 0.13.65
- Commit: closeout commit titled `Polish Hum's first run and restore lyrics`
- Validation: frozen frontend install, typecheck, and production build; 244 Rust tests passed with one network test intentionally ignored; debug and release all-target checks passed; debug and release Clippy passed with warnings denied; full Rust formatting and diff validation passed; the ignored live NetEase smoke test passed separately against two exact songs
- Native QA: default 940 by 700 and minimum 760 by 620 fit with no page overflow; all four steps rendered; Finish persisted version one and switched to Locked; tray-style reopen returned to Place it in Edit mode; Reset Settings retained completion; synchronized lyrics rendered through NetEase while LRCLib was unavailable; the approved hummingbird appeared in Setup, the overlay, and the native title bar; original customer settings were restored byte for byte
- Review repairs: atomic reset preservation, rejected setting-write recovery, native reopen state reset, correct control reactivation, stable audio-output recommendation, focus-safe window ordering, isolated NetEase song sessions, video-duration matching, and one shared hummingbird asset family
- Deferred proof: the 100, 125, 150, and 200 percent Windows scaling matrix remains in HUM-10H
