# HUM-10A license policy and state plan

Date: 2026-08-19
Status: Complete in v0.13.62
Related phase: [HUM-10](../../roadmap/1.0/HUM-10-purchase-trust-first-run.md)
Progress ledger: [HUM-10 ledger](../../roadmap/1.0/HUM-10-progress-ledger.md)

## Outcome

Hum has one provider-neutral Rust contract for its $19 perpetual license, three-device allowance, Hum 1.x entitlement, verification cadence, offline grace, clock safety, and every status the later activation UI must explain.

This slice does not contact Polar, store a license, or block the overlay. It defines and proves the decisions those later slices will use.

## Planned production files

- Add `src-tauri/src/license/mod.rs`
- Add `src-tauri/src/license/model.rs`
- Add `src-tauri/src/license/policy.rs`
- Add `src-tauri/src/license/evaluate.rs`
- Modify `src-tauri/src/lib.rs`

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, `BUGS.md` when needed, this plan, the HUM-10 roadmap and ledger, plus Hum brain records.

## Locked wire contract

`LicenseStatus` serializes as:

- `development`
- `unlicensed`
- `verified`
- `verification_due`
- `offline_grace`
- `verification_required`
- `invalid`
- `revoked`
- `device_limit`
- `clock_error`
- `service_unavailable`

`LicenseState` contains only safe UI data:

- `status`
- `licensed`
- `display_key`, an optional provider-redacted suffix
- `device_limit`
- `verified_at_unix_ms`
- `verify_after_unix_ms`
- `grace_ends_unix_ms`
- `days_until_action`
- `message`
- `recovery`

The full license key, activation ID, customer data, and provider response never enter this serialized state.

## Policy constants

- Product major version: 1
- Device limit: 3
- Verification interval: 30 days
- Offline grace after the verification deadline: 30 days
- Warning starts: 7 days before the verification deadline
- Clock rollback tolerance: 5 minutes
- Refund window: 30 days

## Evaluation rules

1. Development entitlement is explicit and always reports `development`.
2. No protected record reports `unlicensed`.
3. Invalid, revoked, and device-limit provider outcomes keep their distinct terminal status.
4. A clock earlier than the last observed time by more than five minutes reports `clock_error`.
5. A valid record before its warning window reports `verified`.
6. A valid record within seven days of the verification deadline reports `verification_due`.
7. A failed network or provider check after the deadline but before grace ends reports `offline_grace` and remains licensed.
8. A record beyond grace reports `verification_required` and is not licensed until an online check succeeds.
9. A service failure without a prior valid record reports `service_unavailable` and is not licensed.
10. Day counts round up so the UI does not report zero days while time remains.

## Required red-first tests

- [x] Every status preserves its exact lowercase wire value.
- [x] The safe state payload contains no secret fields or full key material.
- [x] Policy constants match the accepted product policy.
- [x] Development, unlicensed, verified, warning, grace, exhausted, invalid, revoked, device-limit, clock-error, and first-use service failure paths are distinct.
- [x] Exact boundary instants at warning start, verification deadline, and grace end choose the documented state.
- [x] A five-minute backward jump is tolerated, while a larger rollback reports `clock_error`.
- [x] Licensed is true only for development, verified, verification due, and offline grace.
- [x] Remaining-day calculations round up and never become negative.

## Acceptance checks

- [x] The shared module imports no Windows or Polar types.
- [x] The UI contract exposes no full license key, activation ID, email, order ID, or provider payload.
- [x] Later storage and provider clients can map into the evaluator without changing its policy.
- [x] HUM-10 remains in progress. No phase acceptance criterion is claimed by this foundation alone.
- [x] HUM-10A is committed and pushed before HUM-10B begins.

## Amendment gate

Stop for approval if this slice needs network calls, secure storage, frontend UI, overlay gating, checkout changes, a new dependency, or more than ten production files.
