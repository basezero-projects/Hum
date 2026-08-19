# HUM-10B protected storage and Polar client plan

Date: 2026-08-19
Status: Complete in v0.13.63
Related phase: [HUM-10](../../roadmap/1.0/HUM-10-purchase-trust-first-run.md)
Progress ledger: [HUM-10 ledger](../../roadmap/1.0/HUM-10-progress-ledger.md)

## Outcome

Hum can keep a license record outside `settings.json`, protect it for the current Windows user, activate and validate through Polar's public desktop endpoints, release a device activation, and evaluate the safe state from HUM-10A.

This slice adds the backend and startup lifecycle. It does not add an activation screen or block the overlay. HUM-10C will make these operations visible and enforce the release entitlement.

## Planned production files

- Add `src-tauri/src/license/record.rs`
- Add `src-tauri/src/license/store.rs`
- Add `src-tauri/src/license/provider.rs`
- Add `src-tauri/src/license/polar.rs`
- Add `src-tauri/src/license/service.rs`
- Modify `src-tauri/src/license/mod.rs`
- Add `src-tauri/src/platform/windows/license_store.rs`
- Modify `src-tauri/src/platform/windows/mod.rs`
- Modify `src-tauri/src/lib.rs`
- Modify `src-tauri/Cargo.toml`
- Modify `src-tauri/Cargo.lock` only if feature resolution changes it

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, `BUGS.md` when needed, this plan, the HUM-10 roadmap and ledger, plus Hum brain records.

## Protected record

The plaintext record exists only in process memory before Windows protects it. It contains:

- `format_version`, initially 1
- `license_key`
- `activation_id`
- `key_suffix`
- `provider_status`, either `granted` or `revoked`
- `verified_at_unix_ms`
- `last_seen_unix_ms`

The type uses a custom redacted `Debug` implementation. Validation rejects an unknown version, blank secret fields, a key suffix longer than eight safe characters, negative timestamps, and a `last_seen` time earlier than the verification time.

## Storage contract

- `LicenseStore` exposes `load`, `save`, and `delete`.
- Missing storage returns `Ok(None)`.
- Corrupt, modified, wrong-user, and unsupported records return typed errors without file paths or secret material.
- Windows stores `license.bin` in Tauri's app data directory.
- DPAPI uses current-user protection, UI forbidden, and fixed Hum entropy.
- Writes go to a sibling temporary file, flush to disk, then replace the destination with `MoveFileExW` using replace and write-through flags.
- Failed replacement removes the temporary file and keeps the prior record.
- Deletion treats a missing file as success.

No provider token, private key, MachineGuid, MAC address, serial number, or hostname is stored or sent.

## Polar client contract

Base URL: `https://api.polar.sh/v1/customer-portal/license-keys`

Endpoints:

- `POST /activate`
- `POST /validate`
- `POST /deactivate`

Every request contains the public Polar organization ID. Activation and validation send `conditions.major_version = 1`. Activation uses a generic `Windows PC` label plus a six-character random identifier. Validation includes the stored activation ID. Deactivation includes the key and activation ID.

The client sends no Authorization header. It uses a ten-second request timeout, accepts at most 64 KiB of response data, and never returns or logs a raw provider body.

Provider results map to:

- `Granted`, with activation ID and safe key suffix
- `Invalid`
- `Revoked`
- `DeviceLimit`
- `ServiceUnavailable`

Polar status `granted` is accepted only for the configured organization, Hum major version condition, and three-device activation limit. `revoked` and `disabled` both map to `Revoked`. Network failures, HTTP 429, and server errors map to `ServiceUnavailable`. Activation-limit errors map to `DeviceLimit`. Other 4xx activation failures map to `Invalid`.

## Service behavior

- Debug builds use the development entitlement and do not read storage or contact Polar.
- A Windows release bootstrap loads the protected record, evaluates clock safety, updates `last_seen_unix_ms`, and saves it before publishing state.
- A license inside its normal verified window makes no network request.
- A due, grace, or revoked record validates on startup.
- A successful activation saves the record before publishing `verified`.
- If saving a new activation fails, Hum attempts to release the remote activation and returns a storage error.
- A successful validation updates the protected record and restarts the verification window.
- A service failure preserves the record and enters the correct verified, due, or grace state.
- A revoked validation persists the revoked marker.
- A successful deactivation releases Polar first, then deletes the local record and publishes `unlicensed`.
- A failed remote deactivation keeps the local record so the customer can retry.

The service keeps one safe `LicenseState` behind an async read-write lock. No frontend command is registered in this slice.

## Required red-first tests

- Record v1 round-trips and rejects every invalid invariant.
- Record `Debug` and all store errors redact the full key and activation ID.
- Store missing, save, replace, delete, corrupt, tampered, and wrong-entropy paths are distinct.
- Windows DPAPI round-trips bytes for the current user and rejects modified ciphertext.
- Polar request paths, JSON bodies, organization ID, major-version condition, activation label, and lack of auth header are exact.
- Polar granted, revoked, disabled, invalid, device-limit, rate-limit, server-error, oversized, malformed, and network-error results map correctly.
- Provider errors never include response bodies or key material.
- Development bootstrap touches neither store nor provider.
- Verified bootstrap updates `last_seen` without a network call.
- Due bootstrap validates and saves granted state.
- Grace bootstrap preserves access when the service is unavailable.
- Revoked bootstrap stays revoked until Polar grants it again.
- Activation save failure attempts remote rollback.
- Remote deactivation failure preserves local state and storage.
- Successful deactivation deletes local state only after Polar succeeds.

## Acceptance checks

- License material is never written to `settings.json` or any plain-text file.
- Shared license modules import no Windows types.
- Polar calls use only public customer endpoints and ship no provider secret.
- Non-Windows all-target checks still compile.
- No package dependency is added.
- HUM-10 phase acceptance remains open until UI and live provider proof exist.
- HUM-10B is committed and pushed before HUM-10C begins.

## Amendment gate

Stop for approval if implementation needs a custom license server, provider access token, user account, hardware fingerprint, frontend file, overlay gate, new package dependency, or more than twenty-two production files.

## Closeout

- Version: 0.13.63
- Validation: frozen install, frontend typecheck and build, 229 Rust tests, debug and release all-target checks, Clippy with warnings denied in both profiles, full Rust formatting, and diff validation
- Review repairs: redacted provider activation IDs, remote rollback for malformed grants, serialized concurrent operations, safe startup without provider configuration, and zeroed plaintext serialization buffers after use
- Production boundary: eleven planned production files, no new package dependency, no frontend file, and no overlay gate
- Deferred: activation UI, checkout handoff, entitlement enforcement, and live Polar organization and product proof remain in HUM-10C onward
