# HUM-10: Purchase, trust, and first run

Status: Proposed
Target release: 0.15.x
Depends on: HUM-00
Blocks: HUM-70
Last reviewed: 2026-08-19

## Outcome

A customer can buy Hum, activate it, understand the overlay, receive signed updates, and find help without opening developer tools.

## Why this phase exists

Hum currently has no license flow or onboarding. Update signing is incomplete, failures are silent, and production menus still expose development controls. A paid app needs a trustworthy first hour before feature depth matters.

## In scope

- License architecture and entitlement decision
- Checkout handoff, activation, restore, offline verification, and recovery
- Signed Windows installer and signed updater artifacts
- First-run setup for placement, listening output, appearance, and shortcuts
- About, support, privacy, diagnostics, and update status surfaces
- Production menu cleanup
- Paid-product promotional policy

## Out of scope

- User accounts for normal use
- Subscription billing
- Cloud settings synchronization
- Team or business license administration
- macOS and Linux licensing implementations

## Architecture constraints

- License verification belongs in platform-neutral Rust.
- Machine identity and secure storage stay behind platform adapters.
- A valid one-time entitlement does not expire. Only its offline verification window can be exhausted.
- Hum must remain usable offline within the accepted license policy.
- No secret or customer data belongs in logs or plain-text settings.
- The updater must reject unsigned artifacts.

## Acceptance criteria

- [ ] HUM-10-AC1: A purchase can activate Hum through a clear in-app flow.
- [ ] HUM-10-AC2: A valid customer can restore a license after reinstalling or changing computers within the license policy.
- [ ] HUM-10-AC3: An offline customer sees their verification state and a useful recovery message before access changes.
- [ ] HUM-10-AC4: A clean install guides the user through overlay placement, listening mode, appearance, and core controls.
- [ ] HUM-10-AC5: The installer and update artifacts have valid signatures, and an update is tested from the previous release.
- [ ] HUM-10-AC6: Manual update checks report current, available, downloading, installing, and failed states.
- [ ] HUM-10-AC7: Production builds hide the dev console and contain no demo-only update path.
- [ ] HUM-10-AC8: About, support, privacy, and diagnostics are reachable from Settings or the tray.
- [ ] HUM-10-AC9: Promotional cards are off by default for paid customers.

## Required test matrix

- Clean install, upgrade, uninstall, and reinstall
- Valid, invalid, revoked, offline-window-exhausted, service-unavailable, and restored license states
- Checkout canceled, activation interrupted, and service unavailable
- Update available, current, signature failure, download failure, and successful relaunch
- Fresh user at 100, 125, 150, and 200 percent Windows scaling

## Slice map

No implementation plan written yet.

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
