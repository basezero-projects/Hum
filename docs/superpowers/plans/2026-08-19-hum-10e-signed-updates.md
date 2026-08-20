# HUM-10E signed updates plan

Date: 2026-08-19
Status: Implementation complete, signed proof pending
Target version: 0.13.73 repair candidate, final closeout follows signed proof
Related phase: [HUM-10](../../roadmap/1.0/HUM-10-purchase-trust-first-run.md)
Progress ledger: [HUM-10 ledger](../../roadmap/1.0/HUM-10-progress-ledger.md)

## Outcome

Hum can build an Azure Trusted Signing Windows installer and its Tauri updater signature from one release workflow. A manual update check always tells the customer whether Hum is checking, current, ready to update, downloading, installing, restarting, or needs a retry.

The workflow supports a manual nonpublishing proof run. Only a matching `vX.Y.Z` tag can publish a GitHub Release and replace `latest.json`. This prevents ordinary roadmap pushes from becoming customer releases.

## Existing failures this slice removes

- `plugins.updater.pubkey` is empty, so no updater artifact can pass production signature verification.
- No release workflow builds, signs, verifies, or publishes the Windows installer and update metadata.
- Manual checks silently return to idle when Hum is current or the endpoint fails.
- The overlay contains a demo-only fake update lifecycle when no real updater resource exists.
- The tray can show only `Check for updates` or `Install update`, not active or failed states.

## Production file boundary

- Add `.github/workflows/release.yml`.
- Add `scripts/prepare-release.mjs` and `scripts/prepare-release.test.mjs`.
- Add `src/update-state.ts` and `src/update-state.test.mts`.
- Add `src-tauri/src/update_status.rs`.
- Modify `src/Overlay.tsx`.
- Modify `src-tauri/src/lib.rs`.
- Modify `src-tauri/tauri.conf.json`.
- Modify `package.json`.
- Add `docs/verification/hum-signed-updates.md`.

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, this plan, the HUM-10 roadmap and ledger, `BUGS.md` when needed, plus Hum brain records.

External setup is part of this slice, but secrets never enter the repository:

- Generate a Hum-specific encrypted Tauri updater keypair.
- Store the private key and password in Arcanum under project `Hum` and as GitHub Actions secrets for `basezero-projects/Hum`.
- Reuse the existing SYVR Azure Trusted Signing account, certificate profile, and service principal by copying their protected values into Hum-scoped Arcanum and repository secrets.
- Commit only the Tauri updater public key.

## Signed release contract

- The workflow runs on `windows-2022` for manual dispatch or a `v*` tag.
- It performs the frozen frontend install, frontend tests, typecheck, production build, full Rust tests, all-target checks, Clippy with warnings denied, and Rust formatting before packaging.
- It verifies that package, Cargo, lockfile, and Tauri versions agree. A tag build must match `vX.Y.Z` exactly.
- Azure Trusted Signing runs inside Tauri's Windows `signCommand`, before updater signatures are created. The final NSIS installer and bundled executable therefore carry Authenticode signatures before Tauri signs the updater bytes.
- A key probe refuses to build when the private updater key does not match the public key compiled into Hum.
- The workflow verifies a valid Authenticode signature and produces one checked `latest.json` entry for `windows-x86_64`.
- Every successful run uploads a private Actions artifact containing the installer, its updater signature, metadata, and proof record.
- Only a matching version tag publishes those files to GitHub Releases. Manual proof runs never modify the public updater feed.

## Customer update flow

- Automatic checks remain quiet when Hum is current or the network is unavailable.
- A tray check shows `Checking for updates...`, then one clear outcome.
- Current state says `Hum is up to date`, then returns to the normal tray action.
- Available state names the version and lets the customer start the update from either the tray or overlay notice.
- Downloading state includes percentage when the server supplies a content length.
- Installing and restarting are separate visible states.
- Check, download, install, and restart failures use safe stage-specific copy and an explicit retry. Raw provider or filesystem errors are not shown to the customer.
- Superseded updater resources are closed, and checks or installs cannot overlap.
- The fake no-resource update branch is removed.

## Required red-first tests

- Every update state produces exact banner copy, tray copy, actionability, and progress clamping.
- Automatic current and check-failure outcomes remain quiet, while manual outcomes are visible.
- A missing updater resource can never enter downloading or installing.
- The release-preparation script rejects version drift, missing or extra installers, missing signatures, malformed signatures, and tag mismatch.
- Valid fixture input produces the exact Windows platform metadata and download URL.
- Rust tray projection sanitizes hostile or oversized version text and disables nonactionable phases.
- The release workflow contract contains both signing layers, runs validation before packaging, keeps manual proof runs private, and limits publishing to matching version tags.

## Proof split

HUM-10E closes the signed build path, customer update-state behavior, updater public key, and a real nonpublishing signed CI artifact. HUM-10H owns the destructive release-path proof from a previously installed signed Hum build, the actual public tag, successful relaunch, feed withdrawal, and manual rollback.

## Amendment gate

Stop for approval if this slice needs a new package dependency, a new signing provider, a second update endpoint, a server component, a different installer technology, publishing an untagged release, more than twenty-two production files, or any change to the license and first-run policies.

## Closeout

Before HUM-10F starts: complete the red-first tests, full local gate, independent cold review, manual nonpublishing signed workflow run, signed artifact audit, version bump, changelog, BUGS review, ledger and brain writes, commit, push, and exact-commit verification.
