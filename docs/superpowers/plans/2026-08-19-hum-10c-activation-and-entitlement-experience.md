# HUM-10C activation and entitlement experience plan

Date: 2026-08-19
Status: Complete in v0.13.64
Related phase: [HUM-10](../../roadmap/1.0/HUM-10-purchase-trust-first-run.md)
Progress ledger: [HUM-10 ledger](../../roadmap/1.0/HUM-10-progress-ledger.md)

## Outcome

A release customer sees one clear Hum license window, can buy Hum or enter the key from their receipt, can restore the same purchase after reinstalling, can retry verification, and can release this PC. The overlay stays hidden whenever the safe Rust license state is not licensed. Development builds continue to open the overlay without touching customer storage or Polar.

This slice completes the local customer workflow. Creating the production Polar product and proving a real purchase still require live provider setup.

## Planned production files

- Add `src-tauri/src/license/commands.rs`
- Modify `src-tauri/src/license/mod.rs`
- Modify `src-tauri/src/lib.rs`
- Modify `src-tauri/tauri.conf.json`
- Modify `src-tauri/capabilities/default.json`
- Add `src/Activation.tsx`
- Add `src/activation.css`
- Modify `src/main.tsx`
- Modify `src/types.ts`

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, this plan, the HUM-10 roadmap and ledger, `BUGS.md` when needed, plus Hum brain records.

## Backend command contract

- `get_license_state` returns only the safe `LicenseState` payload from HUM-10A.
- `activate_license` trims and validates a customer key, calls the serialized HUM-10B service, updates the native window gate, and emits `license-state-changed`.
- `refresh_license` retries protected storage and Polar verification, then updates the same state and windows.
- `deactivate_license` releases Polar before deleting the local record, then keeps the license window open and hides the overlay.
- `open_license_window` shows, focuses, and restores the predeclared license window.
- `open_license_checkout` and `open_license_portal` open compile-time public HTTPS URLs only after a strict Polar-host allowlist check. Missing configuration returns a useful error without exposing a URL or license material.
- Keys are never placed in a URL, event, log, diagnostic string, or command error.

## Entitlement and window behavior

- The overlay starts hidden on every build so a release cannot flash lyrics before protected state loads.
- Development bootstrap shows the overlay and keeps the development entitlement.
- Verified, verification due, and offline grace states show the overlay.
- Unlicensed, verification required, invalid, revoked, device limit, clock error, and service unavailable states hide the overlay and show the license window.
- Closing the license window hides it instead of destroying it. An unlicensed customer can reopen it from the tray.
- The tray gains a `License` item. Its Show or Hide overlay action opens the license window instead of bypassing an unlicensed state.
- Activation success hides the license window and shows the overlay. Successful deactivation does the reverse.

## Activation window design

The visual direction is a quiet hi-fi instrument panel, not a generic checkout card. It uses Hum's black, warm ivory, and muted gold palette, the existing hummingbird mark, subtle echo rings, a narrow purchase-policy rail, and a focused activation form.

The window includes:

- The $19 one-time price, three personal Windows devices, 30-day refund, and Hum 1.x updates
- A Buy Hum button that opens Polar checkout
- A clearly labeled license-key field with paste-friendly formatting and Enter-to-submit behavior
- Contextual active, due, offline grace, expired verification, invalid, revoked, device-limit, clock, and service states
- Retry verification, Manage devices, Release this PC, and Done actions only when relevant
- A note that the key is protected for the current Windows user and no Hum account is required
- Accessible focus, keyboard, busy, live-status, error, and reduced-motion behavior

## Required red-first tests

- Every license status maps to the correct overlay-versus-license-window action.
- Development, verified, verification due, and offline grace are the only licensed window states.
- License key input rejects blank, control-character, and oversized values without returning the key in an error.
- Checkout and portal helpers accept only HTTPS Polar hosts and reject credentials, fragments, other schemes, lookalike hosts, and missing configuration.
- Command errors and events contain no full key or activation ID.
- Activation success chooses the overlay; invalid, revoked, device-limit, and service failures choose the license window.
- Deactivation success chooses the license window and cannot delete before remote success, retaining the HUM-10B ordering test.
- A release bootstrap failure still chooses the license window instead of leaving both windows hidden.
- Frontend typecheck and production build include the new route and styles.

## Acceptance checks

- HUM-10-AC1 can be checked locally only after activation UI and release gating pass.
- HUM-10-AC2 can be checked locally for re-entry and restore behavior, with live provider proof still pending.
- HUM-10-AC3 can be checked locally after every offline and recovery state is visible and actionable.
- A real Polar checkout, receipt key, three-device limit, customer portal release, refund revocation, and reinstall remain physical HUM-10H proof items.
- HUM-10C is committed and pushed before HUM-10D begins.

## Amendment gate

Stop for approval if the slice needs a custom license server, provider access token, user account, hardware fingerprint, new package dependency, more than eighteen production files, or a new window system beyond the predeclared Tauri license window.

## Completion record

- Version: 0.13.64
- Commit: closeout commit titled `Add the Hum activation experience`
- Validation: frozen install, frontend typecheck and production build, 234 Rust tests, debug and release all-target checks, debug and release Clippy with warnings denied, full Rust formatting, and diff validation
- Native QA: release startup entitlement gate, default and minimum window fit, keyboard activation submission, reveal and hide, close and reopen, safe missing-link recovery, and key redaction
- Review repairs: deactivation final-state race, failed-release feedback, action-scoped accessibility errors, focused-field styling, minimum-width layout, and context-aware recovery actions
- Deferred proof: live Polar checkout, receipt key, device-limit portal workflow, refund revocation, reinstall, and scaling matrix remain in HUM-10G and HUM-10H
