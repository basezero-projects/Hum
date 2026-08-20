# BUGS

Out-of-scope-to-fix-right-now things noticed during work. Each line should have enough context to act on later.

## Open

- **The Privacy policy action depends on an unregistered domain.** HUM-10F safely fixes the destination to `https://humlyrics.com/privacy`, but the page cannot serve customers until `humlyrics.com` is registered and the purchase site is deployed there. HUM-10G owns that launch work.
- **Promotional links still use the broad frontend opener permission.** HUM-10F routes support and privacy through fixed Rust-owned destinations, but `Overlay.tsx` still opens remote promotional URLs through `opener:default`. Move promo clicks behind a validated Rust allowlist in a later security-hardening slice.
- **LRCLib HTTPS requests can fail locally with a corrupt TLS content-type error.** During native release QA, both LRCLib lookup and search reported `received corrupt message of type InvalidContentType` while other app networking continued. Hum v0.13.65 now recovers through a hardened NetEase fallback, so known songs still receive synchronized lyrics. The direct LRCLib transport issue remains worth reproducing on a clean network before changing that client.
- **Portable-core CI actions still emit Node 20 deprecation annotations.** GitHub currently forces `actions/checkout@v4`, `actions/setup-node@v4`, and `pnpm/action-setup@v4` onto Node 24. The workflow passes, but the action majors should be reviewed and upgraded together after verifying their current release contracts. This is separate from Hum's application dependency versions.
- **Reset all settings does not immediately reconcile every native service.** `settings::reset_settings` persists and broadcasts defaults but does not reapply the default backdrop, stop an active OBS server, or disable operating-system autostart. The saved values are correct, but these native effects can remain active until another related change or a restart. Route reset through the same effect applicators as `update_settings` during the Settings polish slice.
- **The hidden Settings webview can load before Rust manages settings state.** Native v0.13.65 QA captured `state not managed` in the predeclared Settings window during startup. Hum v0.13.66 now retries saved-settings hydration in the overlay, which prevents a saved Square window from rendering temporary Ribbon defaults. The separate Settings window can still show its recoverable error page on first open, so reuse the production retry helper there in HUM-10F.
- **YouTube metadata normalization is foreground-tab-only.** `youtube_bridge::YouTubeProbe::read` (v0.13.42) confirms the playing track via `web_bridge::youtube_window_shows_track`, which matches a browser *window* title — and browsers only set the window title to the *active* tab's `document.title`. So a YouTube song playing in a background tab still shows the raw channel/decorated-title and misses lyrics/art. Same root limitation as the YouTube ad detector. Full fix would need per-tab enumeration (UIA tree or CDP) instead of window-title matching. Acceptable for now since foreground-tab playback is the common case.

## Resolved (v0.13.55)

- (was) **The full Rust tree was not rustfmt-clean.** HUM-00B applied the repository-wide formatting result after an explicit scope amendment. The full `cargo fmt --check` command is now a passing release gate.

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
