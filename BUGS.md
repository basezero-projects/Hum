# BUGS

Out-of-scope-to-fix-right-now things noticed during work. Each line should have enough context to act on later.

## Open

- **The update banner dot is SYVR gold, not a Hum color.** `UpdateBanner` in `Overlay.tsx` hardcodes `#d4af37` for its idle/working dot, which is the SYVR house gold rather than anything from Hum's own identity. Hum's mark is monochrome white on black, so the overlay is quietly wearing another product's brand. Noticed while rewriting the update lifecycle in v0.13.95; left alone because changing it is a design call, not a bug fix. Decide Hum's own accent before the launch polish pass.
- **The update banner cannot be dismissed.** The dot and label sit on an always-on-top window over whatever the user is watching, and the only way to clear an "available" state is to install. Tracked as slice 2 work in `docs/LAUNCH-CHECKLIST.md` alongside the no-relaunch-during-playback rule.

- **Remix tags survive title cleaning when the credit contains `&` or `.`.** `cleaner()`'s remix branch is `(?:[\w'\-]+\s+)*remix`, and `[\w'\-]` excludes both characters, so `Rainbow (Koastle & Beauvais Remix)` and `Black Eyed Peas - Where Is The Love (BCX feat. Ellena Soule Remix)` keep the whole tag and return zero records. `(Tiesto Remix)` works, `(Bruno Be Remix)` works. Verified 2026-08-25 by probe. Pipe-delimited remixes miss too (`Linkin Park - In The End | Masion Remix | Long Version`), because `pipe_tag_cleaner` only strips a trailing tag and this one sits mid-string with `| Long Version` after it. Widening the word-run to allow `&`, `.` and `+` would fix the bracketed cases. NOTE before doing it: all four misses in the 51-track run were remixes, and all five timing-downgrades were remixes too, so finding the original usually yields words whose timing does not fit the remix. The realistic best outcome is plain untimed lyrics rather than karaoke. Decide whether that is worth it before widening the pattern.
- **One track can produce two rows in the coverage log.** The resolver runs once on the raw SMTC title and again once the YouTube bridge publishes normalized metadata, and the two produce different `(raw_title, raw_artist)` keys so `merge_resolution` cannot collapse them. Seen as `Tipsy Records / Black Eyed Peas - Where Is The Love (BCX...` alongside `Black Eyed Peas / Where Is The Love (BCX`. Inflates the track count by roughly 15% and doubles the network calls per track. The 2.5s wait-for-bridge grace in `lyrics.rs::start` is meant to prevent this and is expiring first; find out why before widening the window.
- **A bridge-normalized title can be truncated mid-token.** The same Black Eyed Peas track normalized to `Where Is The Love (BCX`, an unclosed parenthesis with the rest of the credit gone. `parse_youtube_metadata` or one of the cleaners is cutting inside a bracketed group. A malformed title is a bad lyric query and a plausible source of false matches.
- **`promos.json` returns 404.** `https://syvrstudios.com/hum/promos.json` has been 404ing on every startup through the whole 2026-08-25 session. Ad-break promo cards cannot render until it is served. Not urgent for development, blocking for launch.


- **The Privacy policy action depends on an unregistered domain.** HUM-10F safely fixes the destination to `https://humlyrics.com/privacy`, but the page cannot serve customers until `humlyrics.com` is registered and the purchase site is deployed there. HUM-10G owns that launch work.
- **Promotional links still use the broad frontend opener permission.** HUM-10F routes support and privacy through fixed Rust-owned destinations, but `Overlay.tsx` still opens remote promotional URLs through `opener:default`. Move promo clicks behind a validated Rust allowlist in a later security-hardening slice.
- **Portable-core CI actions still emit Node 20 deprecation annotations.** GitHub currently forces `actions/checkout@v4`, `actions/setup-node@v4`, and `pnpm/action-setup@v4` onto Node 24. The workflow passes, but the action majors should be reviewed and upgraded together after verifying their current release contracts. This is separate from Hum's application dependency versions.
- **Reset all settings does not immediately reconcile every native service.** `settings::reset_settings` persists and broadcasts defaults but does not reapply the default backdrop, stop an active OBS server, or disable operating-system autostart. The saved values are correct, but these native effects can remain active until another related change or a restart. Route reset through the same effect applicators as `update_settings` during the Settings polish slice.
- **The hidden Settings webview can load before Rust manages settings state.** Native v0.13.65 QA captured `state not managed` in the predeclared Settings window during startup. Hum v0.13.66 now retries saved-settings hydration in the overlay, which prevents a saved Square window from rendering temporary Ribbon defaults. The separate Settings window can still show its recoverable error page on first open, so reuse the production retry helper there in HUM-10F.
- **Full YouTube metadata normalization is foreground-tab-only.** `youtube_bridge::YouTubeProbe::read` confirms the playing track from the browser window title, which only describes the active tab. Hum v0.13.82 can recover lyrics from decorated video titles and VEVO channels without that bridge, but background tabs can still show the raw uploader metadata, miss better artwork, and miss lyrics when the title has no recognizable video markers. YouTube ad detection has the same limit. A full fix needs per-tab enumeration through the accessibility tree or CDP.

## Resolved (v0.13.89)

- (was) **`lrclib.net` is blocked by SNI filtering on Wes's network, so LRCLib never answers and lyric coverage collapses to NetEase.** Confirmed SNI-based: the same Cloudflare IP and port accepts `www.cloudflare.com` and `example.com` at TLS 1.3 and drops `lrclib.net`, identically through schannel and OpenSSL, with DNS clean throughout. Fixed by fronting LRCLib at `lyrics.syvr.dev` (Cloudflare Worker in `worker/lyrics-proxy`) and having `lrclib_request` fall back to it when the direct route fails at the transport layer. Verified live on the blocked machine: 20 real search records in 218ms and a synced `/api/get` record in 21ms. Direct stays the preferred route, with a ten-minute suppression window after a failure.

## Resolved (v0.13.88)

- (was) **A failed primary lyric source was reported to the user as an authoritative "no lyrics found".** `resolve_lyrics` let `any_clean_notfound` settle the result, so an errored LRCLib plus a clean NetEase miss rendered as a real miss. Fixed by tracking `lrclib_clean_miss` specifically and moving the decision into a unit-tested `miss_outcome` helper. An errored primary now surfaces `Status::Error`.
- (was) **An errored lyric fetch stayed broken for the rest of the track.** The resolver's `last_key` dedupe claimed the track after the first attempt, so a transient failure never re-resolved. Fixed with an `ErrorRetry` backoff schedule (2, 5, 15, 45, 120s, five attempts) riding the existing `timeline-changed` wakeups.

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
