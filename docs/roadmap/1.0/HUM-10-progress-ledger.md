# HUM-10 progress ledger

This file is the durable execution cursor for HUM-10. Update it at every slice boundary before starting the next slice.

## Current cursor

- Slice: HUM-10C, activation and restore experience
- Step: Survey and plan lock
- Next action: map the release entitlement gate, activation window, checkout handoff, restore, deactivation, and frontend command contracts
- Blocker: None for local implementation. Live provider proof still requires a Polar organization and Hum product.
- Last updated: 2026-08-19 13:00 MDT

## Locked product policy

- Price: $19 one time
- Provider: Polar hosted checkout and license keys
- Devices: three active Windows devices per personal license
- Refunds: 30 days for a full refund
- Updates: Hum 1.x included
- Verification: every 30 days when online, followed by 30 days of offline grace after a service or network failure
- Storage: Windows DPAPI-protected record outside settings
- Accounts: none required for normal app use

## Slice sequence

1. HUM-10A, license policy and entitlement state
2. HUM-10B, protected storage and Polar client
3. HUM-10C, activation and restore experience
4. HUM-10D, first-run setup
5. HUM-10E, signed updates and update states
6. HUM-10F, trust and support surfaces
7. HUM-10G, paid-product defaults and checkout completion
8. HUM-10H, release-path proof

## Completed slices

### HUM-10A, license policy and entitlement state

- Status: Complete
- Version: 0.13.62
- Commit: closeout commit titled `Define the Hum license policy`
- Remote: pushed to `origin/main`
- Validation: frontend frozen install, typecheck, and build passed; 202 Rust tests passed; Cargo all-target check passed; Clippy passed with warnings denied; full-tree Rust formatting passed; diff check passed
- Review: the changed-file audit found an overdue verification countdown pointing at the past deadline; a failing boundary test proved it, the countdown was corrected to the grace deadline, and the full gate passed again
- Acceptance criteria: no HUM-10 phase criterion claimed by this foundation alone
- Known deferrals: protected storage, Polar network calls, activation UI, overlay gating, and production provider setup remain in HUM-10B onward

### HUM-10B, protected storage and Polar client

- Status: Complete
- Version: 0.13.63
- Commit: closeout commit titled `Protect Hum license activations`
- Remote: push to `origin/main` during closeout
- Validation: frozen install, frontend typecheck and build, 229 Rust tests, debug and release all-target checks, Clippy with warnings denied in both profiles, full Rust formatting, and diff validation
- Review: red-first repairs covered activation ID redaction, malformed grant rollback, concurrent operation serialization, release startup without provider configuration, and release-only ownership compilation
- Acceptance criteria: the protected backend supports HUM-10-AC1 through AC3, but none are checked until HUM-10C exposes and enforces the customer workflow
- Known deferrals: activation UI, checkout, restore presentation, entitlement enforcement, and live Polar proof remain in HUM-10C onward

## Execution log

- 2026-08-19 12:05 MDT: HUM-10 started. The purchase and license policy was locked, ADR-0002 was accepted, and HUM-10A entered implementation.
- 2026-08-19 12:32 MDT: HUM-10A passed 202 Rust tests and the full gate after a red-first countdown correction. The slice closed in v0.13.62 and the cursor advanced to HUM-10B survey.
- 2026-08-19 12:44 MDT: HUM-10B locked an eleven-file production boundary for the versioned secret record, storage and provider interfaces, Polar client, license service, Windows DPAPI adapter, startup ownership, and existing Windows API feature additions.
- 2026-08-19 13:00 MDT: HUM-10B passed 229 Rust tests and the complete debug and release gate after four red-first review repairs. The slice closed in v0.13.63 and the cursor advanced to HUM-10C survey.
