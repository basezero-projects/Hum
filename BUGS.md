# BUGS

Out-of-scope-to-fix-right-now things noticed during work. Each line should have enough context to act on later.

## Open

- **YouTube metadata normalization is foreground-tab-only.** `youtube_bridge::YouTubeProbe::read` (v0.13.42) confirms the playing track via `web_bridge::youtube_window_shows_track`, which matches a browser *window* title — and browsers only set the window title to the *active* tab's `document.title`. So a YouTube song playing in a background tab still shows the raw channel/decorated-title and misses lyrics/art. Same root limitation as the YouTube ad detector. Full fix would need per-tab enumeration (UIA tree or CDP) instead of window-title matching. Acceptable for now since foreground-tab playback is the common case.

## Resolved (v0.13.46)

- (was) **SMTC session-change churn on every YouTube track change.** Fixed by debouncing `Msg::SessionChanged` in `smtc.rs`: 200ms sleep + drain all queued messages before acting. Stale events from the dying session are harmless to drop since `attach_session` + `emit_full` re-reads all state from scratch.
- (was) **Auto-update banner only renders in 3-line layout.** Added `<UpdateBanner>` to `single_line` and `full_page` layout branches in `Overlay.tsx`.
- (was) **PowerShell COM-poller child can still orphan when parent is killed externally.** Fixed by assigning the child to a Windows JobObject with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` in `itunes.rs`. OS guarantees teardown when our process exits, even under SIGTERM.
- (was) **Cache key collision on `|` in artist or title.** Fixed by switching delimiter to `\x1f` (ASCII unit separator) in `lyrics.rs::cache_key`.
- (was) **`#[allow(dead_code)]` blanket spam in artist_info.rs.** Investigated: only 1 annotation exists (not ~20 as originally reported), and it's correct — `fetch_artist_info` is genuinely dead code (the cache wrapper is used instead). No action needed.
- (was) **pandora_desktop double UIA tree walk.** Consolidated `collect_pandora_uia_data` and `extract_track_from_uia_subtree` into a single DFS pass that collects URLs, countdown, and track/artist/album names in one walk.

## Resolved (Phase 4–6)

- (was) **PowerShell child fixed-temp-path TOCTOU.** Fixed by `tempfile::Builder` random suffix in `itunes.rs` (audit M2).
- (was) **No size cap on SMTC thumbnail.** Fixed with 10MB ceiling before allocation in `smtc.rs::read_thumbnail_bytes` (audit M4).
- (was) **NetEase lyric URL built via `format!`.** Switched to `Url::parse_with_params` in `lyrics.rs::fetch_netease` (audit M1).
- (was) **Manager-level SMTC session token never explicitly removed.** Wrapped in `ManagerHook` with `Drop` impl that calls `RemoveCurrentSessionChanged` (audit M3).
- (was) **CSP allowed `https:` for `img-src`.** Tightened to `data:` only since album art is delivered as data URLs (audit M5).
- (was) **`update_settings` accepted unvalidated patches.** Added `sanitize()` covering hex-color regex, enum-string allowlists, font-family char filter, numeric clamps. Runs on both update and load paths (audit H1, H2).
