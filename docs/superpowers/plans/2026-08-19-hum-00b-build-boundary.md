# HUM-00B build boundary plan

Date: 2026-08-19
Status: Complete in v0.13.55
Related phase: [HUM-00](../../roadmap/1.0/HUM-00-portable-product-core.md)
Progress ledger: [HUM-00 ledger](../../roadmap/1.0/HUM-00-progress-ledger.md)

## Outcome

The shared Hum shell has honest operating-system boundaries. Windows-only crates and source files are compiled only on Windows, while common lyrics and diagnostic targets retain valid non-Windows code paths. Windows playback behavior remains unchanged.

## Scope

- Move `uiautomation`, `xcap`, and `tempfile` into Windows target dependencies.
- Compile the contrast worker only on Windows until the native screen-sampling interface lands in HUM-00D.
- Split the UI Automation dump utility into a portable entry point and its existing Windows implementation.
- Repair lyrics metadata branches so common code never names a Windows-only bridge on non-Windows.
- Restrict the Windows subsystem crate attribute to Windows release builds.
- Add focused source-boundary tests where the code exposes a pure seam.

## Planned implementation files

- `src-tauri/Cargo.toml`
- `src-tauri/src/lib.rs`
- `src-tauri/src/lyrics.rs`
- `src-tauri/src/main.rs`
- `src-tauri/src/bin/dump_uia.rs`
- `src-tauri/src/bin/dump_uia_windows.rs`

## Approved amendment

The first full formatting gate found repository-wide Rust formatting debt in 11 files outside the original six-file implementation list. Wes approved expanding HUM-00B to apply `cargo fmt` mechanically to these files so the required full-tree formatting gate becomes real instead of remaining a documented exception:

- `src-tauri/src/artist_info.rs`
- `src-tauri/src/artist_window.rs`
- `src-tauri/src/aspect_lock.rs`
- `src-tauri/src/contrast.rs`
- `src-tauri/src/itunes.rs`
- `src-tauri/src/mode.rs`
- `src-tauri/src/pandora_desktop.rs`
- `src-tauri/src/promos.rs`
- `src-tauri/src/smtc.rs`
- `src-tauri/src/web_bridge.rs`
- `src-tauri/src/youtube_bridge.rs`

This amendment is formatting-only. It does not authorize behavioral changes in those modules.

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, `BUGS.md` when needed, this plan, the HUM-00 roadmap and ledger, plus the Hum brain session and state files.

## Constraints

- Do not change Windows source priority, bridge freshness rules, emitted event names, payload fields, lyric providers, or timing math.
- Do not claim macOS or Linux runtime media support.
- Do not introduce `PlatformInfo`, media publishers, native window traits, or audio-output discovery in this slice.
- A native non-Windows CI proof belongs to HUM-00G. HUM-00B only removes known compile-time ownership violations.

## Implementation steps

1. Add the ledger and lock this plan.
2. Target-gate Windows dependencies and contrast startup.
3. Give the UI Automation diagnostic a non-Windows unsupported entry point while preserving its Windows command and behavior.
4. Isolate Windows bridge reads in lyrics behind compile-time branches.
5. Run the full gate: all Rust tests, frontend typecheck, frontend build, all-target Cargo check, Clippy with warnings denied, formatting checks, and diff checks.
6. Run an independent review focused on cfg completeness and Windows behavior preservation.
7. Audit every changed file, bump to v0.13.55 across all manifests, update the changelog, record any out-of-scope findings, update the roadmap and ledger, write the Hum brain records, commit, and push.

## Acceptance checks

- [x] `uiautomation`, `xcap`, and `tempfile` are Windows target dependencies.
- [x] Common Rust modules do not reference missing Windows-only variables.
- [x] `dump_uia` retains its Windows implementation and has a clear unsupported result elsewhere.
- [x] Windows tests and serialized media contracts stay green.
- [x] HUM-00-AC4 is complete.
- [x] HUM-00B is committed and pushed before HUM-00C begins.
