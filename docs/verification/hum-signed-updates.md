# Hum signed update verification

Last updated: 2026-08-19

## What this proves

Hum uses two separate trust layers for Windows releases:

1. Azure Trusted Signing applies an Authenticode signature to the Hum executable and NSIS installer during the Tauri build.
2. Hum's private Tauri updater key signs the updater archive after the Windows binaries are signed. The matching public key is compiled into Hum.

The private updater key, its password, and the Azure credentials live in Arcanum under the `Hum` project and in encrypted GitHub repository secrets. They are not stored in the repository. The temporary local key files used during setup were removed after the generated private key matched the committed public key.

## Workflow safety

The `Signed Windows release` workflow accepts a manual proof run or an exact `vX.Y.Z` tag.

- Every run performs the frontend tests, typecheck, production build, Rust formatting, all-target check, Clippy with warnings denied, and all Rust tests before packaging.
- The package, Cargo, lockfile, and Tauri versions must match. A tag must match that version exactly.
- The workflow signs the Windows executable and installer, verifies their Authenticode status, checks that the private updater key matches Hum's public key, and validates the updater signature and release metadata.
- Every successful run uploads a private Actions artifact with the installer, updater archive, updater signature, `latest.json`, and a SHA-256 proof record.
- Only a matching version tag can publish a GitHub Release. A manual proof run cannot modify the public updater feed.

## Current proof cursor

The v0.13.67 candidate passed the local contract tests and is ready for the first manual nonpublishing workflow run. Record the exact workflow run, commit, Authenticode result, updater key match, artifact filenames, and hashes here after GitHub completes the proof.

The destructive customer update test remains in HUM-10H. That later proof will install a previous signed Hum build, update it through the public feed, confirm relaunch, withdraw the feed, and exercise manual rollback.
