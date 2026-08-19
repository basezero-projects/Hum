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

The first manual proof, [run 32302525832](https://github.com/basezero-projects/Hum/actions/runs/32302525832) at commit `31265b97f5cbbee2b9c33986135047985e0bd8be`, passed every test, version check, Azure setup step, and updater key match. Packaging then failed because the workflow copied `signtool.exe` away from the Windows SDK libraries it needs. The next proof, [run 32305284213](https://github.com/basezero-projects/Hum/actions/runs/32305284213) at commit `7fdc53413fd37521ba7439e348ad9c186ecb6bef`, proved the original SDK tool path and successfully signed the first target. It then exposed the `dump_uia` developer inspector inside Tauri's release binary set. The third proof, [run 32308223373](https://github.com/basezero-projects/Hum/actions/runs/32308223373) at commit `953d9cd1365803d2968f4ba387f47e4f687536d6`, passed the complete gate, built and signed Hum plus its installer, then stopped because PowerShell parsed `$target:` as an invalid variable in the separate Authenticode verifier. The v0.13.70 repair uses an explicit variable boundary and tests that exact workflow string. Its signed proof and artifact audit are still required before this slice closes.

The destructive customer update test remains in HUM-10H. That later proof will install a previous signed Hum build, update it through the public feed, confirm relaunch, withdraw the feed, and exercise manual rollback.
