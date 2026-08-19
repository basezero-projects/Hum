# HUM-10: Purchase, trust, and first run

Status: In progress
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

## License and purchase policy

- Hum costs $19 once. There is no subscription.
- Polar is the hosted checkout, Merchant of Record, license-key issuer, and customer portal.
- One license covers three Windows devices owned or controlled by the buyer. Customers can free an activation from Polar's portal without contacting support.
- The purchase includes Hum 1.x updates. A future major version may have a separate upgrade price, but the purchased 1.x build remains usable.
- Full refunds are available for 30 days. A full refund revokes the license benefit.
- Hum validates an active license every 30 days when a connection is available. A failed network check starts another 30 days of full offline use. The app warns before that grace period ends.
- License keys, activation IDs, and verification timestamps live in Windows-protected storage. They never enter the settings file, URLs, logs, or diagnostics.
- Release builds require a license. Development builds use an explicit development entitlement so normal app work does not consume a customer activation.

The durable reasoning is recorded in [ADR-0002](../../decisions/ADR-0002-use-polar-and-protected-offline-license-state.md).

## Acceptance criteria

- [x] HUM-10-AC1: A purchase can activate Hum through a clear in-app flow.
- [x] HUM-10-AC2: A valid customer can restore a license after reinstalling or changing computers within the license policy.
- [x] HUM-10-AC3: An offline customer sees their verification state and a useful recovery message before access changes.
- [x] HUM-10-AC4: A clean install guides the user through overlay placement, listening mode, appearance, and core controls.
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

- HUM-10A: license policy and provider-neutral entitlement state engine, complete in v0.13.62
- HUM-10B: Windows-protected license storage and Polar activation client, complete in v0.13.63
- HUM-10C: activation, restore, deactivation, and checkout handoff UI, complete in v0.13.64
- HUM-10D: first-run setup for placement, listening output, appearance, and controls, complete in v0.13.65
- HUM-10E: signed installer, signed updater, and complete update-state UI
- HUM-10F: About, support, privacy, diagnostics, and production menu cleanup
- HUM-10G: paid-product promo defaults and purchase-site checkout completion
- HUM-10H: clean-install, recovery, scaling, and prior-version update proof

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
