# HUM-00 progress ledger

This file is the durable execution cursor for HUM-00. Update it at every slice boundary before starting the next slice.

## Current cursor

- Slice: HUM-00, Complete
- Step: Closed out in v0.13.61
- Next action: begin HUM-10 only under a new execution order
- Blocker: None
- Last updated: 2026-08-19 11:45 MDT

## Completed slices

### HUM-00A, Shared media model

- Status: Complete
- Version: 0.13.54
- Commit: `ad11cae` (`Extract shared media model`)
- Remote: pushed to `origin/main`
- Validation: frontend typecheck and build passed; Cargo all-target check passed; Clippy passed with warnings denied; 125 Rust tests passed; targeted Rust formatting check passed; diff check passed
- Review: independent review found no actionable defects
- Acceptance criteria: HUM-00-AC1 complete
- Known deferrals: repository-wide Rust formatting debt remains recorded in `BUGS.md`

### HUM-00B, Build boundary

- Status: Complete
- Version: 0.13.55
- Commit: `a407895` (`Establish portable build boundary`)
- Remote: pushed to `origin/main`
- Validation: frontend typecheck and build passed; Cargo all-target check passed; Clippy passed with warnings denied; 125 Rust tests passed; full-tree Rust formatting passed; diff check passed
- Review: independent review found no actionable defects and confirmed that the 11 amended files contain only rustfmt output
- Acceptance criteria: HUM-00-AC4 complete
- Known deferrals: native macOS and Linux compilation remains assigned to HUM-00G CI; the non-Windows `dump_uia` message has no focused automated test

### HUM-00C, Media backend and publisher

- Status: Complete
- Version: 0.13.56
- Commit: `ff791b2` (`Add media backend publisher boundary`)
- Remote: pushed to `origin/main`
- Validation: frontend typecheck and build passed; Cargo all-target check passed; Clippy passed with warnings denied; 136 Rust tests passed; full-tree Rust formatting passed; diff check passed
- Review: independent review found two test-quality gaps, both were fixed red-first, and the follow-up review approved the production-backed ordering and artwork cache tests
- Acceptance criteria: HUM-00-AC2 and HUM-00-AC3 complete
- Known deferrals: native Windows playback smoke coverage and native non-Windows build proof remain assigned to later HUM-00 validation

### HUM-00D, Native window interfaces

- Status: Complete
- Version: 0.13.57
- Commit: `b10b6a4` (`Isolate native window effects`)
- Remote: pushed to `origin/main`
- Validation: frontend typecheck and build passed; Cargo all-target check passed; Clippy passed with warnings denied; 151 Rust tests passed; full-tree Rust formatting passed; diff check passed
- Review: independent review found four behavioral and portability issues, then one follow-up trigger regression; all five were fixed red-first and received final approval
- Acceptance criteria: HUM-00-AC10 complete
- Known deferrals: Reset all settings still does not reapply the default native backdrop immediately and is recorded in `BUGS.md`

### HUM-00E, Platform information and React

- Status: Complete
- Version: 0.13.58
- Commit: `8f7a9ab` (`Expose platform capabilities to React`)
- Remote: pushed to `origin/main`
- Validation: frontend typecheck and build passed; Cargo all-target check passed; Clippy passed with warnings denied; 157 Rust tests passed; full-tree Rust formatting passed; the platform recovery Node test passed; diff check passed
- Review: independent review found one updater recovery issue, which was fixed red-first and approved on follow-up
- Acceptance criteria: HUM-00-AC5 and HUM-00-AC6 complete
- Known deferrals: native macOS and Linux compilation remains assigned to HUM-00G CI; audio-output discovery remains intentionally false until HUM-00F

### HUM-00F, Audio-output discovery

- Status: Complete
- Version: 0.13.59
- Commit: `e203c9f` (`Add Windows audio output discovery`)
- Remote: pushed to `origin/main`
- Validation: frontend typecheck and build passed; Cargo all-target check passed; Clippy passed with warnings denied; 176 Rust tests passed; full-tree Rust formatting passed; diff check passed
- Review: independent review found a runtime ownership cycle and incomplete Bluetooth enumerator classification; both were fixed red-first and approved on follow-up
- Acceptance criteria: HUM-00-AC9 complete
- Known deferrals: physical multi-device smoke coverage and native macOS and Linux compilation remain assigned to HUM-00G; automatic profile selection remains assigned to HUM-30

### HUM-00G, Build proof and system documentation

- Status: Complete
- Version: 0.13.61
- Commit: closeout repair commit titled `Repair portable native compilation`
- Remote: pushed to `origin/main`
- Validation: frontend frozen install, typecheck, build, and Node retry test passed; 190 Rust tests passed; Cargo all-target check passed; Clippy passed with warnings denied; full-tree Rust formatting passed; the exact v0.13.61 repair commit passed the portable-core frontend, Windows, macOS, and Linux jobs
- Review: independent review found an incomplete OBS fingerprint, stale policy and ownership copy, prohibited long dashes, and two stateful seek-tracking defects; every issue was fixed red-first across three review rounds and received final approval
- Acceptance criteria: HUM-00-AC7, HUM-00-AC8, HUM-00-AC11, and HUM-00-AC12 complete
- Known deferrals: physical Windows player, layout, mode, audio-device, and OBS checks remain in the 1.0 release checklist; shipping macOS and Linux remains outside HUM-00

## HUM-00 completion

- Status: Complete
- Version range: 0.13.54 through 0.13.61
- Slices: HUM-00A through HUM-00G complete, committed, and pushed
- Acceptance: HUM-00-AC1 through HUM-00-AC12 complete
- External proof: portable-core workflow on the exact v0.13.61 repair commit
- Next phase: HUM-10, Licensing and entitlement foundation

## Execution log

- 2026-08-19 01:18 MDT: HUM-00A closed out and committed as `ad11cae`.
- 2026-08-19 01:30 MDT: `ad11cae` pushed to `origin/main` under the continuous-execution order.
- 2026-08-19 01:32 MDT: HUM-00B started at plan lock.
- 2026-08-19 01:30 MDT: HUM-00B implementation checks passed for 125 Rust tests, frontend typecheck and build, all-target Cargo check, and Clippy with warnings denied. The full formatting gate exposed 11 out-of-plan Rust files, so execution stopped at the required amendment gate before review or closeout.
- 2026-08-19 01:31 MDT: Wes approved the HUM-00B formatting amendment. Execution resumed at the full-gate fix.
- 2026-08-19 08:44 MDT: HUM-00B passed the amended full gate and independent review. The ledger advanced to HUM-00C plan lock pending the closeout commit and push.
- 2026-08-19 08:46 MDT: HUM-00B committed as `a407895`, pushed to `origin/main`, and HUM-00C plan lock began.
- 2026-08-19 08:52 MDT: HUM-00C writer, authority, startup, and consumer paths were mapped. The executable 14-file plan was locked with source ownership and event ordering as immutable contracts.
- 2026-08-19 09:09 MDT: HUM-00C passed 136 Rust tests and the full gate after red-first review fixes. The ledger advanced to HUM-00D plan lock pending the closeout commit and push.
- 2026-08-19 09:12 MDT: HUM-00C committed as `ff791b2`, pushed to `origin/main`, and HUM-00D plan lock began.
- 2026-08-19 09:18 MDT: HUM-00D native call sites, geometry, cadence, settings triggers, and failure behavior were mapped. The executable 11-file plan was locked.
- 2026-08-19 09:35 MDT: HUM-00D passed 151 Rust tests and the full gate after five red-first review fixes. The ledger advanced to HUM-00E plan lock pending the closeout commit and push.
- 2026-08-19 09:39 MDT: HUM-00D committed as `b10b6a4`, pushed to `origin/main`, and HUM-00E plan lock began.
- 2026-08-19 09:46 MDT: HUM-00E capability facts, path ownership, updater assumptions, and React control branches were mapped. The executable eight-file plan was locked.
- 2026-08-19 10:00 MDT: HUM-00E passed 157 Rust tests and the full gate after a red-first updater recovery fix. The ledger advanced to HUM-00F survey pending the closeout commit and push.
- 2026-08-19 10:02 MDT: HUM-00E committed as `8f7a9ab`, pushed to `origin/main`, and HUM-00F survey began.
- 2026-08-19 10:09 MDT: HUM-00F endpoint APIs, profile isolation, event contracts, polling lifecycle, and failure behavior were mapped. The executable nine-file plan was locked.
- 2026-08-19 10:35 MDT: HUM-00F passed 176 Rust tests and the full gate after red-first lifecycle and Bluetooth fixes. The ledger advanced to HUM-00G survey pending the closeout commit and push.
- 2026-08-19 10:39 MDT: HUM-00F committed as `e203c9f`, pushed to `origin/main`, and HUM-00G survey began.
- 2026-08-19 10:49 MDT: HUM-00G CI runners, shared-shell smoke, regression evidence tiers, documentation scope, and post-push proof rule were mapped. The executable seven-file plan was locked.
- 2026-08-19 11:24 MDT: HUM-00G passed 188 Rust tests and the full local gate after red-first OBS fingerprint and position-revision fixes. Independent review approved the final implementation, and the ledger closed HUM-00 pending the required exact-commit workflow proof.
- 2026-08-19 11:33 MDT: Portable-core run 32282078921 failed its macOS all-target check. The red exposed a macOS-only artist-window builder method, a cfg-sensitive Tauri command signature, and non-Windows re-export warnings. HUM-00G returned to native CI repair before completion.
- 2026-08-19 11:45 MDT: The macOS red was fixed through target-safe artist-window construction, fixed Tauri command signatures, and Windows-only production re-exports. The complete local gate and independent review passed, and v0.13.61 became the exact final native proof commit.
