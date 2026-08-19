# HUM-00G build proof and system documentation plan

Date: 2026-08-19
Status: Complete in v0.13.61
Related phase: [HUM-00](../../roadmap/1.0/HUM-00-portable-product-core.md)
Progress ledger: [HUM-00 ledger](../../roadmap/1.0/HUM-00-progress-ledger.md)

## Outcome

Hum's shared product core compiles and passes its non-GUI startup smoke on native Windows, macOS, and Linux runners. The implemented media, timing, audio-output, native-window, platform-capability, and OBS flows are documented with honest automated and physical-test evidence.

## Planned production and test files

- Add `.github/workflows/portable-core.yml`
- Modify `src-tauri/src/lib.rs`
- Add `docs/systems/media-and-timing.md`
- Modify `docs/systems/INDEX.md`
- Add `docs/verification/hum-00-windows-regression.md`
- Modify `docs/verification/1.0-release-checklist.md` only to link remaining physical checks if needed
- Modify `README.md`

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, `BUGS.md` when needed, this plan, the HUM-00 roadmap and ledger, plus Hum brain records.

## Locked proof boundary

- CI compiles and tests the shared shell. It does not package or publish macOS or Linux builds.
- The smoke test initializes the same neutral state constructor used by production.
- CI does not pretend to launch a transparent desktop window on a headless runner.
- Automated contract evidence and physical Windows player evidence remain separate.
- GitHub CI is not proof that Spotify, iTunes, Pandora, OBS, tray interaction, or Ghost mode were physically exercised.
- macOS/Linux installers, signing, updater artifacts, MPRIS, player adapters, Playwright, and visual regression are outside HUM-00G.

## Shared-shell smoke

Extract the existing neutral state construction in `run` into `SharedShellState::new()` without changing values or startup order.

The production-backed smoke test asserts:

- The current track is the default track.
- Album artwork is absent.
- Lyrics state is default.
- Overlay mode is Edit.
- SMTC active state is false.
- Audio-output inventory is empty and has no active output.

## Workflow contract

Create one workflow for push, pull request, and manual dispatch. It uploads no artifacts and performs no packaging.

Frontend on Ubuntu 24.04:

- pnpm 10.22.0
- Node 22 with pnpm cache
- frozen dependency install
- `pnpm typecheck`
- `pnpm build`
- `node --test src/platform-info-retry.test.mjs`

Windows on Windows Server 2022:

- Rust 1.93.1 with rustfmt and Clippy
- full Rust formatting check
- all-target Cargo check
- Clippy with warnings denied
- all-target Rust tests
- named shared-shell smoke test

Portable native matrix on Ubuntu 24.04 and macOS 15:

- fail-fast disabled
- all-target Cargo check
- library tests
- named shared-shell smoke test
- Linux installs the official Tauri WebKitGTK, app-indicator, SVG, SSL, build, and utility packages before Cargo runs

## Windows regression evidence

Document two evidence tiers.

Automated coverage names the exact tests and CI jobs for source authority, event order and payloads, shared media wire values, audio-output transitions and lifecycle, timing equations, OBS projection, browser and Pandora rules, native window behavior, platform capabilities, updater recovery, TypeScript, and production rendering compilation.

The physical matrix lists Spotify desktop, Chromium, iTunes, Pandora web, Pandora desktop, pause, seek, track change, ad, no session, Ribbon, Square, OBS, Edit, Locked, and Ghost. Items remain clearly pending unless they were actually exercised. Remaining physical work stays linked to the 1.0 release checklist.

## System documentation

`docs/systems/media-and-timing.md` documents:

- Purpose, scope, and key files
- Startup and ownership sequence
- Raw and effective track snapshots
- Windows media authority and iTunes suppression
- Browser and Pandora enrichment
- Exact media and artwork event contracts
- Lyrics provider and cache flow
- Saved and temporary timing equations
- Audio-output devices, commands, events, polling, deduplication, failures, and profile isolation
- Native window interfaces and PlatformInfo gating
- Windows, macOS, and Linux support truth
- OBS state, settings, artwork, SSE, and polling flow
- Retry, cache, shutdown, and known limitation behavior
- Verification links

Update the systems index and README so they describe the current v0.13.61 architecture and development checks.

## Implementation steps

1. Add the shared-shell smoke test first and confirm it fails because the production constructor does not exist.
2. Extract the existing neutral state constructor and make production `run` consume it.
3. Run the focused smoke and full local gate.
4. Add the native workflow with no packaging or artifacts.
5. Add the system document, index entry, regression evidence, release-checklist links if needed, and current README architecture.
6. Run the full local gate again and independently review code, workflow, evidence claims, platform commands, and documentation accuracy.
7. Audit every changed file, bump the final repair to v0.13.61 across all manifests, update changelog and roadmap records, log out-of-scope findings, update the ledger and Hum brain records, commit, and push.
8. Wait for the exact pushed commit's frontend, Windows, macOS, and Linux jobs.
9. If a job fails, diagnose it, add a red-first local or workflow proof where possible, fix it, restart the complete gate, version, commit, push, and wait again.
10. HUM-00 is complete only after the exact final commit is green on every native job.

## Required red-first tests and checks

- [x] The shared-shell smoke fails before `SharedShellState::new()` exists.
- [x] Production `run` consumes the constructor exercised by the smoke.
- [x] The smoke locks all six neutral defaults.
- [x] Frontend frozen install, typecheck, build, and retry test run on Ubuntu.
- [x] Windows runs formatting, all-target check, Clippy, all tests, and the named smoke.
- [x] macOS and Linux run all-target check, library tests, and the named smoke on the final repair commit.
- [x] Shared modules contain no Windows-only types or unconditional Windows dependencies.
- [x] Regression evidence does not claim physical player testing from CI.
- [x] System documentation matches the implementation and exact event names.

## Acceptance checks

- [x] HUM-00-AC7 automated Windows regression contracts are green, with remaining physical checks recorded honestly for release validation.
- [x] HUM-00-AC8 system documentation is complete and indexed.
- [x] HUM-00-AC11 shared models and modules remain free of Windows types and unconditional Windows dependencies.
- [x] HUM-00-AC12 the exact final commit passes native Windows, macOS, Linux, and frontend jobs.
- [x] README describes the completed portable boundary in present tense.
- [x] No non-Windows artifact is packaged or published.
- [x] HUM-00G is committed and pushed with a green exact-commit workflow.
- [x] HUM-00 is fully complete before execution stops.

## External evidence rule

The final repair commit may describe the workflow as the required proof for that same commit. HUM-00 completion becomes valid only after GitHub reports every required job green for that commit. Do not create a second evidence-only commit that starts an unnecessary new proof loop.

## Amendment gate

Stop for approval if native compilation needs a new platform service, a new architecture mechanism, GUI launch infrastructure, packaging changes, installers, signing, updater artifacts, player adapters, Playwright, visual regression, or expansion beyond roughly twice this file list. Small cfg corrections or runner package fixes found by native CI remain failure repair within the slice.
