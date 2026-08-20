# Hum signed update verification

Last updated: 2026-08-19

## What this proves

Hum uses two separate trust layers for Windows releases:

1. Azure Trusted Signing applies an Authenticode signature to the Hum executable and NSIS installer during the Tauri build.
2. Hum's private Tauri updater key signs the completed NSIS installer after its Windows binaries are signed. The matching public key is compiled into Hum.

The private updater key, its password, and the Azure credentials live in Arcanum under the `Hum` project and in encrypted GitHub repository secrets. They are not stored in the repository. The temporary local key files used during setup were removed after the generated private key matched the committed public key.

## Workflow safety

The `Signed Windows release` workflow accepts a manual proof run or an exact `vX.Y.Z` tag.

- Every run performs the frontend tests, typecheck, production build, Rust formatting, all-target check, Clippy with warnings denied, and all Rust tests before packaging.
- The package, Cargo, lockfile, and Tauri versions must match. A tag must match that version exactly.
- The workflow signs the Windows executable and installer, verifies their Authenticode status, checks that the private updater key matches Hum's public key, and validates the updater signature and release metadata.
- Every successful run uploads a private Actions artifact with the installer, its updater signature, `latest.json`, and a SHA-256 proof record.
- Only a matching version tag can publish a GitHub Release. A manual proof run cannot modify the public updater feed.

## Current proof cursor

The first manual proof, [run 32302525832](https://github.com/basezero-projects/Hum/actions/runs/32302525832) at commit `31265b97f5cbbee2b9c33986135047985e0bd8be`, passed every test, version check, Azure setup step, and updater key match. Packaging then failed because the workflow copied `signtool.exe` away from the Windows SDK libraries it needs. The next proof, [run 32305284213](https://github.com/basezero-projects/Hum/actions/runs/32305284213) at commit `7fdc53413fd37521ba7439e348ad9c186ecb6bef`, proved the original SDK tool path and successfully signed the first target. It then exposed the `dump_uia` developer inspector inside Tauri's release binary set. The third proof, [run 32308223373](https://github.com/basezero-projects/Hum/actions/runs/32308223373) at commit `953d9cd1365803d2968f4ba387f47e4f687536d6`, passed the complete gate, built and signed Hum plus its installer, then stopped because PowerShell parsed `$target:` as an invalid variable in the separate Authenticode verifier. The fourth proof, [run 32309946659](https://github.com/basezero-projects/Hum/actions/runs/32309946659) at commit `471176454ef462831b422a783cfb8c67ba3e38d0`, reached the repaired verifier after Azure reported successful signatures for both targets. PowerShell still classified `hum.exe` as `NotSigned`. The fifth proof, [run 32311966274](https://github.com/basezero-projects/Hum/actions/runs/32311966274) at commit `001330fb9d28fa5078d36a091534326c93446998`, used the Windows SDK SignTool and confirmed that the raw build output has no signature. Tauri's bundler signs the patched executable, packages that signed file into NSIS, then restores its unsigned development copy after packaging. [Run 32313920083](https://github.com/basezero-projects/Hum/actions/runs/32313920083) at commit `a15557248bd8edcaee0398efecfc8c9def589eb6` passed the full gate, signed the build, extracted the completed installer, and verified both the installed `hum.exe` and the installer with SignTool. Metadata preparation then exposed one stale assumption: current Tauri emits the signed installer and an `.exe.sig` updater signature, not a separate `.nsis.zip` archive. That failure led to the v0.13.73 contract for Tauri's real file pair.

The clean proof is [run 32315736382](https://github.com/basezero-projects/Hum/actions/runs/32315736382) at commit `8c503d29a17b9a05f38399329359e31d94d65f1e`. It passed all 18 JavaScript tests, 247 Rust tests, frontend checks, formatting, all-target checking, Clippy, updater-key matching, Azure signing, NSIS extraction, and SignTool verification for the installed `hum.exe` plus installer. Metadata preparation succeeded, and private artifact `9388432007` uploaded exactly four files: `Hum_0.13.73_x64-setup.exe`, its `.exe.sig`, `latest.json`, and `release-proof.json`. The installer SHA-256 is `a6d58d4ddd5d389fae2b62abb417003db6fe1268b13c9a67df51f9bd1f328fec`. The downloaded feed and proof records matched that file, the signature sidecar, version `0.13.73`, the sole `windows-x86_64` platform, and the versioned GitHub Release URL. Local Windows SignTool also verified the downloaded installer and its extracted `hum.exe`. Because this was a manual run, the publish step stayed disabled and no public release changed.

The destructive customer update test remains in HUM-10H. That later proof will install a previous signed Hum build, update it through the public feed, confirm relaunch, withdraw the feed, and exercise manual rollback.
