# BUGS

Out-of-scope-to-fix-right-now things noticed during work. Each line should have enough context to act on later.

## Open

- **PowerShell COM-poller child can still orphan when the parent is killed externally.** `src-tauri/src/itunes.rs` now spawns the child with `kill_on_drop(true)` + a unique `tempfile::NamedTempFile`, so clean exits / panics / future cancellation all kill the child and remove the temp script. But when `pnpm tauri dev` SIGTERMs the parent without giving Tokio a chance to drop the Child, the PowerShell poller still survives. Full fix: assign the child to a Windows JobObject with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so the OS guarantees teardown. Real end users (one cold start = one child, OS reaps on shutdown) are unaffected — this is a dev-iteration nuisance only.
- **Cache key collision on `|` in artist or title.** `src-tauri/src/lyrics.rs::cache_key` formats as `"{artist}|{title}|{dur_secs}"`. Artist `"a|b"` + title `"c"` collides with artist `"a"` + title `"b|c"`. Triggers wrong-lyric display, no security impact. Either pick a delimiter unlikely to appear in metadata (e.g. `\x1f`) or hash the components. Audit reference: L1.
- **`#[allow(dead_code)]` blanket spam in `src-tauri/src/artist_info.rs`.** ~20 annotations across the file mark `ArtistInfoCache`, `fetch_ticketmaster_events`, `fetch_theaudiodb_photo`, the constants (`TICKETMASTER_API_KEY`, `THEAUDIODB_BASE`, `IMPACT_AFFILIATE_PREFIX`), and the cache I/O helpers (`cache_dir`, `cache_file_path`, `read_cache_file`, `write_cache_file`, `build_artist_info_from_cache`) as dead — but they're all actually live, called from `lib.rs` via `.manage(ArtistInfoCache::new(...))` + the `get_artist_info` / `clear_artist_info_cache` Tauri commands. The annotations are leftover from a refactor that never cleaned up; they mask any real dead-code warnings in this module. Walk every annotation, remove the ones masking live code, leave them only where the symbol is truly unused.
- **[pandora_desktop] Normal-track poll path does two full UIA tree walks per cycle:** `collect_pandora_uia_data` collects URLs + countdown, then `extract_track_from_uia_subtree` re-walks the same tree for Name/Value triples. Consolidate into one pass — extend `collect_pandora_uia_data` to also capture `(Name, Value, kind)` per Hyperlink and drop `extract_track_from_uia_subtree`. Introduced in commit `4d087aa` (Task 7).

## Resolved (Phase 4–6)

- (was) **PowerShell child fixed-temp-path TOCTOU.** Fixed by `tempfile::Builder` random suffix in `itunes.rs` (audit M2).
- (was) **No size cap on SMTC thumbnail.** Fixed with 10MB ceiling before allocation in `smtc.rs::read_thumbnail_bytes` (audit M4).
- (was) **NetEase lyric URL built via `format!`.** Switched to `Url::parse_with_params` in `lyrics.rs::fetch_netease` (audit M1).
- (was) **Manager-level SMTC session token never explicitly removed.** Wrapped in `ManagerHook` with `Drop` impl that calls `RemoveCurrentSessionChanged` (audit M3).
- (was) **CSP allowed `https:` for `img-src`.** Tightened to `data:` only since album art is delivered as data URLs (audit M5).
- (was) **`update_settings` accepted unvalidated patches.** Added `sanitize()` covering hex-color regex, enum-string allowlists, font-family char filter, numeric clamps. Runs on both update and load paths (audit H1, H2).
