# HUM-00 progress ledger

This file is the durable execution cursor for HUM-00. Update it at every slice boundary before starting the next slice.

## Current cursor

- Slice: HUM-00F, Audio-output discovery
- Step: F1, survey the accepted audio-output boundary and lock the executable plan
- Next action: map Windows endpoint discovery and active-output events without changing the selected listening profile
- Blocker: None
- Last updated: 2026-08-19 10:00 MDT

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
- Commit: closeout commit titled `Expose platform capabilities to React`
- Remote: push required before HUM-00F implementation begins
- Validation: frontend typecheck and build passed; Cargo all-target check passed; Clippy passed with warnings denied; 157 Rust tests passed; full-tree Rust formatting passed; the platform recovery Node test passed; diff check passed
- Review: independent review found one updater recovery issue, which was fixed red-first and approved on follow-up
- Acceptance criteria: HUM-00-AC5 and HUM-00-AC6 complete
- Known deferrals: native macOS and Linux compilation remains assigned to HUM-00G CI; audio-output discovery remains intentionally false until HUM-00F

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
