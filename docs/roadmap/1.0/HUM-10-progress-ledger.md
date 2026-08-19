# HUM-10 progress ledger

This file is the durable execution cursor for HUM-10. Update it at every slice boundary before starting the next slice.

## Current cursor

- Slice: HUM-10B, protected storage and Polar client
- Step: Survey and plan lock
- Next action: map Windows DPAPI storage, Polar client seams, startup ownership, and failure behavior
- Blocker: None for implementation. Live production verification still requires a Polar organization and Hum product.
- Last updated: 2026-08-19 12:32 MDT

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

## Execution log

- 2026-08-19 12:05 MDT: HUM-10 started. The purchase and license policy was locked, ADR-0002 was accepted, and HUM-10A entered implementation.
- 2026-08-19 12:32 MDT: HUM-10A passed 202 Rust tests and the full gate after a red-first countdown correction. The slice closed in v0.13.62 and the cursor advanced to HUM-10B survey.
