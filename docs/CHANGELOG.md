# Changelog — Hum

All notable changes to this project. Updated on **every commit**, not at the end of a task.

> The app shipped under the name **Lyric Overlay** through v0.10.1. Renamed to **Hum** in v0.10.2 (2026-05-21). Historical entries below still refer to the old name and the old `com.syvr.lyric-overlay` identifier — those references are accurate for the version they describe.

Versions follow `X.Y.Z` (bump all of `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` per commit).

## [0.13.16] - 2026-05-22

### Changed
- **Logo watermark is now significantly larger — the PNG canvas was cropped to the actual content.** The v0.13.15 file was 512×512 with the visible ellipse only occupying roughly the middle 47% × 65% of that canvas (the rest was transparent padding from the original source JPG). At `height: 85%` of the overlay container, the CSS was sizing the whole 512px canvas, not just the visible ellipse — so the rendered logo was much smaller than its container slot. New file is 241×333 (the bounding box of the actual logo pixels), so the same `height: 85%` now scales the visible ellipse to ~1.55× its previous rendered height. File size 5.8 KB → 4.7 KB.

## [0.13.15] - 2026-05-22

### Changed
- **Watermark is the actual Hum brand logo (HUM glyph in an ellipse) instead of text.** Replaces the "Hum by SYVR" text watermark with the real circle-letter logo — a white HUM glyph inside a vertical ellipse on a transparent background. Centered, sized to 85% of the overlay container height with auto width preserving aspect ratio (the ellipse is taller than wide so on a thin-and-wide overlay window the watermark reads as a centered vertical mark between the album art and the metadata column). Same opacity 0.18 (kept as a soft ghost mark). Asset lives at `public/hum-logo.png` (5.8 KB; original was a 6159×6159 JPG, downsized to 512×512 PNG with luminance-as-alpha so anti-aliased edges stay clean against any background).

### Added
- **`GET /hum-logo.png` route on the streamer endpoint** serves the logo as an embedded byte slice (`include_bytes!`) so the OBS browser source stays self-contained — no network calls, no separate file to deploy. Cache-Control: `public, max-age=31536000, immutable`.

## [0.13.14] - 2026-05-22

### Changed
- **Watermark font is now Cascadia Code (monospace) instead of Inter (default UI sans).** Inter at 64px bold looked like the UI font scaled up — generic, no personality, read as "placeholder." Cascadia Code is a monospace display font that ships on Windows (used by Windows Terminal); at large display sizes it gives the watermark an intentional, designed feel rather than a "we forgot to pick a font" feel. Falls back to Cascadia Mono → Consolas → "Courier New" → generic monospace for any system without it. Same 64px, weight 700, opacity 0.18, white, centered.

## [0.13.13] - 2026-05-22

### Fixed
- **Secondary text (metadata, timestamps, prev/next lyric lines) is now solid light gray instead of alpha-blended white, so it stays readable on bright album-art backgrounds.** Previously `--text-dim` was `rgba(255, 255, 255, 0.55)` and the desktop autocolor mode used `rgba(255, 255, 255, 0.45)` — both let the background color bleed through, which meant a bright yellow/orange/cyan album cover would tint the dim text the same color and kill contrast (the v0.13.11 halo shadow helped but couldn't fully rescue it). New values: `#c8c8c8` (streamer `--text-dim`, desktop dark-surface autocolor, default `text_color_dim`), `#909090` (streamer `--text-faint`), `#5a5a5a` (desktop light-surface autocolor). Solid hex values render the same regardless of what's behind them. Includes a one-shot migration in `settings.rs` that swaps the old exact-default `rgba(255,255,255,0.45)` to the new solid value on load — users who customized their dim color in Settings keep their value untouched. Theme presets on the streamer side (neon, retro, minimal) still use their own opinionated colors.

## [0.13.12] - 2026-05-22

### Changed
- **Watermark is now a centered ghost-mark instead of a small corner credit.** Moved from bottom-center to vertically + horizontally centered (`top: 50%; left: 0; right: 0; transform: translateY(-50%); text-align: center;`). Font size 28px → 64px. Opacity 0.4 → 0.18. Letter-spacing 0.5 → 1px to match the bigger size. Text-shadow removed (at opacity 0.18 a dark shadow would render darker than the text itself on busy backgrounds). The watermark now reads as a soft sponsor-style brand mark behind the lyric content rather than a small credit underneath it. Applies to both the desktop overlay and the OBS browser source. `z-index: 1` keeps it behind the streamer's `#row` lyric stack; on the desktop side it sits at the same level as the flex children but the low opacity makes the overlap a non-issue.

## [0.13.11] - 2026-05-22

### Changed
- **Watermark is thicker and more transparent.** Font weight 400 → 700 (bold), opacity 0.55 → 0.4. The heavier weight reads as a clearer brand mark at the new size while the lower opacity keeps it pushed behind the lyric content. Applies to the desktop overlay and the OBS browser source.

### Fixed
- **Metadata text and timestamps are now legible over bright/busy backgrounds.** The artist · title · album line and the m:ss / m:ss timestamps were disappearing on tracks with bright blurred album-art backgrounds (e.g. yellow/orange covers like "Breakeven" by The Script). Streamer side was the worst case — `#meta-text` and `#meta-row` had `color: var(--text-dim)` / `var(--text-faint)` with NO text-shadow, so once the background went bright they vanished. Desktop side had the lyric text-shadow applied but it was tuned for the bigger lyric font and wasn't enough for the smaller metadata. Both surfaces now use a triple-stack halo shadow on the metadata: `0 1px 2px rgba(0,0,0,1), 0 0 6px rgba(0,0,0,0.85), 0 3px 10px rgba(0,0,0,0.55)` — solid 1px contact shadow + 6px black halo + 10px soft spread. The halo is what does the work against bright backgrounds; reads as a subtle glow but keeps the text crisp.

## [0.13.10] - 2026-05-22

### Changed
- **Watermark is white now instead of gold.** Matches the lyric text color (`--text` on the streamer side, `#ffffff` on the desktop overlay). Over arbitrary blurred album-art backgrounds, gold ended up clashing with whatever color was bleeding through — white reads as a subtle behind-the-content mark without fighting the background. Streamer-side: switched from `var(--gold)` to `var(--text)`, which means `?accent=<hex>` no longer re-tints the watermark (accent now stays focused on actual brand accents, not the credit).

## [0.13.9] - 2026-05-22

### Changed
- **Watermark is now bottom-center, much larger, and softer.** Moved from the bottom-right corner to bottom-center (full-width, `text-align: center`). Font size 13px → 28px. Opacity 0.7 → 0.55 for a more "behind the content" feel. Letter-spacing 0.4px → 0.5px (slight bump to balance the bigger font). Color, gold (`#d4af37`), and the dual text-shadow stay the same. Applies to both the desktop overlay (all three layout modes) and the OBS browser source.

## [0.13.8] - 2026-05-22

### Changed
- **Watermark text is now "Hum by SYVR" instead of "hum.syvr.dev"**, and grew from 9.5px to 13px. v0.13.7 inherited the `hum.syvr.dev` string from the runway plan in the prior session — but that domain isn't owned/planned, and a URL no one can actually visit is a worse search target than the product name itself. New text is brand-forward and gives viewers something recognizable to look up. Bottom-right corner across all three desktop overlay layout modes and on the OBS browser source. Same gold color (`#d4af37`), same opacity (0.7), same letter-spacing (0.4px), same dual text-shadow for legibility, same `z-index: 5` (desktop) / `z-index: 3` (streamer).

## [0.13.7] - 2026-05-22

### Changed
- **`hum.syvr.dev` watermark now lives on the overlay itself, not just the OBS browser source.** Previously v0.13.6 added the watermark only to the streamer endpoint (`http://127.0.0.1:38247/overlay`) and gated it behind `?credit=1`. That model assumed every streamer would set up the OBS browser source — but many capture Hum via window capture or display capture of the desktop overlay window itself, which never touched the browser-source URL and so never saw the credit. New model: the watermark is drawn directly on the desktop overlay container in the bottom-right corner (small gold text, 9.5px, letter-spacing 0.4px, opacity 0.7, twin text-shadows for legibility — same visual as the streamer version). Appears in all three overlay layout modes (single-line, full-page, default 3-line), sits above the lyric content via `z-index: 5`, doesn't intercept pointer events, doesn't compete with the lyric column. Same mark also still paints on the OBS browser source — now default-on instead of `?credit=1`-gated, so a streamer who uses the browser source URL gets the credit without configuration. Net effect: however a streamer captures Hum, viewers see "hum.syvr.dev" in the bottom-right corner.
- **`?credit=1` URL param is no longer honored** — the watermark is unconditional on the streamer endpoint now. Param is quietly ignored if present (no error, no breakage for anyone who already pasted that URL).

## [0.13.6] - 2026-05-22

### Added
- **Optional `hum.syvr.dev` watermark on the OBS browser source** — opt-in via the new **`?credit=1`** URL param. When enabled, the text `hum.syvr.dev` appears in the bottom-right corner of the OBS browser source in small letterspaced gold (9.5px, letter-spacing 0.4px, opacity 0.7, subtle dark text-shadow for legibility over bright captures). Default is OFF — streamers who want to credit Hum on stream add `?credit=1` to the browser source URL; streamers who want a clean overlay leave it off. The watermark color uses `--gold`, so a `?accent=hex` override re-tints it (e.g. `?credit=1&accent=ff00aa` paints the credit magenta). Sits inside the `#wrap` container so the `?only=` source filter blanks it along with the rest of the layout when the active source doesn't match, and stays put under both the lyric/metadata row and the ad-break PromoCard branch. Orthogonal to `?nochrome=1` — streamers can run minimalist mode and still keep the credit. Eventually a Pro tier will toggle this from the Settings panel; until then it's URL-param only.

## [0.13.5] - 2026-05-22

### Added
- **Karaoke-style per-word highlighting on the OBS browser source.** When the lyric source provides word-level timings (SimpMusic's `richSyncLyrics`, NetEase word-level data — already in `WordSpan` for the desktop overlay), each word in the current line now fills left-to-right with the text color as it's sung, instead of the whole line snapping to lit-color at once. Rendered as per-word `<span>` elements with a two-stop linear gradient (lit half / dim half) sized 200% wide, clipped to the glyphs via `background-clip: text`. Background-position transitions 100%→0% over each word's duration (next-word-time minus current-word-time, floored at 80ms so tightly-packed lyrics still register). Lines without per-word timings fall back to the existing line-level animation. Computed at rAF cadence client-side from the interpolated playback position — no extra server load, no SSE messages per word.
- **Track-change stinger animation on the OBS browser source.** When the track key changes (title|artist diff), the album art slides in from the left with a small scale bounce (-18px → 0px, 0.92 → 1.0 scale, 600ms cubic-bezier), the metadata column fades in from the right with an 80ms delay (14px → 0px, 600ms cubic-bezier), and the lyric line uses the existing `.anim-in` lift-and-fade. Together they give a clear visual "moment" on song change that reads on stream. Removed automatically 800ms after the swap so the animation doesn't loop.
- **URL params for streamer customization** — `http://127.0.0.1:<port>/overlay?<params>`:
  - **`?theme=neon|retro|minimal|default`** — visual presets. Neon is electric magenta dim + cyan accent; retro is sepia parchment + orange accent; minimal kills the blur background + plate (full transparency) and uses pure white for accents; default keeps the existing monochrome dark + gold look. Applied via body classes that override CSS custom properties.
  - **`?accent=hex`** — overrides the gold accent color directly. Accepts 3-8 hex chars with or without `#` prefix. Useful for matching the streamer's brand color exactly. Combines with `?theme=` (accent overrides whatever the theme set for `--gold`).
  - **`?only=spotify|pandora|itunes|youtube|browser|...`** — render only when the active source matches. Substring match against the server's `source_label` (case-insensitive), so `?only=apple` matches both "Apple Music" and any source containing "apple". Lets streamers layer multiple OBS browser sources — one per service — and each filters to its own. Mismatched source = wrap is set to `visibility: hidden` (OBS sees nothing, but the page keeps polling so re-matches re-show without a reload).
  - **`?nochrome=1`** — back to the v0.13.0 minimalist 3-line look (no album art, no metadata column, no blurred background, no plate). For streamers who want only the lyrics with everything else handled by their own OBS layout.

### Changed
- **OBS source's `<title>` is `Hum — OBS source`** to make it clear in OBS's source list which browser-source is the lyric overlay (vs other browser sources the streamer may have).
- **Track-change detection guards against the empty-track edge case.** Previously the `(title || "") + "|" + (artist || "")` comparison fired on app boot when track was still loading (both empty), then immediately fired again when the real track arrived, triggering a stinger on the boot-to-first-song transition. Now skips when both fields are empty so the first song's stinger only fires when there's a real track to highlight.

### Internal
- **New `.claude/launch.json` config** for the preview tool to reach the streamer endpoint during dev verification. Points at port 38247 with a no-op `powershell sleep` runtimeExecutable since the Hum binary is independently launched. Used by the post-Write hook verification workflow.

## [0.13.4] - 2026-05-22

### Fixed
- **Lyric timing no longer needs a per-song +500ms nudge on every track.** The Settings panel's "Anticipation" slider defaulted to 500ms, which meant lyrics were displayed 500ms BEFORE their LRC-tagged time on every song. For most Spotify users that's too aggressive — Spotify reports the decoder's position, which sits a few hundred ms ahead of the audio the user actually hears (decoder buffer + OS audio output buffer), so the 500ms anticipation stacked on top of the natural buffer made lyrics appear ~half-a-line early. Users were compensating with the per-song Ctrl+Alt+] nudge but that nudge resets on every track change by design (so a bad-LRC fix doesn't bleed). The fix: default is now **0ms** (display at LRC truth), and the slider is renamed "Lyric offset" with a range of **−2000 to +2000 ms** (was 0 to 1500) so users whose setup runs early or late can dial in once and never touch the per-song nudge. Help text now explains the directionality: positive = anticipate, negative = delay. The Rust backend's clamp was already [-2000, 5000] — only the frontend slider was blocking negative values.
- **OBS browser-source overlay now respects the lyric-offset setting.** Previously the streamer's `/state` and `/events` cursor was computed from raw `position_ms` with no offset applied — so if your desktop overlay had `anticipate_ms = 500` saved, the OBS view ran 500ms behind it. Now `streamer.rs::build_state` reads `SharedSettings` and applies the same offset formula the live overlay does (`src/Overlay.tsx::lookupPositionMs`). Desktop and stream stay in sync.

### Changed
- **"Anticipation" slider in Settings is now labeled "Lyric offset"** with a clarified help blurb that names the direction (positive = earlier, negative = later) and the per-song hotkey escape valve (Ctrl+Alt+[ / Ctrl+Alt+]).

## [0.13.3] - 2026-05-22

### Changed
- **OBS browser source now mirrors the desktop overlay's chrome — album art, metadata, progress bar, source badge, ad-break promo card.** Previously the streamer overlay (`http://127.0.0.1:<streamer_port>/overlay`) rendered a stripped-down 3-line lyric column only: no album art, no artist · title · album header, no `0:39 / 3:44` progress + bar, no SPOTIFY / PANDORA / ITUNES badge, no fallback during ad breaks. Streamers comparing the OBS preview to the desktop overlay saw a noticeably emptier picture and ads showed a generic "no lyrics" status instead of the SYVR PromoCard. New `streamer_overlay.html` layout: album art square on the left (loaded from the new `/art` endpoint), 3-line lyric stack in the middle with karaoke-style cursor advancement, metadata column on the right with the artist · title · album line, a 120px progress bar that interpolates locally between server pushes, the `m:ss / m:ss` time row, and a pill-shaped source label. During ad breaks the layout swaps to the same PromoCard render (image-driven when `image_url` is set, text-driven fallback with supertitle / product / tagline / CTA) and the source badge flips to a gold "Ad Break" chip. Background carries a blurred-art aesthetic plate identical to the desktop overlay (filtered `blur(40px) saturate(1.35) brightness(0.62)`). Edit-mode dashed gold border is omitted — it's a desktop-only affordance that has no value on stream.
- **Streamer overlay state pushes are now sub-100ms — switched from 250ms polling to Server-Sent Events.** The 250ms `/state` poll loop previously left the OBS view up to a quarter-second behind the desktop overlay; comparing the two side-by-side made the lag obvious to the streamer. New `/events` endpoint streams the same state payload via SSE (axum's built-in `axum::response::sse` types). Internal cadence is 100ms with a fingerprint-diff guard so position-only ticks don't flood the wire — only changes to track / lyrics status / cursor / ad_active / playback state / album art trigger a push. The client interpolates the progress bar locally between pushes (rAF cadence), so the bar advances smoothly even though the server itself only pushes on change. Auto-reconnects if OBS reloads the source; polls `/state` every 500ms as a fallback only when the initial EventSource never opens. Backwards-compatible: `/state` remains as a one-shot poll endpoint for external consumers.
- **New `/art` endpoint serves the cached album art image bytes.** Previously the streamer page hard-coded `$art.style.display = "none"` with a comment "add a /art endpoint if streamers ask." Endpoint decodes the desktop fetch chain's `data:image/...;base64,...` URL into raw bytes and serves with the correct `Content-Type` so `<img src="/art?k=<track_key>">` Just Works. Returns 404 when no art is currently cached. Client appends `?k=<art_key>` so the browser caches per track and refreshes naturally when the track changes.
- **New `/events` endpoint adds Server-Sent Events.** See above. Streams `event: state\ndata: <json>\n\n` frames; auto-keep-alive every 15s prevents proxy / OBS-source idle timeouts.

### Internal
- **Tauri runtime gained the `time` feature.** Required for `tokio::time::interval` in the SSE tick loop. Already implied by `rt-multi-thread` for the runtime itself but the `tokio::time::*` API surface needs the feature explicitly.
- **New dependency: `async-stream = "0.3"`.** ~150 LOC crate used for the SSE stream macro. No transitive deps beyond what axum already pulls in.

## [0.13.2] - 2026-05-22

### Fixed
- **Pandora-in-Chrome ads now flip the overlay to the SYVR promo card reliably.** Previously, when a Pandora ad break started while Chrome's MediaSession kept publishing the previous song's title with state=Playing, the lyric area kept trying to fetch lyrics for the stale track instead of swapping to the promo card. The browser bridge's URL-classified ad signal was being silently dropped because the writeback gate also suppressed the position-emit when SMTC was "actively playing." The two concerns are now decoupled in `src-tauri/src/web_bridge.rs`: the bridge's `ad_active` flag is always synced to the shared snapshot when a probe is active (Pandora web, YouTube web, Pandora desktop — the active probe by definition matches SMTC's `source_app_id`, so they describe the same source), while the synthesized `timeline-changed` emit still defers to SMTC's authoritative position when it has one. User impact: AD BREAK transitions on Pandora web now consistently render the promo card with no "no lyrics for —" gap.
- **Settings now save reliably when both the Overlay and Settings windows write at the same time.** Previously, two concurrent `update_settings` Tauri commands (e.g. Overlay reacting to a hotkey while the Settings panel toggles a checkbox) could race: both windows would read the same baseline, each merge its own change, and the later write would overwrite the earlier one — silently losing one of the two updates. The full read-merge-write sequence in `src-tauri/src/settings.rs` now runs under a single held write lock, so concurrent updates serialize correctly. User impact: changes made in the Settings panel while the Overlay is also writing settings (e.g. mode-toggle hotkeys, streamer-on toggle) no longer get clobbered.

### Changed
- **Lyric cache is now size-capped at 256 entries (was unbounded).** The in-memory lyric cache in `src-tauri/src/lyrics.rs` previously grew without limit over long listening sessions — heavy day-long playback could push it past 1000 unique tracks, bloating app RSS by 10+ MB. Now uses the `lru` crate (added as a dependency) with a 256-entry LRU cache: ~1MB typical / ~12MB worst-case ceiling. Tracks evicted from memory still hit the persistent `tauri-plugin-store` cache on the next play, so cold misses cost only a single store read, not a network round-trip. No UI change — users won't see lyrics behave differently.
- **Artist-info disk cache is now size-capped at 500 entries with oldest-mtime eviction.** The per-artist JSON cache at `%APPDATA%/com.syvr.hum/cache/artist/<slug>.json` (populated by Wikipedia bio + TheAudioDB photo + Ticketmaster tour-dates fetches) previously grew indefinitely — one file per artist you've ever opened the artist panel for, no size cap. After 10K artists that's 10+ MB on disk forever. Now sweeps at startup via a non-blocking background task in `src-tauri/src/lib.rs`: when more than 500 JSON files exist, the oldest-mtime files are removed until count == 500. Tour-date refresh rewrites mtime every 12h, so artists you're actively listening to stay cached; one-off artist panel views are first to evict. New helper: `artist_info::sweep_disk_cache`. User impact: invisible — opening a previously-cached artist that's been evicted just triggers a re-fetch, same as the first time.

## [0.13.1] - 2026-05-22

### Changed
- **Internal cleanup — no user-visible changes.** Audit-driven pass after running `/audit full` (15 Tier 1 workers, ~13.6 kloc surface):
  - **Dead-code annotation cleanup in `src-tauri/src/artist_info.rs`.** Removed 16 false-positive `#[allow(dead_code)]` annotations after verifying each annotated symbol has a real caller. The annotated functions / constants / structs (`slug_for_artist`, `tour_dates_stale`, `TICKETMASTER_API_KEY`, `TICKETMASTER_DISCOVERY_BASE`, `IMPACT_AFFILIATE_PREFIX`, `fetch_ticketmaster_events`, `THEAUDIODB_BASE`, `fetch_theaudiodb_photo`, `CachedArtistData`, `now_unix_ms`, `cache_dir`, `cache_file_path`, `read_cache_file`, `write_cache_file`, `ArtistInfoCache` struct + impl, `build_artist_info_from_cache`) are all reachable through `ArtistInfoCache::fetch` (Tauri-managed state wired in `lib.rs`) or unit tests. The single remaining annotation on `fetch_artist_info` is correct — that's a genuinely unused public top-level API kept as a non-cached fallback entry point. `cargo check` + `cargo clippy` clean post-cleanup.
  - **CLAUDE.md doc drift fixes.** Stack table updated to reflect v0.13.0 reality: added Tailwind 4 + browser bridges + iTunes PowerShell bridge + lyrics fallback chain + artist info APIs + streamer server + promo source + auto-updater rows. Architecture section rewritten as a four-layer model (sources → blend → resolve → render) instead of the original three-layer scaffolding description. Phase status section replaced — previously claimed "Phase 1 implemented, Phases 2-6 not started," now lists all ten phases shipped (SMTC source, LRCLib, overlay render, hotkeys, settings, streamer, browser bridges, artist panel, ad-break detection, image-driven PromoCards) with next-planned slices below.
  - **README.md version bump.** Current-version line updated from v0.11.7 → v0.13.0; bundle filename example updated to match.
  - **RELEASE_NOTES.md rewrite.** Replaced the unfilled `{{PROJECT_NAME}}` template placeholder + "v0.1.0 / Not released yet" header with current v0.13.0 release notes (image-driven PromoCards: full hero images can now fill the lyric area during ad breaks; text-driven layout preserved as fallback).

## [0.13.0] - 2026-05-22

### Added
- **Image-driven SYVR PromoCards — design every pixel.** Promos can now ship as a single designed hero image rendered edge-to-edge in the lyric area during an ad break, replacing the text-driven product name + tagline + CTA layout. This matches how real advertisers (Chipotle, T-Mobile, etc.) deliver Spotify ads — the advertiser controls every pixel of the creative, including custom typography, gradients, logos, brand colors. The text-driven layout from v0.11.9 is preserved as a fallback for promos without an image.

  **Schema:** the `Promo` type in `promos.json` gains two optional fields:
  - `image_url` — URL of the hero image (PNG/JPG/SVG). When set, takes precedence over the text-driven layout.
  - `alt` — accessibility alt text. Defaults to `"Sponsored content from <product_name>"` when not provided.

  **Example entry:**
  ```json
  {
    "id": "trellis",
    "image_url": "https://syvrstudios.com/hum/promo-images/trellis.png",
    "alt": "Trellis — Guided AI creative platform",
    "url": "https://trellis.syvr.dev",
    "product_name": "Trellis",
    "tagline": "Guided AI creative platform.",
    "weight": 1,
    "active": true
  }
  ```
  The `product_name` / `tagline` / `cta_text` / `accent_color` fields are still respected when present (used for accessibility fallbacks and the text-driven path if the image fails to load), but only the image renders visually when `image_url` is set.

  **Recommended asset dimensions:** **1920×240 (8:1 aspect)** for crisp rendering at any overlay width on HiDPI displays. The card slot in the default overlay is roughly 770-1000px wide × ~100px tall; 1920×240 gives 2× pixel density at the default and stays sharp when users drag the overlay wider. Other aspects work — the image uses `object-fit: contain` so it letterboxes gracefully — but designing at the recommended aspect gives edge-to-edge fill with no empty bands.

  **Layout behavior:**
  - `three_line` (default) layout: image fills the lyric area's full width and height
  - `full_page` layout: same — image is the centerpiece
  - `single_line` layout: falls back to the text-driven path. The ~26px row height is too short for a hero image to read; the text layout still rotates from the same `promos.json` pool

  **Graceful degradation:** if the image URL 404s, is blocked by network policy, or otherwise fails to load, PromoCard catches the error and falls back to the text-driven layout for that promo. The user never sees a broken-image gap. Per-promo failure state is reset whenever the rotation picks a different promo.

  **Author workflow (Wes):**
  1. Design a card at 1920×240 (Figma / Photoshop / Canva).
  2. Export as PNG (or SVG for crisp scaling).
  3. Drop into `Websites/sites/syvr-site/public/hum/promo-images/<filename>.png`.
  4. Add `"image_url": "https://syvrstudios.com/hum/promo-images/<filename>.png"` to the entry in `Websites/sites/syvr-site/public/hum/promos.json`.
  5. `git push`. Vercel auto-deploys. Live on all Hum installs within 6 hours (or on next app launch).

  **Implementation:** `Promo` struct in `src-tauri/src/promos.rs` gained `image_url: Option<String>` + `alt: Option<String>`, both `#[serde(default)]` so existing JSON without these fields continues to parse unchanged. `Promo` type in `src/types.ts` mirrored. `PromoCard` in `src/Overlay.tsx` gained an early-return branch that renders `<img src={image_url} alt={alt} ... />` filling the card slot with `object-fit: contain`, gated on `image_url` being set AND layout being `three_line`/`full_page` AND image not having failed to load. The text-driven render path remains untouched as the fallback.

## [0.12.4] - 2026-05-22

### Fixed
- **Clicking the album cover now actually opens the artist info panel** with the artist's bio, photo, and tour dates (the panel that was designed in v0.10.x and shipped broken). Before this release, clicking the album cover spawned a 360×480 always-on-top window titled with the dev-console heading and showing only a blank black background — confusing and unusable. Root cause: the Tauri webview was asked to load `artist-panel/index.html` but Vite's multi-page build preserves the input path, so the entry actually lives at `src/artist-panel/index.html`. The wrong URL 404'd, Tauri fell back to the root `index.html`, `main.tsx::pickComponent()` saw the unrecognized `"artist-info"` window label and defaulted to rendering the `DevConsole` component inside the panel window. Fixed by correcting the URL path in `src-tauri/src/artist_window.rs` to `src/artist-panel/index.html`.

  **What you'll see now:** click the album cover → a small panel slides in below (or above, if no room below) the overlay window with the artist's photo, bio (Wikipedia), and any upcoming tour dates with ticket links. The panel auto-closes when the track changes to a different artist, and reopens on the next click.

## [0.12.3] - 2026-05-22

### Fixed
- **Overlay correctly returns to the lyric view when a Spotify ad break ends.** v0.12.2 and earlier left the overlay stuck on the SYVR promo card with the AD BREAK chip after the ad break completed — even with the real song actively playing (e.g. "One Man Band — Old Dominion" 3:06 in the Spotify player while Hum still showed "Brought to you by SYVR Studios — Trellis — Try free"). Root cause: the SMTC ad-detection in `src-tauri/src/smtc.rs::emit_blended` was set-only (`if is_spotify_ad { snap.ad_active = true; }`) and never cleared the flag. When MediaChanged fired for the new real song, the cloned snapshot inherited `ad_active = true` from the prior ad, `is_spotify_ad` returned false for the real song (~3-min duration trips the duration heuristic's exclusion), but the conditional didn't execute → the flag persisted across the transition and the shared-snapshot sync kept writing `true` to the resolver.

  **Fix:** replaced the set-only conditional with explicit set/clear semantics. When `is_spotify_ad` matches → set true. When the source is Spotify AND duration ≥ 35s (i.e. a confident real song) → clear to false. Non-Spotify SMTC sources (Chrome with a Pandora tab, iTunes, etc.) leave `ad_active` alone — the bridge worker in `web_bridge.rs` owns the flag for those via its own emit-and-sync path, and an unconditional clear here would clobber legitimate Pandora-web ad detections.

### Changed
- **Album art square and blurred-background tint are now hidden during an ad break.** Before this release the prior song's album cover stayed visible on the left of the overlay and its dominant-color tint continued blurring the background through the entire ad break, making it visually look like "your song is still playing." Now when `lyrics.status === "ad"`, both `AlbumArtSide` and `BlurredAlbumBg` are skipped — the PromoCard becomes the visual focus on the left and the background reverts to whatever your `bg_color` / `bg_opacity` settings normally produce (typically transparent or a flat dark). When the ad ends and the next song loads its art, both come back.

  **Implementation:** added a single `adActive = lyrics?.status === "ad"` derived flag in `src/Overlay.tsx`, gated both `showArt` and `showBlurBg` on `!adActive`.

## [0.12.2] - 2026-05-22

### Fixed
- **First ad of an ad break now reliably swaps to the SYVR promo card.** v0.12.1 fixed ads 2+ but the FIRST ad of a Spotify ad break still showed `"♪ no lyrics for —"` with just the AD BREAK chip firing alone. Root cause: Spotify's SMTC fires `MediaChanged` for the first ad before the duration metadata fully loads (initial `duration_ms = 0`), then `TimelineChanged` arrives ~hundreds of ms later with the real `duration_ms = ~15-30s`. The duration heuristic in `is_spotify_ad` doesn't match on the first wake (duration is 0), so `ad_active` stays false → the lyrics resolver runs LRCLib for the garbage title `"—"` → emits `status: "not_found"`. The subsequent TimelineChanged correctly sets `ad_active = true` on the shared snapshot, but the resolver wasn't subscribed to `timeline-changed` events — so it never woke to consult the fresh state until the NEXT track-change (i.e., ad 2). Subsequent ads worked because by then Spotify is in "ad mode" and full metadata is available on the first MediaChanged.

  **Fix:** the lyrics resolver in `src-tauri/src/lyrics.rs::start` now also subscribes to `timeline-changed` events. Dedupe via `last_key` keeps the per-tick cost trivial during normal song playback (the resolver wakes ~1Hz, reads the snapshot, sees the same track key, and continues without doing any work). When ad_active flips on the late-arriving TimelineChanged, the resolver wakes, sees the new state, and emits the `Status::Ad` outcome with the picked promo.

## [0.12.1] - 2026-05-22

### Fixed
- **Spotify third-party ads (Hotels.com, TikTok, BINI promos, etc.) now actually swap the overlay to the SYVR promo card.** v0.12.0 only detected Spotify's own house ads (the "Listen to music, ad-free" prompts) because the heuristic looked for literal `"Advertisement"` / `"Spotify"` strings in the SMTC title/artist fields. Third-party ads come through with arbitrary titles like `"—"` (em-dash) or `"LISTEN NOW"` paired with the advertiser's brand name in the artist slot, and weren't matched. The detector now also flags any Spotify-sourced playing track with `duration_ms < 35_000` as an ad — real Spotify songs are virtually never under ~60s, so the new heuristic catches third-party ads without false-positive on real songs. Five new unit tests cover the duration cases.

  **False-positive caveat:** legitimate sub-35s tracks (intro tracks, skits, sound effects) would be mis-classified as ads. Rare on Spotify; acceptable trade-off.

- **SYVR promo card now renders during ad breaks instead of just the AD BREAK chip firing alone.** v0.12.0 shipped with a race where the AD BREAK chip (in the right-side metadata column) would correctly show during an ad, but the lyric area would keep showing `"♪ fetching"` or `"♪ no lyrics for —"` instead of the SYVR promo card. Root cause: the `emit_blended` helper in `src-tauri/src/smtc.rs` was mutating its own local copy of the snapshot to set `ad_active = true`, but never wrote that flag back to the shared snapshot. The frontend's `track` state (read from the emit payload) correctly received `ad_active = true` → AD BREAK chip fired. The lyrics resolver (which reads the shared snapshot, not the emit payload) saw stale `ad_active = false` → didn't short-circuit → went through LRCLib resolution → emitted `status: "fetching"`/`"not_found"`. The PromoCard render condition (`lyrics.status === "ad"`) never matched. Fixed by writing `snap.ad_active` to the shared snapshot inside `emit_blended` after both the Spotify heuristic AND the bridge blend.

- **Same race fixed in the bridge worker's emit path** (`src-tauri/src/web_bridge.rs`). The bridge worker (drives Pandora desktop + Pandora web + YouTube probes) was building a locally-blended snapshot for its `timeline-changed` emits without syncing `ad_active` back to the shared snapshot. Now it writes `blended.ad_active` after the blend so the resolver sees the flag through the same code path.

- **Ad-end transition for bridge sources** — `blend_bridge_into_snapshot` now explicitly clears `snap.ad_active = false` when the bridge reports a real (non-ad) track. Previously the flag only flipped to true on `bt.is_ad`, never back to false, so a Pandora ad → next song transition would leave the overlay stuck on the promo card.

  **What you'll see fixed:** Spotify free ads (any kind) now swap to the SYVR promo card within the snapshot tick of the ad starting. When the ad break ends and music resumes, the card disappears and lyrics resume. Same flow for Pandora web/desktop and YouTube ads.

## [0.12.0] - 2026-05-22

### Added — Ad-break detection + SYVR cross-promo overlay (full feature)

This is the consolidated user-facing release entry for v0.12.0. Sub-entries 0.12.0-rc1 through 0.12.0-rc8 below this entry document the per-commit work; the bullets below summarize what's new for the user.

- **Ad-break detection across Spotify, Pandora web, Pandora desktop, and YouTube web.** When an ad break is detected, the overlay's lyric area is replaced with a rotating SYVR Studios product promo card. The right-side metadata column stays visible so users see how much of the ad is left via the progress bar — the source-app badge swaps to an amber `AD BREAK` chip in place of `SPOTIFY` / `PANDORA` / `CHROME`. The Artist · Song · Album line is hidden during ads (showing the previous track's title would be misleading; the AD BREAK chip + promo card already explain what's happening).

- **Promo cards rotate one product per ad break, weighted-random with last-shown cooldown.** Each card shows: a small "Brought to you by SYVR Studios" supertitle, an optional 32×32 product icon, the product name (same size as a lyric line), a dim tagline, and a clickable CTA. Default CTA is `Learn more →`; per-promo `cta_text` overrides are supported. Clicking anywhere on the card opens the promo's URL in the default browser. In edit mode the card is a window drag region instead of a click handler, mirroring the rest of the overlay's chrome behavior.

- **Promo list lives at `https://syvrstudios.com/hum/promos.json` — hot-swappable without a Hum release.** Updates to the file go live on the next refresh (every 6 hours; immediate on app launch). Disk-cached at `%APPDATA%\com.syvr.hum\promos.json` so the first ad break of a session always has something to show even before the network call returns. Bundled `default_promos.json` ships with the app as the final fallback.

- **New Settings toggle: "Show SYVR promo cards during ad breaks"** (default on). Toggling off shows a neutral "Ad break" centered text in place of the promo card. The progress bar + AD BREAK chip still show ad timing regardless.

### Limitations

- **YouTube ad detection requires the YouTube tab to be the foreground Chrome window.** Background-tab YouTube ads are not detected. Implementation reuses the same UIA tree-walking pattern as the Pandora bridges — the Chromium accessibility tree is only fully populated for the active tab.
- **Pandora ad position interpolation falls back to a 30-second default duration** when the ad countdown widget can't be read from the UIA tree. The progress bar will jump to "complete" when the next real track is detected, which is acceptable for free-tier Pandora (most ads are 30s anyway).
- **Spotify Premium users never see ads** so the promo card path is unreachable for them. Not a bug — by design.

### Phase 2 (deferred — premium feature)

User-supplied promos are designed for but not built. The architecture supports them (the `PromoSource` trait, the in-memory pool model), so they slot in cleanly as a paid feature for streamers and small-business owners who want their own promo content during music ad breaks. Tracking in the spec at `docs/superpowers/specs/2026-05-22-hum-ad-break-detection-design.md`.

---

## [0.12.0-rc8] - 2026-05-22

### Added
- **Pandora web ad-break detection.** Free-tier Pandora.com ads in Chromium browsers (Chrome, Edge, Brave, Opera, Vivaldi) now also trigger the SYVR promo card with countdown. Shares the same `classify_pandora_state` decision helper as the desktop probe (Task 7) — the only difference is where the UIA tree comes from (Chrome's tab vs the Pandora.exe window).

  **Implementation:** Extended `PandoraProbe::read` in `src-tauri/src/web_bridge.rs`. Added a new `collect_pandora_web_data` function that walks the Chrome accessibility tree in a single DFS pass collecting: class-based track/artist/album text (previously three separate walks), Pandora Hyperlink URLs (for ad classification via `classify_pandora_state`), and countdown text (`M:SS` regex match). Replaces the previous three separate `find_text_by_class_substr` calls — eliminating the triple-DFS performance hazard flagged in BUGS.md. Constructs an `is_ad: true` `WebBridgeTrack` when no `/TR…` URLs are present. `position_ms` mirrors Task 7's simpler approach (always 0) — initial-duration caching deferred for the same reason as the desktop probe. Manual live verification skipped — no Pandora Free available; the shared classifier is covered by Task 7's unit tests.

## [0.12.0-rc7] - 2026-05-22

### Added
- **Pandora desktop ad-break detection.** When the Pandora Microsoft Store app plays an ad break, Hum now switches the overlay to the SYVR promo card and shows the ad's countdown in the progress bar (when the countdown widget is readable from the UIA tree). Detection: the existing DFS-walk of Pandora's accessibility tree no longer requires `/artist/...TR{id}` Hyperlinks — when none are present but the window is still visible, the probe declares an ad break and looks for the player's countdown text (matches `M:SS`) to surface as ad position + duration.

  **Implementation:** New `classify_pandora_state` helper in `src-tauri/src/pandora_desktop.rs` that combines URL collection with countdown parsing into a single decision struct (`PandoraStateResult { is_ad, countdown_seconds }`). New `collect_pandora_uia_data` DFS function collects both the Pandora Hyperlink URLs and any countdown-shaped text node (`^\d+:\d{2}$` via cached `OnceLock<Regex>`) in one pass. `WebBridgeTrack` gained `is_ad: bool` + `duration_ms: Option<u64>` fields. `blend_bridge_into_snapshot` maps both onto the snapshot via a fast ad-path that bypasses the title-empty guard. Countdown parse fallback: when the countdown can't be read, ad duration defaults to 30 seconds and position stays 0 (progress bar shows 0% — documented limitation). Five unit tests cover the classifier in `pandora_desktop::ad_detection_tests`. Manual live verification skipped — no Pandora Free available; classifier verified independently via unit tests.

## [0.12.0-rc6] - 2026-05-22

### Added
- **Spotify ad-break detection.** Spotify's free tier ad breaks now automatically trigger the SYVR promo card. Detection runs entirely off SMTC metadata heuristics with zero new permissions, processes, or APIs. Fires when the source is Spotify (matched on `source_app_id` containing `spotify`) AND one of: title is `Advertisement` or `Spotify` (case-insensitive); artist is `Spotify` with non-empty title; OR both title and artist are empty while Playing (rare but observed for cold-started sessions). Real Spotify songs (`title = "Mr. Brightside"`, etc.) are never matched. Spotify Premium users never see ads and never trigger this path.

  **Implementation:** New `is_spotify_ad` helper in `src-tauri/src/smtc.rs`. Called from `emit_blended` before the tier-1 priority check so the flag rides on the snapshot regardless of state. Nine unit tests cover positive matches (Advertisement / Spotify title, Spotify artist, empty-while-playing, AUMID), negatives (real song, paused empty, non-Spotify source), and case-insensitivity. Manual live verification skipped — no Spotify Free available; heuristics verified independently via unit tests.

## [0.12.0-rc5] - 2026-05-22

### Added
- **SYVR promo card replaces the lyric area during ad breaks** (still no real detection — pending Tasks 6-8). When `lyrics.status === "ad"`, the prev / cur / next lyric rows in the three-line and single-line layouts (or the scrolling column in full-page) are swapped out for a stacked card showing: a small `Brought to you by SYVR Studios` supertitle, an optional 32×32 product icon, the product name (same size as the current lyric), a dim tagline, and a clickable CTA (defaults to `Learn more →`). The card is fully clickable in locked/ghost modes — opens the promo's URL in the default browser via the new `tauri-plugin-opener`. In edit mode the card is a drag region instead. Right-side metadata column behavior: the Artist · Song · Album line is hidden during ads; the source badge swaps to an amber `AD BREAK` chip with the same shape as the existing source badges; the progress bar + time readout stays so users can see how much of the ad is left.

  **Implementation:** New `PromoCard` and `AdBreakChip` components in `src/Overlay.tsx`. `MetadataColumn` gained an `adActive: boolean` prop wired from `track.ad_active`. Added `tauri-plugin-opener` (Rust + JS sides) plus `opener:default` / `opener:allow-open-url` capabilities. PromoCard renders in all three layouts (`three_line` stacked, `single_line` inline-collapsed, `full_page` centered stacked).

## [0.12.0-rc4] - 2026-05-22

### Added (internal — promo data wired end-to-end, still no detection)
- **Promo rotation engine fetches from `https://syvrstudios.com/hum/promos.json` on startup and every 6 hours.** Falls back to a disk cache at `%APPDATA%\com.syvr.hum\promos.json`, then to a bundled `default_promos.json`, then to a hardcoded SYVR Studios entry. Picked promo rides on the `CurrentLyrics` payload's new `promo` field whenever `status == Ad`. Cooldown state (last-shown id) lives in app-managed shared state. No detection yet; verified manually with a temporary force-ad hack on Spotify.

  **Implementation:** New `PromoSource` trait + `SyvrRemoteSource` in `src-tauri/src/promos.rs`. Bootstrap is synchronous (reads disk before the event loop starts); refresh is a background tokio task. `tauri-plugin-store` not used — promos cache is a plain JSON file via `std::fs::write` because the existing store plugin's API is overkill for one file. `ad_break_outcome` in `lyrics.rs` is now async and consults the rotation engine. Promo type mirrored to `src/types.ts`.

## [0.12.0-rc3] - 2026-05-22

### Added (internal — no user-visible behavior yet)
- **Promo rotation engine + bundled default promos.** New `src-tauri/src/promos.rs` module defines the `Promo` schema (id, product_name, tagline, url, optional icon_url / weight / active / cta_text / accent_color) and the `pick_next_promo` helper. Weighted-random selection with last-shown cooldown that gracefully degrades when only one active promo exists. Bundled `src-tauri/resources/default_promos.json` ships with one SYVR Studios fallback entry. No fetch, no UI integration yet — those land in Tasks 4 and 5.

## [0.12.0-rc2] - 2026-05-22

### Added (internal plumbing — no user-visible behavior yet)
- **Lyrics resolver short-circuits when ad_active is set.** When the snapshot has `ad_active = true`, the resolver emits a `CurrentLyrics` with `status = Ad`, empty lines, and zero network calls. No detector is wired up yet (that's Task 4+); this is the resolver-side path. Verified manually by forcing `ad_active = true` for Spotify and confirming the overlay's placeholder status line renders.

  **Implementation:** New `ad_break_outcome(snap)` helper in `src-tauri/src/lyrics.rs` that synthesizes the payload. Wired into the resolver loop in `start()` ahead of the bridge consultation. Track-key namespaced as `ad|<app>|<duration>` so consecutive ads on the same source don't dedupe-skip.

## [0.12.0-rc1] - 2026-05-22

### Added (internal plumbing — no user-visible behavior yet)
- **`ad_active` flag on the current-track snapshot + `Status::Ad` lyrics variant.** No user-visible behavior in this commit — this is the data-path scaffolding for the ad-break detection feature (spec: `docs/superpowers/specs/2026-05-22-hum-ad-break-detection-design.md`). When `lyrics.status === "ad"`, the overlay currently shows a temporary "♪ ad break — promo coming in Task 5" status line; the real SYVR promo card lands in a follow-up commit.

  **Implementation:** Added `ad_active: bool` (defaults false, serde `#[serde(default)]` for backwards compatibility) to `CurrentTrack` in `src-tauri/src/smtc.rs`. Added `Ad` variant to the `Status` enum in `src-tauri/src/lyrics.rs`. Mirrored both in `src/types.ts`. Frontend `statusLine` function has a placeholder branch for `"ad"`.

## [0.11.10] - 2026-05-22

### Fixed
- **Overlay window can no longer be dragged so small it clips the lyrics or the new right-side metadata column.** Before this release the overlay window had no enforced minimum dimensions, so dragging the resize edges aggressively could shrink the window past the point where the lyric text fit (only the current line would show, prev/next clipped off) or where the right-side metadata column from v0.11.9 fit (only the time readout visible, artist line + source badge cropped off the right edge). The window now refuses to shrink past **520 × 110** logical pixels, which is enough room for the three-line lyric stack at default font size plus the metadata column's artist line, progress bar, and source badge with breathing room. Drag-resize past that point simply stops; the window holds at the minimum.

  **Implementation:** Added `"minWidth": 520, "minHeight": 110` to the `overlay` window block in `src-tauri/tauri.conf.json`. Mirrors the settings window's existing min-size pattern (lines 44-45 of the same file).

## [0.11.9] - 2026-05-22

### Added
- **Right-side metadata column shows track info, progress, and source app.** The previously-empty area to the right of the lyric text in the 3-line and single-line layouts now stacks three small read-only widgets, vertically centered, right-aligned, in the current dim text color. The dim color follows the auto-contrast flip the same way the lyric color does — it's the same `text_color_dim` setting used for the prev / next dim rows.

  **What appears, top to bottom:**
  1. **Artist · Song · Album** line. ~11px, dim, single-line with ellipsis if the joined string overflows. Hovering the line shows the full text as a tooltip via `title` attribute. Empty fields are skipped, so "Artist · Song" renders cleanly when the SMTC source doesn't publish an album.
  2. **Progress bar + time readout.** A 160px-wide row showing `m:ss` current position on the left and `m:ss` total duration on the right (10px, tabular-nums), with a 2px-tall track underneath them. The filled portion uses the active lyric color at 85% opacity; the empty portion is `rgba(127,127,127,0.35)`. Position interpolates client-side against wall time every 500 ms while playing (Windows SMTC only pushes timeline updates every 2 s, so the bar would otherwise jump in 2-second steps), and freezes at the reported position when paused / stopped. The ticker only runs while `state == "playing"`, so a paused / closed app doesn't pay for a wake every half-second.
  3. **Source badge.** A small uppercase pill (9.5px, letter-spaced, rounded) showing which app the metadata is coming from. Possible values include: `SPOTIFY`, `PANDORA`, `ITUNES`, `APPLE MUSIC`, `YOUTUBE MUSIC`, `TIDAL`, `AMAZON MUSIC`, `DEEZER`, `VLC`, `FOOBAR2000`, `MUSICBEE`, `WINAMP`, `WINDOWS MEDIA`, `GROOVE`, `CHROME`, `EDGE`, `FIREFOX`, `BRAVE`, `OPERA`, `ARC`, `ZEN`. Unrecognized app IDs fall back to a best-effort capitalized basename (`.exe` stripped, AUMID `Publisher.AppName_hash` simplified to `AppName`). The badge hides entirely when there's no `source_app_id` rather than showing a raw path.

  **Layout behavior:** The new column sits inside the same flex row as the lyrics, with `flex-shrink: 0` and `max-width: 38ch`, so a long Artist · Song · Album doesn't push the lyrics to nothing. The lyrics column still takes its remaining flex space — when lyric text is short, the empty space between the lyric end and the metadata column is the layout's natural gap; when lyric text is long, it ellipsis-truncates first (preserving the metadata column's full width). In edit mode the metadata column is also a `data-tauri-drag-region`, so the user can grab the window from the new area too.

  **Layouts touched:** `three_line` (default) and `single_line`. `full_page` is unchanged this release — it has no "right side" since the lyrics scroll vertically through the whole window.

  **Implementation:** New `MetadataColumn`, `ProgressBar`, `SourceBadge` components and a `sourceLabel(appId, override)` helper in `src/Overlay.tsx`. New `progressTick` state + a `useState`-bound `setInterval(500ms)` whose only job is to force a re-render while `track.state === "playing"` — the bar itself computes interpolated position inline at render from `track.position_ms + (Date.now() - track.last_update_unix_ms)`. The `fmtMs` helper was already exported from `src/types.ts` (it powers the lyrics-fetching status line) and is reused here for the `m:ss` formatting. Bridge-source override prop (`source`) is plumbed through but currently passed `null` everywhere — surfacing the bridge label ("pandora-web" / "pandora-desktop") through to the badge requires a Rust change to add a `bridge_source` field on `CurrentTrack`, which is deferred to a follow-up. For now the badge reflects the OS-reported app (so Pandora-in-Chrome shows `CHROME`, Pandora desktop shows `PANDORA` via its package name).

## [0.11.8] - 2026-05-22

### Fixed
- **Lyric text no longer flips to black on tinted-but-bright surfaces.** Before, when the auto-contrast feature decided the surface under the lyrics was "light" it switched the text from white to black. The rule was a pure luminance check, so a tan / gold / pale-pastel tint coming from an album-art blurred background (e.g. Yellowcard, Nelly Furtado, anything with a warm cover) tripped the threshold even though black text on tan is genuinely hard to read. The text now stays white over tinted brights, and only flips to black on near-grayscale lights — pure white, light gray, pale cream, very faint pastels.

  **Implementation:** The contrast worker (`src-tauri/src/contrast.rs`) was already emitting RGB along with luminance via the `bg-luminance` Tauri event, but the React side discarded the color components and used only luminance. Renamed the React state to `screenColor` and plumbed RGB through `computeSurfaceColor` (was `computeSurfaceLuminance`) so the final composited surface is an `{r,g,b}` object instead of a scalar. Derive both luminance (Rec. 601 weighted) and HSV saturation `(max - min) / max` from the composite, then compute a single `lightnessScore = luminance × (1 - saturation)`. Pure white scores 1.0; light gray ~0.8; tan / gold tints ~0.4; saturated colors < 0.3. Dark-text threshold is now `lightnessScore > 0.60`, with hysteresis at 0.55 / 0.65 to suppress flicker when dynamic backgrounds hover near the boundary.

### Updated
- **README source-compatibility table.** Replaced the vague "anything that registers with SMTC" line with a concrete table of confirmed-working sources (iTunes app, Spotify app + web, YouTube web, Pandora web) plus the semi-working Pandora desktop app entry with its known limitation. README's tech-stack table also gained rows for the Pandora-web and Pandora-desktop bridges.

## [0.11.7] - 2026-05-22

### Fixed
- **Pandora pause now actually freezes the lyrics.** v0.11.6 attempted pause detection by reading the play button's `TogglePattern` from UIA, which turned out unreliable in practice — Pandora's React shell either doesn't expose the pattern at all or reports a stale value, so the bridge kept reporting `Playing` while audio was silent. Replaced with a WASAPI-based signal: query the Windows audio session for the Pandora.exe process and check the peak meter. Peak below `0.0001` (effectively silent) → `Paused`; nonzero → `Playing`. This is the same surface Windows itself uses to draw the per-app volume meter in the system mixer, so it tracks actual audio output rather than any app-published flag.

  **Behavior:** Pause Pandora → within 2 seconds the bridge reports `Paused`, the snapshot's state flips to `paused`, and the overlay's wall-clock interpolation halts. Resume → the bridge re-anchors `period_start_unix_ms` to "now" and the cumulative played-ms keeps advancing from where it stopped. Track changes still reset cumulative to 0.

  **Fallback:** WASAPI is the primary signal. If session enumeration fails to find a session for the Pandora PID (rare — would mean the app hasn't requested audio from the default render endpoint yet, e.g. during a cold start), the probe falls through to the prior UIA `TogglePattern` + Name-fallback logic. Worst case: bridge defaults to `Playing`, same as v0.11.4-v0.11.6 behavior.

  **Implementation:** New `pandora_desktop::is_process_audio_silent(pid)` opens `MMDeviceEnumerator → eRender → eMultimedia` default endpoint, activates `IAudioSessionManager2`, enumerates `IAudioSessionEnumerator`, matches each session's `IAudioSessionControl2.GetProcessId()` against the Pandora window's owning PID, then casts the matching session to `IAudioMeterInformation` and reads `GetPeakValue()`. `detect_playback_state_with_audio` is the new entry point that tries WASAPI first, then falls back to the existing UIA helper (renamed to `detect_playback_state_via_uia`). New Cargo features: `Win32_Media_Audio`, `Win32_Media_Audio_Endpoints`, `Win32_System_Com`.

  Verified: 67 unit tests pass; cargo check + build clean.

## [0.11.6] - 2026-05-22

### Fixed
- **Pandora pause is now respected; another player taking over (Spotify / iTunes / YouTube via Chrome) properly switches the overlay.** Follow-up to v0.11.5: pausing the Pandora desktop app no longer leaves the lyrics scrolling forward as if the song were still playing, and starting a different player while Pandora is paused now correctly switches Hum to the new source.

  **What you'll see when you pause Pandora:** Hum freezes the current lyric line in place. Wall-clock interpolation stops advancing position because the snapshot's playback state is now `Paused`. On resume, lyrics pick up from the same line — the bridge tracks cumulative played-ms (excluding paused stretches) per track, so pause/resume cycles don't desync.

  **What you'll see when you start a different player while Pandora is paused:** Hum switches to the new player within ~2 seconds. Spotify, iTunes, YouTube-in-Chrome — anything that publishes to Windows SMTC with `state == Playing` — now wins over a paused-but-still-running Pandora desktop.

  **What you'll see when you start a different player while Pandora is still playing:** Same — the new player wins. SMTC-publishing apps always beat Pandora desktop because their published position is real-time and accurate, while Hum's Pandora-desktop position is necessarily estimated (Pandora's seek bar is not exposed to UI Automation).

  **Known limitation that did NOT change:** the position estimate for Pandora desktop is still anchored at "the first time Hum saw this track in the UIA tree." If Hum starts after a Pandora track has already been playing for a minute, the lyrics will scroll from 0:00 instead of from 1:00. This is unsolvable without a seek-bar surface; switching tracks resets the anchor to 0 and re-syncs.

  **Implementation:**

  - `pandora_desktop::detect_playback_state` walks the Pandora window's UIA subtree for a `Button` named `"Play"` or `"Pause"`. Tries `TogglePattern.get_toggle_state()` first (Pandora's React shell sets `aria-pressed` which UIA exposes via that pattern); falls back to interpreting the button's `Name` (`"Pause"` → playing, `"Play"` → paused). When the button can't be found at all, the probe defaults to `Playing`.
  - `pandora_desktop::update_track_state` is a per-track state machine (`Mutex<Option<TrackPlayState>>`) holding `(track_key, cumulative_ms, period_start_unix_ms, last_state)`. Transitions: `Playing→Paused` adds elapsed ms to cumulative and freezes; `Paused→Playing` re-anchors `period_start`; `Playing→Playing` reports `cumulative + (now - period_start)`. New track keys reset everything to 0.
  - `WebBridgeTrack` gains `state: Option<crate::smtc::PlaybackState>`. The Pandora desktop probe sets it from `update_track_state`; the Chrome `PandoraProbe` leaves it `None` because SMTC's state is already correct for Pandora-in-Chrome.
  - `blend_bridge_into_snapshot` now copies the probe's state into the snapshot (was previously hard-forced to `Playing`).
  - `bridge_is_authoritative` returns `false` when the bridge's state is `Paused`, so SMTC's emits can take over for the new player.
  - `smtc::emit_blended` now has a 3-tier priority: (1) SMTC with `state == Playing` and non-empty title wins outright, (2) bridge takes over when authoritative, (3) raw SMTC otherwise. The bridge worker's parallel `timeline-changed` emit also yields when SMTC is actively playing, so the two streams don't race.

## [0.11.5] - 2026-05-22

### Fixed
- **Lyrics now scroll in sync when playing through the Pandora desktop app.** Follow-up to v0.11.4: the Pandora desktop bridge correctly identified the track + artist (so the album art and lyric-lookup were right), but the overlay's current-line indicator was jumping around because Hum was still reading playback position from Windows SMTC — which was stuck on whatever iTunes (or another app) last published. Pandora's seek bar does not surface to UI Automation at all, so Hum now estimates position by recording the unix-ms when each new Pandora track was first seen and reporting elapsed-since-start each subsequent poll. Lyrics now advance smoothly from the song's beginning. Pausing the Pandora app will still cause the lyrics to drift forward (Hum can't detect the pause because the seek bar is invisible to UI Automation); the drift resets on the next track change.

  **Implementation:** Added `position_ms: Option<u64>` to `WebBridgeTrack`. Probes that can determine position (`PandoraDesktopProbe`) set it; probes that can rely on SMTC's position (`PandoraProbe` for Pandora-in-Chrome) leave it `None`. `pandora_desktop.rs` keeps a `Mutex<Option<(String, i64)>>` of `(track_key, start_unix_ms)` and computes `position_ms = now - start_ms` whenever it re-detects the same track; switching tracks resets the anchor to 0.

  Made bridge data visible to the frontend in two new places:
  - `get_current_track` Tauri command now blends bridge data into the snapshot before returning (was previously raw snapshot only) so the overlay's initial mount sees Pandora's track + position instead of SMTC's stale iTunes data.
  - The `web_bridge` worker now emits `timeline-changed` events with a blended snapshot on every poll that produced position data, so the overlay re-syncs every 2 seconds while Pandora is playing.

  Stopped SMTC's emits from yanking the frontend back to iTunes between bridge polls: every `app.emit("track-changed" / "timeline-changed" / "playback-state-changed", ...)` site in `smtc.rs` now goes through a new `emit_blended` helper that overrides the emit payload with fresh bridge data when present. SMTC still owns the snapshot's writeable fields when bridge is stale; the blend only kicks in for the 5-second freshness window after each bridge read. New helper `web_bridge::blend_bridge_into_snapshot` is the single source of truth for the override rules. Updated the `any_probe_detects_aggregates_correctly` unit test to test `PandoraProbe` in isolation since the aggregator's result now legitimately depends on whether `Pandora.exe` is currently running.

## [0.11.4] - 2026-05-22

### Added
- **Pandora desktop app is now a supported source.** Hum's overlay now shows the correct lyrics when you're playing music through the Microsoft Store Pandora app (the `Pandora.exe` Chromium-shelled desktop client), rather than incorrectly showing lyrics for whatever app last published to Windows SMTC (commonly iTunes, leaving the overlay stuck on stale tracks). No setting to enable — works automatically the moment Pandora.exe is open with a visible window and a track is playing. The overlay swaps to the new track within ~2 seconds of any Pandora track change.

  **Why this needed a dedicated bridge:** Pandora's desktop app does not publish to Windows SMTC at all. Hum's pre-existing fallback chain (SMTC → Chrome `web_bridge` for Pandora-in-Chrome) didn't cover the desktop variant, and the SMTC-gated Chrome probe rejects it on the app-id check. The new bridge ignores SMTC entirely and gates on process enumeration instead.

  **Implementation:** New `src-tauri/src/pandora_desktop.rs` registers `PandoraDesktopProbe` as the second entry in `web_bridge.rs::PROBES`. Its `detects()` enumerates visible top-level windows via Win32 `EnumWindows` looking for `process_name == "Pandora.exe"` (rather than reading SMTC). Its `read()` re-anchors the matched HWND through `automation.element_from_handle(hwnd)` to trigger Chromium's accessibility tree, then does a DFS preorder walk of the control-view subtree looking for `Hyperlink` elements whose `ValuePattern` URL starts with `https://www.pandora.com/artist/`. Classification by the URL's last path segment's two-character prefix: `TR` → track (use `Name` as title), `AR` → artist, `AL` → album. First hit of each kind wins (now-playing block renders before similar-artist Hyperlinks in document order). The `/artist/lyrics/...` "See All Lyrics" link is explicitly rejected so its `Name="See All Lyrics"` string never poisons the title field. Hard cap of 10,000 nodes per walk.

  Sits naturally in the existing `web_bridge` poll loop — when active, writes `WebBridgeTrack { source: "pandora-desktop", ... }` to the shared cache every 2 seconds; the lyrics resolver's 5-second freshness window picks it up over SMTC's stale data. 10 unit tests cover the URL classifier (track / artist / album acceptance, lyrics-URL rejection, wrong-host rejection, unknown-prefix rejection, empty/malformed-ID rejection, trailing-slash tolerance).

## [0.11.3] - 2026-05-22

### Changed
- **Artist bio now sourced from Wikipedia instead of Last.fm.** In the artist-info panel, the bio section (below the photo and artist name, above the Upcoming Shows section) now shows Wikipedia's encyclopedic summary for the artist rather than Last.fm's user-contributed biography. The prose style changes: Wikipedia bios are encyclopedic and factual ("X is an American rapper from Y, known for Z") rather than the more casual tone Last.fm bios could have. Bio text is still capped at 1,500 characters, trimmed to the last sentence boundary. The "Read more" link at the bottom of the bio section now reads **"Read more on Wikipedia →"** and opens the artist's Wikipedia article instead of their Last.fm profile page.

- **"Similar artists" section removed.** The gold "Similar to {artist1}, {artist2}, ..." line that appeared between the Bio and Upcoming Shows sections is gone. Wikipedia has no equivalent endpoint, so the section has been removed entirely. Users who want similar-artist discovery should use Spotify's radio/mix features or Last.fm directly.

- **Footer attribution updated.** The attribution row at the very bottom of the artist-info panel now reads **"Powered by Ticketmaster · Wikipedia · TheAudioDB"** (was "· Last.fm ·"). The Wikipedia name links to `https://wikipedia.org`.

  **Why:** Last.fm permanently WAF-blocked the account used to register an API key, so the bio section could never be activated at ship. Wikipedia's REST API requires no authentication and no API key — a `User-Agent` header is sufficient per their TOS.

  **Technical:** New `fetch_wikipedia_bio` in `src-tauri/src/artist_info.rs` calls `https://en.wikipedia.org/api/rest_v1/page/summary/{artist}`. Accepts pages only when `type == "standard"` and the `description` field (e.g. "American rapper", "English rock band") contains at least one music-relevance keyword (musician, singer, rapper, songwriter, band, group, dj, producer, composer, musical, music, vocalist, guitarist, drummer, bassist, pianist, rock, pop, hip hop, hip-hop, country, jazz, metal, indie, electronic, r&b, soul, folk). If the direct lookup fails the gate, retries with disambiguator suffixes in order: `(musician)`, `(singer)`, `(rapper)`, `(band)`, `(rock band)`, `(group)`. Removed `fetch_lastfm_bio`, `fetch_lastfm_similar`, `fetch_lastfm_bio_by_mbid`, `resolve_mbid_musicbrainz`, and the MusicBrainz mbid-retry orchestration block. `ArtistBio.lastfm_url` renamed to `ArtistBio.wikipedia_url` in Rust and TypeScript. `ArtistInfo.similar_artists` and `ArtistInfo.mbid` fields removed from the public struct, cache struct, and TypeScript types.

## [0.11.2] - 2026-05-22

### Added (dev-only, not shipped to users)
- **`dump_uia` dev binary — discover UI Automation selectors for non-SMTC apps.** New stand-alone binary at `src-tauri/src/bin/dump_uia.rs` that prints the UIA tree of every visible top-level window whose title or process file name matches a needle (default `"pandora"`, case-insensitive substring). Used to find the AutomationId / Name / ClassName paths that hold the currently-playing track + artist inside apps that do not publish to Windows SMTC, so we can build a Layer-3 universal-tracking bridge that mirrors `web_bridge::PandoraProbe` for desktop targets.

  Not part of the main `hum` binary. Run from `src-tauri/` with `cargo run --bin dump_uia` (pretty ASCII tree), `cargo run --bin dump_uia -- --json` (JSON array for grepping), or `cargo run --bin dump_uia -- spotify --raw` (custom needle + raw-view walker that shows every element including non-content shells). The default walker is the UIA control view, which matches the proven pattern in `web_bridge::PandoraProbe`. Per-element output includes `ControlType`, `LocalizedControlType` (when different), `Name`, `AutomationId`, `ClassName`, and the `ValuePattern` value where available. Hard caps at 20,000 nodes per window and depth 60.

  Critical implementation detail: each matched window is re-anchored through `automation.element_from_handle(hwnd)` rather than walked from the desktop root. This fresh-from-HWND query is what wakes Chromium / WebView2 / XAML hosts to expose their renderer subtree to UIA — without it, Chromium-backed apps return a 14-node shell with the actual content hidden behind a single empty `Chrome_RenderWidgetHostHWND` element. Same pattern as the working `web_bridge.rs` probe.

  No new dependencies — the `uiautomation = "0.25"` crate was already in `src-tauri/Cargo.toml` for the existing `web_bridge::PandoraProbe`. Cargo auto-discovers binaries in `src/bin/` so no manifest changes were needed.

### Changed (no user-visible effect this release)
- **Live Ticketmaster Discovery API key in place.** `TICKETMASTER_API_KEY` in `src-tauri/src/artist_info.rs` was populated with the live key from Wes's `SYVR-App` Ticketmaster developer account (originally landed as commit `ed23536` without a version bump in v0.11.1; documenting here for completeness). The artist-info panel's Upcoming Shows section now serves real tour data instead of empty / 401 responses. Free-tier limits: 5 requests per second, 5,000 requests per day per key.

## [0.11.1] - 2026-05-21

### Changed
- **Tour-date source swapped from Bandsintown to Ticketmaster.** The Upcoming shows section in the artist-info panel now sources events from the Ticketmaster Discovery API. The panel's appearance, behavior, and interaction model are unchanged — same row layout (date, city + region/country, venue, gold "[Tickets]" button), same "Sold Out" disabled variant, same 10-event cap with "View all on Ticketmaster →" link when more events exist. The footer attribution row at the bottom of the panel now reads "Powered by Ticketmaster · Last.fm · TheAudioDB" instead of "Powered by Bandsintown · Last.fm · TheAudioDB"; the Ticketmaster name links to ticketmaster.com.

  **Why:** Bandsintown's developer program signup was deprecated around 2022-2023, their `app_id` placeholder we shipped with was never officially registered, and the affiliate revenue side of their partner program is most likely no longer active. Ticketmaster's Discovery API is actively maintained (free tier: 5 requests/second, 5,000 requests/day), and their affiliate program runs on Impact (impact.com) with documented commissions (~2-4% of ticket face value).

  **Implementation:** `fetch_ticketmaster_events` in `src-tauri/src/artist_info.rs` calls `https://app.ticketmaster.com/discovery/v2/events.json?keyword={artist}&classificationName=music&size=50&sort=date,asc`. Results are validated against the requested artist using a case-insensitive `eq_ignore_ascii_case` check on the first attraction's name (mirrors the v0.10.26 art-validation pattern). Date parsing reuses the existing `parse_iso8601_to_unix_ms` helper against `localDate + "T" + localTime`. Ticket URLs are wrapped through an Impact affiliate prefix via `wrap_with_impact_affiliate`; the prefix is currently `None` (no affiliate credit), to be replaced post-Impact-signup with a real tracking template.

- **Ticket URL whitelist updated.** The `open_ticket_url` Tauri command's host whitelist (defense against cache-poisoned URLs) now accepts Ticketmaster regional domains (`ticketmaster.com`, `.ca`, `.co.uk`, `.de`) and Impact tracking subdomains (`*.go.impact.com`). Bandsintown's domain was removed. The command now also enforces `https` scheme as defense-in-depth.

### Pre-launch (Wes only)
- Sign up for Ticketmaster Discovery API key at `https://developer-acct.ticketmaster.com/user/register` (instant, self-serve) → replace `PLACEHOLDER_REPLACE_BEFORE_LAUNCH` in `src-tauri/src/artist_info.rs::TICKETMASTER_API_KEY`.
- Sign up for Impact affiliate platform at `https://impact.com`, join the Ticketmaster brand → replace `IMPACT_AFFILIATE_PREFIX = None` with the tracking URL prefix template.
- Last.fm API key signup remains open from v0.11.0.

## [0.11.0] - 2026-05-21

### Added
- **Artist-info panel — click album art to see bio, similar artists, tour dates, and buy tickets.** In edit and locked modes, clicking the album art square (visible in the 3-line and single-line layouts as the square image to the left of the lyrics; in the full-page layout as the small badge in the top-left corner) opens a new floating window showing information about the currently playing artist. When album art is hidden or unavailable, a small gold "•••" dot appears in the top-right corner of the overlay (same anchor as the update banner); hovering it expands an "Artist info" label, clicking it opens the panel. The panel is not available in ghost mode, consistent with ghost's "no chrome, click-through" design. The click affordance shows a 1.5px gold outline on hover so users know it is interactive; tooltip is omitted in the full-page layout where the badge is too small.

  The panel window (labeled `artist-info` internally) is 360×480px, transparent, always-on-top, no OS titlebar, and floats 8px below the overlay by default. If the overlay is within 500px of the screen's bottom edge, the panel anchors above the overlay instead. It can be dragged from its header to any screen position after opening. The panel closes via: the × button in the panel header, the ESC key, or automatically when the SMTC source switches to a different artist (same artist / new track keeps the panel open — the panel is artist-keyed, not track-keyed).

  **What the panel shows, top to bottom:**
  - **Header:** 60×60 round artist photo (from TheAudioDB) + artist name (18px, weight 600) + × close button (gold on hover). Header is the drag region.
  - **Bio section:** Last.fm artist bio prose, truncated at the last sentence before 1,500 characters. "Read more on Last.fm →" link. Section hidden entirely when bio is unavailable — no "no bio" placeholder.
  - **Similar artists section:** Up to 8 similar artists from Last.fm, comma-separated, prefixed with a gold "Similar to" section label. Section hidden when empty.
  - **Upcoming shows section:** Up to 10 upcoming tour dates from Bandsintown, sorted by date. Each row shows the date (gold, monospace, "Mar 5" format, year included only if not the current year), city+region (or city+country for international), venue in italic dim text, and a gold "[Tickets]" button right-aligned. Clicking Tickets opens the Bandsintown affiliate URL in the user's default browser — Bandsintown routes the click through Ticketmaster, SeatGeek, AXS, or Live Nation depending on the event. Sold-out events show a gray "[Sold Out]" non-clickable button instead. Empty state shows "No upcoming tour dates." in dim italic text. When more than 10 events exist, shows "View all on Bandsintown →" after the first 10.
  - **Footer:** "Powered by Bandsintown · Last.fm · TheAudioDB" in 10px dim centered text; each name is clickable and opens the respective service's website.

- **Affiliate ticket links via Bandsintown partner program.** Every ticket click from the panel routes through Hum's partner `app_id`. Affiliate revenue accrues to Wes on every user's click — no per-user setup required. The implementation ships with a `hum-dev` placeholder `app_id`; replace with the live partner ID from https://bandsintown.com/partners before public release.

- **Settings: "Show artist info panel" toggle** in Settings → Artist info panel section. Disabling it hides the click affordance on the album art and the fallback "•••" dot; any open panel closes. Below the toggle, a "Clear artist info cache" button wipes the on-disk artist cache (`%APPDATA%\com.syvr.hum\cache\artist\`).

### Architecture / files
- **New `src-tauri/src/artist_info.rs`** — All data types (`ArtistInfo`, `ArtistBio`, `TourDate`, `TicketStatus`); pure helpers `slug_for_artist` (diacritic-mapped, alphanumeric-only slug), `tour_dates_stale` (12-hour TTL check), `strip_html` (regex tag stripper + entity decode). Source fetchers: `fetch_lastfm_bio`, `fetch_lastfm_similar` (Last.fm REST), `fetch_bandsintown_events` (Bandsintown REST, ISO8601 date parser), `fetch_theaudiodb_photo` (TheAudioDB, base64-inline image), `resolve_mbid_musicbrainz` (MusicBrainz fallback). Orchestrator: `ArtistInfoCache` Tauri managed state — `fetch()` reads disk cache, returns immediately on fully-fresh data, refetches only tour dates when stale (≥12h), fires a full `tokio::join!` parallel fetch on cache miss with MusicBrainz fallback on Last.fm error 6. In-flight dedup via `Arc<Mutex<HashMap<String, Arc<Notify>>>>`. Disk cache at `%APPDATA%\com.syvr.hum\cache\artist\{slug}.json`, one JSON file per artist, version field for future schema evolution. Tauri commands: `get_artist_info`, `clear_artist_info_cache`.
- **New `src-tauri/src/artist_window.rs`** — `open_artist_panel` (creates `WebviewWindowBuilder` for label `artist-info`, computes anchor position from `overlay.outer_position` + `outer_size` + monitor height, auto-close listener via `app.listen("track-changed")`), `close_artist_panel`, `open_ticket_url` (URL host whitelist: bandsintown.com, ticketmaster.com, seatgeek.com, axs.com, livenation.com, last.fm, theaudiodb.com, musicbrainz.org). Uses `opener` crate for `shell.open` equivalent.
- **`src-tauri/src/lib.rs`** — added `mod artist_info; mod artist_window;`. `ArtistInfoCache::new(app.handle().clone())` managed in setup hook. Six new commands registered in `invoke_handler!`: `get_artist_info`, `clear_artist_info_cache`, `open_artist_panel_cmd`, `close_artist_panel_cmd`, `open_ticket_url`.
- **`src-tauri/src/settings.rs`** — new `show_artist_info_panel: bool` field, default `true`.
- **`src-tauri/Cargo.toml`** — added `urlencoding = "2"` and `opener = "0.7"`. Version bumped to `0.11.0`.
- **`src-tauri/capabilities/default.json`** — added `artist-info` to windows scope; added `core:window:allow-close`, `core:window:allow-set-position`, `core:window:allow-set-size`, `core:webview:allow-create-webview-window`.
- **`src/types.ts`** — `Settings` extended with `show_artist_info_panel: boolean`; new types `ArtistInfo`, `ArtistBio`, `TourDate`, `TicketStatus`.
- **`src/Overlay.tsx`** — `AlbumArtSide` and `AlbumArtBadge` accept optional `onClick?: () => void`; gold outline + pointer cursor on hover when onClick is provided. New `ArtistInfoDot` component (mirrors `UpdateBanner` geometry — 9×9 gold dot, hover-expand label). `openArtistPanel` handler gated on `settings.show_artist_info_panel && mode !== 'ghost'`. `DEFAULT_SETTINGS` updated with `show_artist_info_panel: true`.
- **`src/Settings.tsx`** — new `<Section title="Artist info panel">` with `<Toggle>` and cache-clear `<button>`.
- **New `src/artist-panel/index.html`**, **`src/artist-panel/main.tsx`**, **`src/artist-panel/ArtistPanel.tsx`** — second Vite entry point; React panel component with header/bio/similar/tour-dates/footer sections.
- **`vite.config.ts`** — `build.rollupOptions.input` added for multi-page build (`main` + `artistPanel`).

## [0.10.26] - 2026-05-21

### Fixed
- **Album art now matches the actual artist, not whatever iTunes/Deezer rank highest.** Real example that hit Hum: Lil Wayne's "Let It All Work Out" (Tha Carter V, 2018) displayed a T-Pain album cover because iTunes' free-text search ranked a T-Pain track higher on the term-frequency query `"Lil Wayne Let It All Work Out"`, and v0.10.22's `limit=1` accepted whatever came back without verifying the artist matched. The art fetcher now requests 10 results per query, iterates them, and accepts only records whose primary credited artist fuzzy-matches the requested artist. Primary artist is the part before `feat.` / `ft.` / `featuring` / `&` / `+` / `,` / `;` / `/` / `vs.` separators (so "Lil Wayne feat. T-Pain" → primary is "Lil Wayne", which matches a "Lil Wayne" validation; but "T-Pain feat. Lil Wayne" → primary is "T-Pain", which does NOT). Match is case-insensitive bidirectional substring after normalizing Unicode punctuation flavors (curly apostrophes, en-dashes, etc.), so legitimate variations like SMTC's "Beatles" vs iTunes' "The Beatles" still match. If no record in the top 10 passes validation, the variant returns no art rather than the wrong cover — better silence than wrong information.
- **Variant (c) title-only fallback no longer accepts wrong-artist tracks.** Previously the title-only retry (used when neither the SMTC artist nor any title-split prefix gave a hit) accepted iTunes' first result with no validation. Now the title-only path still validates returned records against the SMTC artist — only the QUERY drops the artist filter, not the validation. Concrete behavior: an SMTC track from "Lil Wayne" routed through variant (c) requires the returned record's primary artist to contain "Lil Wayne" (or vice versa). A wrong-artist match at the variant (a) level no longer "leaks" through variant (c).

### Architecture / files
- **`src-tauri/src/smtc.rs`** — refactored the iTunes + Deezer fetch chain to thread a `validation_artist` parameter separately from the `query_artist`. `fetch_art_via_itunes` (the public entry point) passes the SMTC artist as validation in all three variants — variant (a) `as-is` validates against the SMTC artist (same as the query artist), variant (b) `title-split` validates against the title-prefix (presumed to be the real artist when the SMTC artist field is junk), variant (c) `title-only` queries with no artist but still validates against the SMTC artist. Iteration uses a new `pick_artist_matched` helper that consumes a results iterator + a closure that extracts each record's artist string + the validation target, and returns the first matching `&serde_json::Value` (or `None` when nothing matches). New `primary_artist_matches` + `primary_artist_token` + `art_normalize` helpers implement the matching: `primary_artist_token` splits on the separator list above, `art_normalize` lowercases + collapses Unicode punctuation, `primary_artist_matches` does bidirectional substring containment with empty-input bail-out. iTunes search `limit` parameter bumped from 1 to 10; Deezer search `limit` parameter bumped from 1 to 10. The iTunes art-URL upscale (`100x100bb` → `600x600bb`) and Deezer XL-cover preference are unchanged from v0.10.22.
- **New test module `#[cfg(test)] mod tests` in `smtc.rs`** — 6 new unit tests pin the contract. `primary_artist_token_strips_feat_variants` covers all separator variants + the v0.10.26 bug case (`"T-Pain feat. Lil Wayne"` → `"T-Pain"`). `primary_artist_matches_accepts_real_artist` covers exact match, case-insensitive match, feat. credits, bidirectional "Beatles" vs "The Beatles", punctuation variants. `primary_artist_matches_rejects_wrong_artist` covers the Lil-Wayne-vs-T-Pain failure case, T-Pain-feat-Lil-Wayne rejection, completely unrelated artists, empty-input bail-out. `pick_artist_matched_*` tests cover empty-validation passthrough, skip-to-matching iteration, no-match returns None. All 28 lyrics + smtc tests pass, clippy `-D warnings` clean.

## [0.10.25] - 2026-05-21

### Fixed
- **YouTube uploads with a file extension in the title now find lyrics.** Real example that hit Hum: playing Uncle Kracker's "Follow Me" via a YouTube upload titled `"Follow Me Uncle Kracker Lyrics.wmv"` showed `♪ no lyrics for Follow Me Uncle Kracker Lyrics.wmv` despite being a well-known song. The `.wmv` extension shielded the trailing bare `Lyrics` from v0.10.24's `bare_trailing_tag_cleaner` (which requires `\s+Lyrics\s*$`), so the whole uploader-chrome suffix survived every cleaner pass and poisoned the LRCLib search. The cleaner now strips trailing media file extensions as the FIRST pipeline step, before any other cleaner runs. Vocabulary covers video containers (`.wmv`, `.mp4`, `.mkv`, `.avi`, `.mov`, `.webm`, `.flv`, `.m4v`, `.mpg`, `.mpeg`) and audio containers (`.mp3`, `.wav`, `.flac`, `.m4a`, `.aac`, `.ogg`, `.opus`). Case-insensitive. Trailing whitespace after the extension is allowed (`"Song.wmv  "` → `"Song"`). Mid-title extensions are left alone — only the trailing position is stripped, because no canonical released song has a media file extension at the end of its title but some weird tracks could plausibly contain `.mp3` in the middle (`"Song.mp3 (Live)"` → `"Song.mp3"`).

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — new `file_extension_stripper()` regex added as step 1 in the `clean_title` pipeline. Runs BEFORE the existing trailing-quote / bracketed / pipe / bare-trailing-tag steps, so by the time v0.10.24's `bare_trailing_tag_cleaner` looks for trailing `Lyrics` it sees the cleaned `"Follow Me Uncle Kracker Lyrics"` rather than the original `"Follow Me Uncle Kracker Lyrics.wmv"`. Vocabulary is restricted to real media container extensions — no ambiguous tokens like `.live` or `.remix`. Same safety bar as v0.10.24's "Lyrics is safe, Audio is not" reasoning: file extensions never appear at the end of canonical released song titles. 22 new unit tests added to the existing `cleans_titles` test, covering the original Uncle Kracker failure, every supported extension (video + audio), case-insensitivity, trailing whitespace, composition with the bracketed + bare-tag cleaners, and preservation of mid-title extensions.

## [0.10.24] - 2026-05-21

### Fixed
- **Songs titled `"Artist - Song Lyrics"` on YouTube now resolve to real lyrics.** Real example that hit Hum: playing Shaggy's "Angel" via a YouTube upload titled `"Shaggy - Angel Lyrics"` showed `♪ no lyrics for Shaggy - Angel Lyrics` despite "Angel" being one of the most-covered songs on LRCLib. The trailing bare word `Lyrics` (no brackets, no parens, no pipe) survived every previous cleaner pass and poisoned the LRCLib search query — even the retry path that strips the leading `"Shaggy - "` channel prefix saw `"Angel Lyrics"` rather than `"Angel"` and missed. The cleaner now also strips bare trailing uploader-chrome words from titles: `Lyrics`, `Lyric Video`, `Music Video`, `Official Music Video`, `Official Video`, `Official Audio`, `Official Visualizer`, and quality markers `HD`, `UHD`, `4K`, `8K`, `1080p`, `1440p`, `2160p`. Compound trailing tags collapse in one pass (`"Song HD 4K Music Video"` → `"Song"`). Titles that ARE the bare tag (e.g. a song literally named "Lyrics" or "Music Video") are preserved by requiring at least one non-whitespace char before the first tag.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — new `bare_trailing_tag_cleaner()` regex added as step 4 in the `clean_title` pipeline (after the bracketed `cleaner()` and `pipe_tag_cleaner()`). The vocabulary is deliberately narrower than the bracketed cleaner — bare `Audio`, `Visualizer`, `MV`, `HQ` without an `Official` qualifier are NOT stripped because they appear in legit song titles often enough to risk false positives. Bare `Lyrics`/`Lyric Video`/`Music Video`/`Official *` and the quality markers are safe because no canonical released song uses them as a title suffix outside of YouTube uploader conventions. 13 new unit tests added to the existing `cleans_titles` test, covering the original Shaggy failure, every bare-tag variant, compound trailing tags, preservation of single-word "tag-only" titles, preservation of the risky bare vocab (`Audio`/`MV`/`HQ`), and composition with the bracketed cleaner.

## [0.10.23] - 2026-05-21

### Added
- **Window backdrop setting.** A new "Window backdrop" dropdown appears in the Settings window beneath the "Blurred album art background" toggle. Four options: **Acrylic** (the default — translucent frosted-glass blur of whatever sits behind the overlay window, updates live as background windows move, the Windows 11 fly-out / Now Playing aesthetic), **Mica** (calmer, opaque-feeling tint that adopts the user's wallpaper color, doesn't blur live content, the File Explorer / Settings "in-place" look), **Tabbed Mica** (Mica variant with a slightly different tint, kept for parity with the DWM enum), and **None** (no OS backdrop — the previous fully transparent behavior, useful if the user wants the existing blurred-album-art layer to be the only background surface). Changing the dropdown re-paints the overlay immediately with no app restart; the choice persists to `%APPDATA%\com.syvr.hum\settings.json` under the `window_backdrop` key (values are `"acrylic"`, `"mica"`, `"tabbed_mica"`, `"none"`) and is restored on next launch. The setting is independent of the existing "Blurred album art background" toggle — both can be on, off, or any combination; both on means OS Acrylic at the bottom, blurred album-art layer on top, lyrics above that. On Windows 10 or pre-22H2 Windows 11 builds the OS no-ops the DWM call and the window stays transparent as if "None" were selected, with no error visible to the user. No new dependencies, no new packages, no permission prompts.

### Architecture / files
- **New `src-tauri/src/backdrop.rs`** — `BackdropKind` enum (`None`, `Mica`, `Acrylic`, `TabbedMica`, serialized snake_case via `serde(rename_all)`) with `#[derive(Default)]` defaulting to `Acrylic`. `apply_backdrop(hwnd: HWND, kind: BackdropKind) -> windows::core::Result<()>` calls `DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &value, size_of::<u32>())` with `value` mapped from `BackdropKind::dwm_value()` to Microsoft's `DWM_SYSTEMBACKDROP_TYPE` integers (None=1, Mica=2, Acrylic=3, TabbedMica=4). Four unit tests cover the integer mapping, default-is-Acrylic, snake_case serde round-trip, and rejection of unknown variants.
- **`src-tauri/Cargo.toml`** — added `"Win32_Graphics_Dwm"` to the existing `windows` crate feature array (no version change, no new dependencies).
- **`src-tauri/src/settings.rs`** — added `window_backdrop: BackdropKind` field to the `Settings` struct, defaulting to `BackdropKind::Acrylic` in the `impl Default for Settings` block. The `sanitize()` function normalizes invalid `window_backdrop` values on the non-Windows code path (Windows uses serde's enum rejection + `serde(default)`). `update_settings` now captures `backdrop_changed = patch.get("window_backdrop").is_some()` before the merge step and, when true, re-applies the new backdrop via `crate::backdrop::apply_backdrop` against the overlay window's HWND immediately before emitting `settings-changed`.
- **`src-tauri/src/lib.rs`** — setup hook now applies the persisted backdrop against the overlay window's HWND (`HWND(raw_hwnd.0)` to bridge potential windows-crate version mismatch with Tauri's internal HWND type) before the existing `apply_mode` call, so the OS compositor effect is in place when the overlay first paints. Errors from `apply_backdrop` and `overlay.hwnd()` log via `eprintln!` and continue silently.
- **`src/types.ts`** — `Settings` type extended with `window_backdrop: "acrylic" | "mica" | "tabbed_mica" | "none"`.
- **`src/Overlay.tsx`** — `DEFAULT_SETTINGS` literal updated with `window_backdrop: "acrylic"` to keep TS happy and pre-IPC defaults aligned with backend defaults.
- **`src/Settings.tsx`** — new `<Row label="Window backdrop">` containing the labeled `<Select>` immediately after the existing `Blurred album art background` toggle row. Wired through the existing 200ms-debounced `update("window_backdrop", v)` → `invoke("update_settings", { patch })` flow.

## [0.10.22] - 2026-05-21

### Added
- **Pandora.com web player now works.** Songs playing on pandora.com in Chrome surface real lyrics, the same as Spotify Web / YouTube / iTunes already did. Previously the overlay showed `♪ no lyrics for Today's Hits Radio - Now Playing on Pandora` because Pandora's website doesn't call `navigator.mediaSession.metadata` — SMTC fell back to the browser tab title (a station name, not a song) and the resolver had nothing real to look up. Hum now reads Pandora's now-playing widget directly from Chrome's accessibility tree via Windows UI Automation (the same API screen readers use); Chromium enables its UIA tree on demand with no user prompt, no extension, no flag. The real track title / artist / album feed into the standard cleaner + LRCLib / SimpMusic / NetEase resolver path, so any song Pandora plays gets the same lyric coverage as a song from any other source. Polls every 2 seconds while a Pandora tab is the active SMTC source; idle (zero CPU, zero UIA calls) when the user is on YouTube / Spotify / iTunes / anything else. Trait-based extension point under the hood means future no-Media-Session web players (SoundCloud, Bandcamp, etc.) land as one-file additions.
- **New "Unsupported" overlay status for sources Hum can't decode.** When Hum sees SMTC reporting an unreliable source (Pandora web with the UIA probe unavailable, or any future case where a known-broken source publishes audio without track metadata) and has no fresh bridge data, the overlay now shows `♪ Pandora web — track info unavailable` instead of the misleading `♪ no lyrics for [station name]`. The honest message replaces the "lookup failure" framing — users know it's a source-side limitation, not a missing-song problem. Like NotFound (since v0.10.15), Unsupported is never persisted to disk and never cached in memory, so a Hum upgrade or a Pandora-side fix immediately propagates without stale verdicts.

### Architecture / files
- **New `src-tauri/src/web_bridge.rs`** — `WebPlayerProbe` trait + `PandoraProbe` impl + polling loop. `PandoraProbe::detects` is a pure string match (Chromium AUMID + Pandora `<title>` suffix); `PandoraProbe::read` walks the UIA tree of the matching Chrome window via the `uiautomation` crate, extracting the track / artist / album text from Pandora's now-playing widget. The loop spawns at startup; idles at 5s ticks with zero UIA calls when no probe matches, polls every 2s when a probe is active. The Pandora selector strategy is case-insensitive substring match on stable CSS Module slot names (`__current__trackName`, `__current__artistName`, `__current__albumName`) — these are derived from Pandora's source CSS and survive React rebuilds. `CHROMIUM_PROCESS_NAMES` constant (chrome.exe, msedge.exe, brave.exe, opera.exe, vivaldi.exe) keeps the AUMID-side check (`PandoraProbe::detects`) and the process-name-side check (`find_chrome_windows`) consistent.
- **`src-tauri/src/lyrics.rs`** — new `CachedLyrics::Unsupported` and `Status::Unsupported` enum variants. `lyrics::start`'s main loop consults the shared web-bridge cache before falling back to the SMTC snapshot; when the bridge is fresh (<5s old, non-empty title), its title / artist / album override SMTC for the duration of that resolution. When SMTC's title matches a known-unreliable source AND no fresh bridge data exists, the loop short-circuits to Unsupported without any network calls. `write_store` and `read_store` skip Unsupported the same way they skip NotFound — never persisted, never memory-cached. A second event listener wakes the loop on `web-bridge-updated` (emitted by the bridge worker on title change) so Pandora track changes register even when SMTC reports the same browser tab title.
- **`src-tauri/src/lib.rs`** — new `SharedWebBridge` managed state. `web_bridge::start` spawns alongside `smtc::start` at boot. `lyrics::start` receives the shared bridge as a new parameter (gated `#[cfg(windows)]` since the bridge module is Windows-only).
- **`src/Overlay.tsx`** — new `unsupported` branch in `statusLine`. Renders `♪ Pandora web — track info unavailable` for Pandora-specific source matches and a generic `♪ track info unavailable for this source` for future unsupported sources.
- **`src/DevConsole.tsx`** — parallel `unsupported` rendering branch mirrors the existing `not_found` styling.
- **`src/types.ts`** — `LyricsStatus` union extended with `"unsupported"`.
- **`Cargo.toml`** — new `uiautomation = "0.25"` crate dependency, plus `Win32_System_Threading` and `Win32_System_ProcessStatus` features added to the existing `windows` crate for the Chrome window enumeration helpers.

### Diagnostic notes
- Pandora UIA selectors are matched on case-insensitive substrings of stable CSS Module slot names (`__current__trackName`, etc.) — not on Pandora-internal React class names that change between deploys. If Pandora ships a redesign that renames the underlying slots, the probe returns `None` and the resolver falls through to Unsupported; overlay shows the honest status rather than wrong lyrics. Updating the selectors after a Pandora redesign is a single-file edit; the discovery procedure (run `inspect.exe`, hover the elements, copy their stable attributes) is documented at the top of the `Pandora UIA selector reference` comment block in `web_bridge.rs`.
- Performance: a Pandora UIA tree walk visits a few hundred nodes (capped at 5000) and takes 50-300ms on a typical Chrome session. With 2s polling cadence, that's well under 1% CPU averaged over a minute. Idle state (no probe active) uses essentially zero CPU — the loop is a 5s sleep that checks the SMTC snapshot pointer and goes back to sleep.
- YouTube / Spotify Web / iTunes desktop / Apple Music desktop / SoundCloud (today) / Bandcamp (today) all expose Media Session metadata correctly and never enter the probe path. SMTC title-pattern matching is precise (`ends_with("Now Playing on Pandora")` not `contains("pandora")`) to avoid false-positives on song titles that mention Pandora.
- Pandora's track title sometimes contains a doubled `Name` property in the UIA tree (visible text + aria-label concatenated). The `dedupe_doubled` helper collapses exact whitespace-separated duplicates (`"Song Song"` → `"Song"`) while preserving any title whose halves differ.

## [0.10.21] - 2026-05-21

### Fixed
- **YouTube lyric-channel videos with a quoted excerpt in the title now find lyrics.** Channels like BangersOnly bait clicks by appending a memorable line in quotes after the real title — e.g. `Benson Boone - Beautiful Things (Lyrics) "i want you i need you oh god"`. Previously, the cleaner pipeline stripped `(Lyrics)` but left the trailing `"..."` intact. That bloated the user-side title to ~60 chars while LRCLib's canonical record is ~16 chars (`Beautiful Things`), so the length-ratio path in `pick_best` returned a title score around 67 — below the threshold of 80 — and the overlay showed `♪ no lyrics for Benson Boone - Beautiful Things (Lyrics) "i want you i need you oh god"`. Now a new `trailing_quote_stripper` regex strips trailing ASCII `"..."` and curly `"..."` excerpts before any other cleaner step runs. The stripper requires non-whitespace + whitespace before the opening quote, so legit fully-quoted titles like Macklemore's `"Same Love"` (which start with the opening quote) are left alone. Mixed-quote flavors (curly opening + ASCII closing, seen when YouTube's smart-quote pass is inconsistent) are also handled.
- **Videos with new video-quality tags in the title now find lyrics.** Real-world failure: `Train - Drops Of Jupiter (Tell Me) (Official 4K Video)` left `(Official 4K Video)` intact because the previous `cleaner()` regex only accepted a fixed allowlist of modifiers — `music`, `lyric`, `hd`, `animated` — between `Official` and `Video`. `4K`, `8K`, `60fps`, `1080p`, `Animated` combined with `4K`, and every other new uploader fashion slipped through. The cleaner regex now accepts ANY sequence of words before the `video` / `audio` / `visualizer` terminals using `(?:[\w'\-]+\s+)*` — a structural rule rather than an enumeration. New quality tokens added in YouTube uploader chrome no longer require a per-token regex patch. `1080p`, `1440p`, `2160p`, `60fps`, `30fps`, `hq` were added as standalone bracket-content alternatives too. The same flexibility applies to `(Official Animated 4K Music Video)`, `[Official 1080p HD Music Video]`, `(Live 4K UHD Audio)`, and any combination ending in one of those three terminals.
- **Cleaner pipeline has unit-test coverage for both real-world failures plus regression coverage for legit annotations.** Title-cleaning bugs were the dominant lyric-resolver failure mode across v0.10.8 → v0.10.20; tracks kept slipping past one regex patch at a time. The `cleans_titles` test now covers 21 cases: baseline noise tags, v0.10.11's `(Official Audio)` family, the v0.10.21 video-quality-modifier failures (4K/8K/60fps/1080p/2160p including Train's "Drops Of Jupiter" case), the v0.10.21 trailing-quote excerpt (including the Benson Boone case), curly-quote handling, fully-quoted-title preservation (`"Same Love"`), and combined two-layer cases. New failure patterns now get codified as test cases rather than diagnosed track-by-track in chat.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — `cleaner()` regex loosened: the previous tight allowlist `(?:music\s+|lyric\s+|hd\s+|animated\s+)?video` (and equivalents for `audio` / `visualizer`) is replaced with structural `(?:[\w'\-]+\s+)*video` (and equivalents). The bounded-vocabulary alternatives (`lyrics?`, `feat`, `ft`, `featuring`, `remaster`, `live at|from|in`, `acoustic`, `unplugged`, `demo`, version markers, `radio edit`, etc.) remain enumerated since they ARE finite sets. Standalone quality-token alternatives extended: `4k`, `8k`, `1080p`, `1440p`, `2160p`, `60fps`, `30fps`, `hq` join the existing `hd`, `uhd`, `mv`, `\d{1,2}k` set. New `fn trailing_quote_stripper() -> &'static Regex` declared next to `clean_title`. `clean_title` now runs three steps in order: trailing-quote strip → parenthetical/bracket strip → pipe-tag strip. `tests::cleans_titles` extended from 7 to 21 assertions covering the failure surface that prompted this release.

### Diagnostic notes
- Score walkthrough for the Benson Boone case after the fix: user title cleans to `Benson Boone - Beautiful Things`. The aggressive retry path in `fetch_lrclib` calls `strip_youtube_noise` which strips the `Benson Boone - ` prefix, leaving `Beautiful Things`. LRCLib `/api/search?track_name=Beautiful Things` returns the canonical record with title `Beautiful Things` by Benson Boone, ~213s, synced. Score: title 100 (exact) + duration 30 (≤5s diff) + artist 0 (we don't pass artist on the retry path's search, scoring sees neither side) + synced 20 = 150. Picked. Overlay surfaces the synced lyrics.
- Score walkthrough for the Train case after the fix: user title cleans to `Train - Drops Of Jupiter (Tell Me)`. Note the cleaner KEEPS `(Tell Me)` because the parenthetical contains no noise tokens — that's the legit canonical subtitle. The retry path strips `Train - ` leaving `Drops Of Jupiter (Tell Me)`, which exactly matches LRCLib's canonical record title. Score: title 100 + duration ~30 + synced 20 = 150. Picked.
- The structural rule "any words before video/audio/visualizer terminal" only loosens at one specific position (just before those three terminal words). It does NOT broadly accept any parenthetical content — `(Tell Me)`, `(Acoustic Version)`, `(2024 Remaster)`, `(Demo)`, and other legit subtitle/version annotations still survive because none of them end in `video`/`audio`/`visualizer` and none match the other enumerated alternatives that exist.

## [0.10.20] - 2026-05-21

### Fixed
- **Mashups / bootlegs / fan edits no longer surface a constituent song's lyrics out of sync.** Fan-made YouTube mashups don't exist on LRCLib / SimpMusic / NetEase, but the songs they're built from do. Previously the resolver would happily match one of the source songs (e.g. "Twista x Wetter (SW Mashup)" returned Twista's actual "Wetter" lyrics) and surface those lyrics confidently misaligned against the mashup audio — feeding the user wrong-time output. Now the resolver runs a `looks_like_mashup` check upfront on the user-reported title; if the title contains explicit fan-creation keywords (`mashup`, `bootleg`, `fan edit`, `flip edit`, `dj edit`), the resolver short-circuits to NotFound and the overlay shows the normal "♪ no lyrics for X" status. Detection is intentionally conservative: " x " / " vs " / " versus " separators are NOT included because they appear in plenty of released tracks ("Romeo x Juliet", "Smith vs Mills"). False negatives are acceptable since the scoring threshold will reject weak matches downstream; false positives (refusing real songs) are not.
- **SimpMusic now uses the same scoring framework as LRCLib.** Before this release, `pick_best_simpmusic` filtered by artist + ±5s duration and IGNORED title entirely — SimpMusic's broad title-search API returns whatever's plausibly related, and the picker was happy to pick a record by the user's artist within ±5s of the track length without checking that the actual title matched. For mashups specifically this meant whatever song happened to land near the runtime got picked. Now SimpMusic candidates score on the same axes as LRCLib: title 0-100 (exact / substring with length ratio / token overlap), duration -50 to +30, artist 0-20, plus a SimpMusic-specific lyric-quality bonus (richSync = +25, syncedLine = +20, plain = +5 — SimpMusic's reason for being in the cascade is rich word-level timing, so it gets the heavier weight).

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — new `fn looks_like_mashup(title: &str) -> bool` near `clean_title`. New early-return branch at the top of `resolve_lyrics` (right after `clean_title` / `clean_artist`) that bypasses LRCLib / SimpMusic / NetEase entirely when the original SMTC title is a mashup. `SimpMusicRecord.song_title` lost its `#[allow(dead_code)]` since the new picker uses it. `pick_best_simpmusic` rewritten end to end to mirror `pick_best`'s scoring shape (THRESHOLD: i64 = 80, title 0-100, duration step-function -50..+30, artist 0-20, lyric_bonus 0-25). `fetch_simpmusic` updated to pass `title` into `pick_best_simpmusic`. User-Agent bumped to `hum/0.10.20`.

### Diagnostic notes
- The "Twista x Wetter (SW Mashup)" case: `looks_like_mashup` finds "mashup" in the lowercased title → returns true → `resolve_lyrics` returns NotFound immediately with `source: "mashup-skip"` and `persist: false`. No LRCLib / SimpMusic / NetEase calls fire. Overlay shows "♪ no lyrics for Twista Ft. Morgan Wallen - Dangerous x Wetter (SW Mashup)" — correct.
- The list of mashup keywords is short on purpose. If real-world reports surface other patterns ("nightcore edit", "slowed + reverb", "tiktok edit"), they can be appended trivially without changing the surrounding logic. "Remix" is intentionally NOT in the list — many legit released tracks are titled "X (Remix)" and DO have LRCLib records (Madeon's "All My Friends (Remix)" etc.).

## [0.10.19] - 2026-05-21

### Changed
- **LRCLib record matching switched from cascading hard filters to weighted scoring.** Every prior version of `pick_best` used "title substring AND duration ±N seconds OR rejected" — one weak signal rejected the candidate entirely. Real LRCLib data is too noisy for hard filters: YouTube lyric uploads vary 5-15s from canonical durations, uploader-pseudonym records mix verbatim YouTube titles with canonical ones, artist names appear in 4-5 capitalization/spacing flavors. Every track that previously needed a per-track bandaid (Fleetwood Mac, G-Eazy & Halsey, The Script, Goo Goo Dolls) had the SAME root cause: a strong signal on one dimension was being killed by a marginal failure on another. The new pick_best scores each candidate on title similarity (0-100), duration closeness (-50 to +30), artist match (0-20), synced-vs-plain bonus (0 or +20). A threshold of 80 requires strong evidence on at least two signals; a record above the threshold with the highest total wins. Strong title matches survive marginal duration mismatches, exact title matches with bad duration AND wrong artist correctly get rejected (the classic Britney-vs-Ashnikko "Toxic" disambiguation still works), and partial-token overlap can rescue records whose title shape doesn't quite match the SMTC-reported form.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — `pick_best` rewritten end to end. The `_artist` parameter that was previously ignored is now used for the artist-score component. Title scoring has four tiers: exact match (100), bidirectional substring with length-ratio scaling (60-90 — short ratios mean one side has lots of extra noise around a real match, longer ratios mean the substring carries most of the meaning), word-token overlap (20-50 — last-chance partial match for cases like reordered words or unusual cleanups), no overlap (-1, filtered before scoring continues). Duration scoring is a step function: 0-5s = +30, 6-10s = +22, 11-20s = +12, 21-30s = +4, 31+s = -50 (the negative is what enforces the Toxic disambiguation). Artist score is 0 / 10 / 20 for empty / substring / exact (skipped when either side is empty so SMTC-blank-artist tracks aren't penalized). Synced bonus is a flat +20. Threshold constant is `THRESHOLD: i64 = 80`. Walkthroughs of concrete cases live in the function-level doc comment. User-Agent bumped to `hum/0.10.19`.

### Diagnostic notes
- Concrete score walkthroughs for the tracks that previously failed:
  - **Fleetwood Mac - Dreams (Official Audio)**: PASS 1 (cleaned to "Fleetwood Mac - Dreams") returns records that contain or equal the user title. Exact match record at 250s vs YouTube ~256s → 100 + 22 + 20 = 142, picked.
  - **G Eazy & Halsey - Him & I (Lyrics)**: PASS 1 returns "G-Eazy & Halsey - Him & I (Official Video)" records, where rec contains user title minus 9 chars of "(Official Video)" noise (ratio ~0.74). Title 60 + 30·0.74 = 82. Duration close. Synced. Total ~124, picked. (Plus the PASS 2 stripped retry remains in place as a fallback.)
  - **The Script - The Man Who Can't Be Moved (Lyrics)**: PASS 1 returns carbon-copy "The Script - The Man Who Can't Be Moved (Lyrics)" records where rec_title CONTAINS user_title (the user title cleaned of "(Lyrics)" is exactly the substring before " (lyrics)" in the rec). Ratio ~0.82, title = 84. Duration diff ~8s, score 22. Synced 20. Total 126, picked.
  - **The Goo Goo Dolls - Iris**: Exact title match against "The Goo Goo Dolls - Iris" record at 290s. If YouTube duration is anywhere from 280-300s, score 100+22+20 = 142 (10s diff threshold) or 100+12+20 = 132 (20s diff threshold) — both well above 80.
- Toxic disambiguation: if a user is playing Britney's "Toxic" (~200s) but LRCLib search returns only the Ashnikko version (160s), record-1 scores 100 (exact title) + (-50) (40s diff) + 20 (synced) + 0 (artist mismatch if SMTC reported "Britney Spears") = 70 → below threshold 80 → filtered → return None → resolver continues to SimpMusic / NetEase or returns NotFound. Behavior matches what the old hard ±5s/±10s filter did for that case, but the new code achieves it through scoring rather than threshold tuning.

## [0.10.18] - 2026-05-21

### Fixed
- **Auto-contrast now respects the overlay's own background.** When the blurred album-art background (v0.10.8) was on and the desktop behind happened to be light — e.g. a white-themed YouTube tab, a Word doc — the previous auto-contrast logic sampled the screen-behind-the-window luminance, decided "background is light → use dark text," and rendered black text directly on top of the dark blurred album art. Result: nearly invisible dark-on-dark lyrics for the entire song. Auto-contrast now composites the actual surface the user sees through the lyric text: blurred album art (dimmed at brightness 0.62) → user `bg_color` alpha-blended on top at `bg_opacity` → only falls back to the screen sample when neither has any signal. Hysteresis (light → dark crosses 0.45, dark → light crosses 0.55) is unchanged but now applied to the composited luminance so it correctly debounces transitions caused by track changes (album-art swaps producing new tint colors) instead of just transitions from a video playing on the desktop behind.

### Architecture / files
- **`src/Overlay.tsx`** — state shape changed: `bgIsLight: boolean | null` is now `surfaceIsLight: boolean | null`, sourced from a derived `surfaceLuminance: number | null` computed each render via the new `computeSurfaceLuminance` helper. New `screenLuminance: number | null` state holds the raw value from `contrast.rs`'s `bg-luminance` event (the listener no longer applies hysteresis there — it just stores the value). A `useEffect` keyed on `surfaceLuminance` applies the hysteresis pass and updates `surfaceIsLight`. `autoColorActive` / `effectiveTextColor` / `effectiveTextColorDim` / `effectiveTextShadow` all use `surfaceIsLight` instead of the old `bgIsLight`. Two new pure helpers near the existing color utilities: `computeSurfaceLuminance` (composites screen + blur+tint + user-bg layers back-to-front, returns 0..1 or null), and `hexLuminance` (luminance of a `#rrggbb` color, returns 0..1 or null).

### Diagnostic notes
- Composite math: screen layer is the raw `screenLum` from `contrast.rs`. Blur layer is `(0.299·r + 0.587·g + 0.114·b) / 255 · 0.62` from the extracted dominant `tintColor`, where the 0.62 multiplier mirrors the CSS `filter: brightness(0.62)` on the blur element. User layer is `hexLuminance(bg_color) * (bg_opacity/100) + previous * (1 - bg_opacity/100)`. When `showBlurBg` is true the blur layer replaces the screen layer (the blur opaquely covers it). When `bgOpacityPct = 0` the user layer is a no-op.
- For the typical case Wes hit (blurred bg ON, light desktop behind): screen layer is now ignored entirely because the blur covers it; blur layer's luminance comes from the dimmed album art (usually 0.1-0.3 — solidly dark); user layer at default `bg_opacity=0` is also a no-op; final composited luminance ~= 0.15-0.25 → `surfaceIsLight=false` → white text. Correct.
- If a user wants to force a specific text color regardless of background detection, the "Auto contrast" toggle in Settings → Extras (still defaults ON because most setups benefit) remains the master switch — turning it off uses the user's explicit `text_color` / `text_color_dim` everywhere.

## [0.10.17] - 2026-05-21

### Fixed
- **LRCLib now finds "The Script - The Man Who Can't Be Moved (Lyrics)" and similar YouTube lyric videos with intro/outro padding.** Duration filter widened from ±5s to ±10s. The previous filter assumed the playing track's duration would land within 5s of the canonical recording — true for Spotify (which reports the exact studio length) but wrong for YouTube uploads, where lyric-video creators routinely add 5-10s of black screens, title cards, or fadeouts. Live LRCLib data for "The Man Who Can't Be Moved" shows records at 240-244s; the typical YouTube lyric video plays at ~249s, leaving every match strictly outside the ±5s window. ±10s catches the padding while still comfortably rejecting unrelated covers (the canonical Toxic disambiguation example — Ashnikko 163s vs Britney 203s — is a 40s diff and still gets filtered out cleanly).
- **In-memory NotFound cache no longer survives the session.** v0.10.15 stopped persisting NotFound to disk, but the in-memory cache (used to avoid redundant API calls within a single session) was still writing NotFound entries, so a track that failed under an older resolver version stayed failed for the rest of the running session even after a Hum upgrade — the user had to fully restart the app to get a fresh resolution attempt. Mem cache write is now skipped for NotFound results to match the disk-cache behavior. Cost: each replay of an unfindable track within a session now re-hits the 3 lyric sources in parallel (~1-2s of background work, doesn't block the overlay UI). Acceptable while the resolver heuristics are still being tuned.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — `pick_best`'s `tolerance_secs` constant changed from 5 to 10, with an updated comment explaining the YouTube lyric-video padding rationale and reconfirming the Toxic example is still safely disambiguated at the wider tolerance. The `if any_clean_notfound { ... }` branch in `resolve_lyrics` no longer writes `CachedLyrics::NotFound` to the `mem` RwLock, and `persist: errors.is_empty()` becomes the unconditional `persist: false` (the disk write was already a no-op for NotFound since v0.10.15, but the field is now correctly set to false on the data path so the value matches the behavior). User-Agent bumped to `hum/0.10.17`.

### Diagnostic notes
- The combination of all the lyric-finding fixes since v0.10.10 — pipe-tag cleaner (v0.10.9), `(Official Audio)` parens (v0.10.11), `pick_best` retry on filter-fail (v0.10.12), Unicode punctuation normalization (v0.10.14), disk-cache NotFound discard (v0.10.15), mem-cache NotFound skip + ±10s duration tolerance (this release) — should leave very few real-world tracks unresolved. If a NotFound report surfaces after v0.10.17, the next layers to investigate are: artist-cleaner gaps for less-common YouTube channel suffixes, SimpMusic / NetEase per-source filters, and LRCLib upload coverage (some niche tracks genuinely aren't on LRCLib).
- Wider duration tolerance does mean ambiguous-name tracks with similar runtimes ("Closer" by Chainsmokers vs NIN — 4:04 vs 4:38, comfortably outside ±10s) stay safe, but two same-name songs within 10s of each other could now resolve to the wrong record. Mitigations already in place: title substring match is bidirectional + Unicode-normalized; sort prefers synced. If this becomes a real-world issue, the next step is adding an artist-substring tiebreaker in `pick_best` (the `_artist` param is currently ignored).

## [0.10.16] - 2026-05-21

### Added
- **Upcoming first lyric preview during song intros.** When a track has synced lyrics loaded but the song hasn't reached the first line yet (intro / instrumental opening), the overlay now shows `lines[0]` in the smaller "next" row below the current row. The current row stays as the status line (`♪`) since no line is technically "playing." When the song advances past the first line's timestamp, `displayIdx` flips to 0, the preview becomes the current line (big row), and `lines[1]` shifts into the next slot — same transition the line-swap motion (v0.10.13) already animates. Previously the overlay rendered a lonely `♪` through the whole intro and the user had no signal for how long the wait would be or what would come first.

### Architecture / files
- **`src/Overlay.tsx`** — the `prev` / `cur` / `next` derivation block previously short-circuited on `displayIdx >= 0`. Now when synced lyrics are loaded but `displayIdx === -1`, the code takes a separate branch that sets `next = lyrics.lines[0]` and leaves `cur` / `prev` undefined (so the existing `middleText` fallback to `statusLine(...)` still drives the cur row's content — no behavioral change for the "♪" / "♪ fetching" / "♪ no lyrics" cases). Single-line and full-page layouts unaffected (single-line only ever renders the cur row; full-page renders all lines indexed by position so the intro preview is implicit there).

### Diagnostic notes
- The preview only renders when status is `synced`. For status `fetching` / `not_found` / `error` / `instrumental` / `plain`, `next` stays undefined and the next row is empty — those statuses don't have a meaningful "upcoming first lyric" to show.
- If a synced track has its first lyric at t=0 (no intro), the preview state lasts for a single frame before `displayIdx` advances to 0. Invisible in practice. No special case needed.

## [0.10.15] - 2026-05-21

### Fixed
- **Edit mode now lets you drag the window from anywhere inside it.** Previously, only the lyric text rows and the outermost container's tiny padding band were Tauri drag regions — clicking on the album art square, the gap between art and lyrics, or any blurred-background area did nothing. Now every visible chrome element in edit mode (the outer stack wrapper, the inner art+lyrics row, the lyrics column, and the album-art square) gets the `data-tauri-drag-region` attribute so the user can grab any pixel of the window and move it. The blurred album-art background layer keeps `pointer-events: none` so clicks pass through to the drag-region children underneath. Locked / ghost modes still have no drag regions anywhere — exactly as before, the window stays put unless you cycle back to edit mode.
- **"No lyrics for X" no longer persists across app restarts.** The lyric-finding algorithm is still evolving — every recent version (v0.10.11 stripped `(Official Audio)`, v0.10.12 added pick_best retry, v0.10.14 normalized Unicode punctuation) opened up new tracks that used to fail. But once a track returned NotFound under an older version, the result was cached to disk and the new algorithm never got a chance to re-run for that key. Now `write_store` skips any NotFound entry (only Synced / Plain / Instrumental results hit disk), and `read_store` discards any pre-existing NotFound entry it loads — so on the next restart, every previously-unfindable track gets a fresh resolution attempt with the latest logic. Within a single session, the in-memory NotFound cache still suppresses redundant API calls if the same unfindable track plays multiple times.

### Architecture / files
- **`src/Overlay.tsx`** — `{...dragProps}` (which spreads `{ "data-tauri-drag-region": true }` when `isEdit`) added to `outerStackStyle`, `innerRowStyle`, and `lyricsColStyle` wrapper divs in both the 3-line and single-line layout branches. `AlbumArtSide` gains a `dragRegion: boolean` prop and applies the attribute to its outer wrapper div (the inner `<img>` stays `pointer-events: none` so the drag region is the square's full area).
- **`src-tauri/src/lyrics.rs`** — `read_store` now parses the value, checks for `CachedLyrics::NotFound`, and returns `None` if so (treating the entry as if it wasn't there). `write_store` early-returns when the value is `NotFound` before opening the store. Documented why successful matches (Synced / Plain / Instrumental) still persist forever — their content doesn't depend on resolver heuristics. User-Agent bumped to `hum/0.10.15`.

### Diagnostic notes
- The drag-region regression was technically present pre-v0.10.8 — the album-art square and the gap to its right never had drag-region — but became more noticeable with v0.10.8's blurred background, which made the previously-invisible "empty" areas visually full and tempting to click. Adding drag-region to the wrapper elements is the right fix regardless of the background.
- The NotFound disk-cache change does mean a clean-NotFound track now costs 3 parallel HTTP calls on every app restart instead of 0. In practice these calls are bounded by the 30s SMTC + reqwest timeouts and run in the background — the user-visible "fetching" state lasts ~1-2s and doesn't block the overlay rendering. The trade-off favors algorithm freshness over network frugality, which is the right call while the resolver is still being tuned.

## [0.10.14] - 2026-05-21

### Fixed
- **LRCLib title-match no longer rejects records that differ only in punctuation flavor.** Live LRCLib data shows different uploaders use different Unicode punctuation for the same song — one record titled "The Man Who Can't Be Moved" with an ASCII apostrophe (`'`), another with a curly right single quote (`'` U+2019), another with an en-dash where ASCII uses a hyphen. The substring match in `pick_best` is byte-level, so `Can't` (ASCII) was failing against `Can't` (curly) and the candidate got filtered out. Now both the query title and each record's title are normalized through a new `normalize_for_match` helper before lowercasing — curly apostrophes / left+right quotes / primes collapse to ASCII `'`, curly double quotes collapse to ASCII `"`, en-dash / em-dash / figure-dash / horizontal-bar collapse to ASCII `-`, and non-breaking space (U+00A0) collapses to regular space. Symmetric — applies to both sides of every comparison, so no record that genuinely matches gets rejected on punctuation grounds.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — new `fn normalize_for_match(s: &str) -> String`. Called twice in `pick_best`'s filter loop: once on the query title (cached as `title_l`), once per record on `r.track_name`. Replaces the previous `s.to_lowercase()` direct calls. Lowercase happens AFTER the character-by-character normalization map so the casing rules apply to the normalized characters (matters for none of the punctuation chars but keeps the API tidy). User-Agent bumped to `hum/0.10.14`.

### Diagnostic notes
- The trigger for this fix was a real-world report of "no lyrics for The Script - The Man Who Can't Be Moved (Lyrics)". The first-pass LRCLib search returns 15 records with the YouTube-noisy title shape (e.g. "The Script - The Man Who Can't Be Moved (Lyrics)"), and the v0.10.5 retry catches the second-pass query for "The Man Who Can't Be Moved" returning 20 records — but those 20 records use a mix of ASCII and curly apostrophes, so any record whose punctuation flavor didn't match the SMTC-reported title was being skipped. With the normalization, all flavors collapse to the same comparison string.
- If a particular track still returns NotFound after this fix, the next suspect is the duration filter (±5s tolerance). YouTube lyric videos sometimes add intro/outro screens that push the upload past the canonical song duration by 5–10s, which would still filter all matches out. Out of scope for this fix; revisit if real-world reports surface.

## [0.10.13] - 2026-05-21

### Added
- **Line-swap motion when the song advances.** Each time a lyric line transitions (prev → cur → next slot, or the very first line of a freshly-fetched track), the new text now animates in with a brief lift-from-below + fade rather than instantly replacing the previous content. Animation is 340ms with a slight spring-overshoot easing (`cubic-bezier(0.34, 1.56, 0.64, 1)`) — fast enough to keep up with rapid lyric advances, springy enough to feel alive. Fires on all three rows simultaneously (prev / cur / next), so the entire row group moves as a unit when the song advances by one line. Pairs naturally with the v0.10.8 karaoke per-word gradient sweep: line-in motion handles between-line transitions, karaoke handles within-line word timing.

### Architecture / files
- **`src/index.css`** — new `@keyframes hum-line-in` (translateY 10px → 0, opacity 0 → 1, 60% keyframe holds opacity at 1 so the fade-in finishes before the spring-overshoot settle) and `.hum-line-in` class binding the animation with the easing curve. Lives in index.css instead of inline because `@keyframes` can't be expressed in React inline styles.
- **`src/Overlay.tsx`** — `LineRow`'s render content (both karaoke per-word spans and plain-text branches) is now wrapped in a `<span key={text}>` with `className="hum-line-in"` and `display: inline-block`. The key change on text update remounts the wrapper, which triggers the CSS animation. `inline-block` is required because the animation uses `transform`, which doesn't apply to inline-level boxes. The karaoke per-word gradient animation (background-position transition on the inner spans) is independent of the line-in transform animation — they layer cleanly.

### Diagnostic notes
- The animation fires on EVERY render where the wrapper's text changes — including the initial mount (`♪` → first line) and the prev/next rows (whose text also rotates on each line advance). This is intentional: animating all three rows in sync looks like the row group moved up by one slot, even though structurally the rows just re-keyed their text.
- The "lift-from-below" displacement is 10px regardless of font size. At very small font sizes (slider dragged way down) the lift is proportionally larger; at very large sizes it's proportionally smaller. Acceptable trade-off since the user's typical font range is 18-32px where 10px reads as a subtle hop. If this becomes a real issue, the keyframe could be ported to use `em` units.

## [0.10.12] - 2026-05-21

### Fixed
- **LRCLib now finds "G Eazy & Halsey - Him & I (Lyrics)" and other tracks that previously returned non-matching records.** Specific case verified via live LRCLib query: searching for the cleaned title `"G Eazy & Halsey - Him & I"` returns 3 records, but all 3 are unsynced AND have track_name `"G-Eazy & Halsey - Him & I (Official Video)"` — `pick_best` rejects them because its bidirectional substring check fails on `"G-Eazy"` (hyphen) vs `"G Eazy"` (space, from SMTC). Meanwhile a search for the stripped form `"Him & I"` returns 20 records including the canonical `"Him & I"` by `"G-Eazy feat. Halsey"` at 269s — synced. The retry to the stripped form was already implemented in v0.10.5 but was gated on `try_search_lrclib` returning zero records, which doesn't fire when the first pass returned the wrong-titled records. Retry logic moved up into `fetch_lrclib` and now fires whenever the first-pass `pick_best` returns None — covering both "zero records returned" AND "records returned but all filtered out" cases.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — deleted the `try_search_lrclib` wrapper that did empty-records-only retry. `fetch_lrclib` now calls `try_search_lrclib_once` directly in the parallel `tokio::join!` with `try_get_lrclib`, then after the first-pass `pick_best` fails, runs a sequential second-pass: compute `strip_youtube_noise(title)`, if it changed, fire another `try_search_lrclib_once` with the stripped query, then `pick_best(records, &stripped, ...)` — passing the stripped form to pick_best so the substring check matches the cleaner record titles LRCLib returns for clean queries. Net cost: +0 API calls on tracks where the first pass works, +1 API call on tracks that need the retry (which were previously NotFound anyway). User-Agent bumped to `hum/0.10.12`.

### Diagnostic notes
- For the G-Eazy case the resolution chain is now: `clean_title("G Eazy & Halsey - Him & I (Lyrics)")` strips `(Lyrics)` → `"G Eazy & Halsey - Him & I"` → first search returns 3 unsynced wrong-titled records → `pick_best` filters them all out (hyphen mismatch + no duration match on the 286s "(Official Video)" records vs the 269s actual song) → retry fires → `strip_youtube_noise` drops `"G Eazy & Halsey - "` prefix → second search for `"Him & I"` returns the synced 269s record → match.
- The retry's `pick_best` is called with the stripped title rather than the original because the second-pass records have CLEAN track_names (e.g. `"Him & I"` directly), so substring matching against the stripped title is correct. Passing the original would also work in most cases (the stripped title is a substring of the original) but adds noise to the substring check for titles like `"Him & I (with Halsey)"` where the parens content doesn't appear in the original.

## [0.10.11] - 2026-05-21

### Fixed
- **LRCLib now finds "Fleetwood Mac - Dreams (Official Audio)" and similar.** The bracketed `cleaner()` regex hardcoded "Official" as a required prefix for `Video` variants (`Official Video`, `Official Music Video`, `Official Lyric Video`, `Official HD Video`) but treated `audio` and `visualizer` as standalone-only tokens. So `(Official Audio)` and `(Official Visualizer)` survived `clean_title` unchanged, leaving query strings like `"Fleetwood Mac - Dreams (Official Audio)"` to hit LRCLib's fulltext index — which returned zero rows because of the noise. Even the `strip_youtube_noise` retry (v0.10.5) couldn't save it because that fallback only handles `" - "` prefixes and `" feat. X"` suffixes, not parens content. The regex now accepts an optional `official\s+` prefix on `video`, `audio`, and `visualizer` alternatives — symmetric across all three. Same change to `pipe_tag_cleaner` (v0.10.9) so `Song | Official Audio` strips too. New variants caught: `(Official Audio)`, `(Official Music Audio)`, `(Official Visualizer)`, `(Official Animated Video)`, `| Official Audio`, `| Official Visualizer`. Resolution chain for the Fleetwood Mac case is now: `clean_title` strips `(Official Audio)` → search `"Fleetwood Mac - Dreams"` → likely zero hits → `strip_youtube_noise` drops `"Fleetwood Mac - "` → retry search `"Dreams"` → match.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — `cleaner()` and `pipe_tag_cleaner()` regexes both restructured. Each now has three normalized alternatives for video / audio / visualizer that accept optional `official\s+` and optional sub-modifier (`music\s+`, `lyric\s+`, `hd\s+`, `animated\s+`) before the base token. The `audio` and `visualizer` bare-token entries from before are now redundant under the new patterns (the optional `(?:official\s+)?` makes them match standalone too) — kept only the `lyrics?` standalone since "lyrics" without parentheses is its own special case. User-Agent bumped to `hum/0.10.11`.

### Known limitations
- LRCLib `/api/search` still doesn't get a retry with the stripped form when the FIRST pass returned records but `pick_best` filtered them all out (e.g. duration mismatch across cover versions). Out of scope for this fix; the cleaner change alone solves the Fleetwood case because the first search now returns zero, which triggers the existing retry.

## [0.10.10] - 2026-05-21

### Added
- **`Ctrl+Alt+B` global hotkey toggles the blurred album-art background.** Works from anywhere — the lyrics overlay, the desktop, a fullscreen game, a different app on top. Each press flips the `blur_album_art_background` setting, persists it to `settings.json`, and emits `settings-changed` so the overlay re-renders instantly. The on/off state survives app restarts since it's the same setting the Settings window exposes. No on-screen indicator fires — the visual change (background appearing or disappearing) IS the feedback. Joins the existing global hotkeys: **Ctrl+Alt+L** (cycle overlay mode), **Ctrl+Alt+[** (nudge lyrics earlier 250ms), **Ctrl+Alt+]** (nudge lyrics later 250ms).
- **Settings hint mentions the new shortcut.** Under "Blurred album art background" in Settings → Background, the description now ends with `Toggle on the fly with Ctrl+Alt+B`, rendered with a `<code>` tag for the keycap.

### Architecture / files
- **`src-tauri/src/lib.rs`** — new `toggle_blur` `Shortcut` (`Ctrl+Alt+B`, `Code::KeyB`) constructed alongside the existing cycle/nudge shortcuts. New branch in the global-shortcut handler: on `Pressed`, fetches the `SharedSettings` arc, spawns a Tokio task (the handler closure is sync but the settings RwLock is async), flips the bool, calls `settings::save_to_store`, and emits `settings-changed` with the new snapshot. Registered alongside the existing three in `register_hotkey` — adding `("Ctrl+Alt+B", toggle_blur)` to the per-shortcut registration loop so failures log per-key like the others.
- **`src/Settings.tsx`** — Hint paragraph updated with the keycap.

### Diagnostic notes
- The handler uses `tauri::async_runtime::spawn` to do the flip-and-persist work off the synchronous handler thread. Mirrors the pattern in `settings::persist_last_mode` for the mode-change persist. The `state.inner().clone()` returns an `Arc<RwLock<Settings>>` clone — cheap (refcount bump), not a deep copy of the settings.
- The frontend `listen("settings-changed")` subscription already exists and pipes into `applySettings(s)` in `Overlay.tsx`. No frontend changes needed — the new hotkey reuses the same channel that the Settings window's toggle uses.

## [0.10.9] - 2026-05-21

### Fixed
- **LRCLib now finds tracks like "Zach Bryan - Pink Skies | Lyrics".** The trailing `" | Lyrics"` (and other pipe-delimited YouTube uploader tags) was sitting outside the bracketed-noise cleaner, so the search query was `"Zach Bryan - Pink Skies | Lyrics"` — way too noisy for LRCLib's fulltext index to match the canonical `"Pink Skies"` record. Strip catches: `" | Lyrics"`, `" | Lyric"`, `" | Lyric Video"`, `" | Music Video"`, `" | Official Video"`, `" | Official Music Video"`, `" | Official Lyric Video"`, `" | Official HD Video"`, `" | Audio"`, `" | Visualizer"`, `" | HD"`, `" | UHD"`, `" | MV"`, `" | 4K"`, `" | 8K"`. Case-insensitive. Only stripped from the END of the title — interior pipes (e.g. `"Hard Out Here | Live at Glastonbury"`) are left alone. Combined with the existing v0.10.5 `strip_youtube_noise` retry that drops the leading `"Artist - "` prefix, the resolver chain for "Zach Bryan - Pink Skies | Lyrics" is now: clean_title strips `" | Lyrics"` → search "Zach Bryan - Pink Skies" → zero hits → retry with `strip_youtube_noise` → search "Pink Skies" → match.
- **Removed the temporary demo update banner.** The gold "New Update Available: v0.11.0-demo" pill that was firing on every launch was leftover demo code from when the v0.10.7 banner UX was being designed. The real updater check (`tauri-plugin-updater`'s `check()` against the configured endpoint) is still wired up — it just won't show a banner until the endpoint actually serves a `latest.json` with a newer version than what's installed.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — new `pipe_tag_cleaner()` regex (case-insensitive, same noise vocabulary as the bracketed `cleaner()` plus `4k` / `8k`). `clean_title()` applies both cleaners in sequence: bracketed first, then pipe-tag. User-Agent bumped to `hum/0.10.9`.
- **`src/Overlay.tsx`** — removed the `useEffect` that force-set `setUpdateState({ phase: "available", version: "0.11.0-demo", ... })` 800ms after mount, plus the `invoke("set_update_indicator", ...)` next to it. The genuine updater check that runs in the other `useEffect` (above the demo) is untouched.

### Known limitations
- Titles that use pipes for legit categorization at the END (e.g. `"Song | Album Name"`) would not be stripped — only the specific noise-keyword vocabulary is matched after the pipe. A hand-edited LRC search on LRCLib would still need to be done manually for those edge cases. Out of scope for this fix.
- `" - Lyrics"` / `" Lyrics"` suffixes (without the pipe delimiter) are NOT stripped. Too risky given there are legitimate song titles ending in "Lyrics" (e.g. punk / hardcore song titles). Re-evaluate if a real-world case surfaces.

## [0.10.8] - 2026-05-21

### Added
- **Blurred album-art background ("Now Playing" style).** The overlay now paints a heavily blurred, dimmed copy of the current track's album art as the window background, with the user's Background color rendering as a tint on top. Visually similar to Apple Music's full-screen "Now Playing" view: the cover art shape is soft, the dominant tones bleed across the whole pill, and the lyric text floats on a track-specific atmosphere instead of flat black. Default is **ON**, so users will see the change immediately after upgrading. Toggle is in **Settings → Background → "Blurred album art background"** with a hint underneath. Settings persist to `settings.json` as the new `blur_album_art_background: bool` field.
  - Renders on all three layout modes (3-line, single-line, full-page).
  - No-op when the current track has no album art (LRCLib-only matches without an SMTC source thumbnail, fresh app launch before the first art payload arrives, etc.) — the overlay falls back to the flat `bg_color` rendering exactly like before.
  - Plays nicely with the existing **"Tint background from album art"** toggle: when both are on, the gradient tint blends with the blurred photo. When only the blur is on, the user's literal `bg_color` (which defaults to `#000000` at 0% opacity = fully transparent) tints the blur — i.e., no extra darken. Crank Background opacity up to dim the blur further.
- **Karaoke per-word gradient sweep.** When the current track came from SimpMusic's richSyncLyrics (which provides word-level timing), the current lyric line is now rendered with a smooth left-to-right color wipe across each word's glyphs — the brighter "lit" color fills in from the left edge over the word's exact duration, replacing the previous abrupt dim→lit color flip. Words that have already passed stay fully lit; words still to come stay dim. Visual effect matches what people expect from karaoke / lyric-video apps. Falls through unchanged for sources without word timing (LRCLib, NetEase): the line still uses the simple opacity / color contrast between prev / current / next.

### Architecture / files
- **`src-tauri/src/settings.rs`** — new `pub blur_album_art_background: bool` field on the `Settings` struct (serde-default ON via `Settings::default`). No sanitize / clamp needed (boolean), but the field's documented as ON-by-default in the struct comment so future readers don't think the rename was a regression.
- **`src/types.ts`** — `Settings` type gains `blur_album_art_background: boolean` to match the Rust struct.
- **`src/Overlay.tsx`** — `DEFAULT_SETTINGS` mirrors the new field at `true`. New `showBlurBg` boolean (computed from the setting + current-track / current-art match) drives a new `BlurredAlbumBg` component that renders two stacked absolute layers: (1) the blurred image (`backgroundImage: url(...)` with `filter: blur(40px) saturate(1.35) brightness(0.62)`, sized with `top/left/right/bottom: -48px` so the blur's soft falloff doesn't show as a transparent halo at the window edges) and (2) the user's `bgRgba` as a tint on top (skipped entirely when transparent). The whole thing is wrapped in an `overflow: hidden` absolute box so the negative-inset doesn't create phantom scroll content in the full-page layout's `overflow: auto` container. Container's `background` switches from `bgRgba` to `"transparent"` when the blur is active, since the layer system handles the user tint above. `innerRowStyle`, `outerStackStyle`, and `LineRow`'s outer div all gain `position: "relative"` so the flex content paints above the absolutely-positioned blur layer (without explicit positioning, statically-positioned flex children paint BELOW positioned siblings in the same stacking context, regardless of DOM order). The karaoke `LineRow` render path was rewritten to use a 2-stop `linear-gradient(to right, lit 0% 50%, dim 50% 100%)` clipped to the text glyphs via `background-clip: text` + `WebkitBackgroundClip: text` + `color: transparent`. `background-position` slides the gradient under the text: past words sit at `0% 0%` (lit half visible), future words at `100% 0%` (dim half visible), the current word animates `100% → 0%` with `transition: background-position {dur}ms linear` where `dur` is the per-word duration computed from word-time gaps + line-end fallback (already existed via `wordDurationMs`).
- **`src/Settings.tsx`** — new Toggle under the existing "Tint background from album art" with a Hint paragraph describing the effect.

### Diagnostic notes
- The two layers (blur + bgRgba tint) deliberately render BEFORE NudgeBanner / UpdateBanner / lyrics content in the JSX tree. Combined with `position: relative` on the wrapping flex elements, this puts the visible content cleanly on top without needing `z-index` arithmetic. Adding negative-z-index would have required `isolation: isolate` on the container (to keep the layer above the parent's own background paint), which interferes with the auto-contrast luminance sampler — that path tries to measure the screen behind the window, not the overlay's own composited surface.
- The karaoke gradient stops at 50% rather than a soft falloff: a sharp boundary reads as a clear "filled to here" cursor, while a soft gradient would smear the leading edge and lose the timing signal. If a softer effect is wanted later, change `${lit} 50%, ${dim} 50%` to `${lit} 45%, ${dim} 55%` (5% feather either side) — no other math changes needed.
- The blur radius (40px) and dim brightness (0.62) were tuned against three reference tracks: a high-contrast pop cover, a near-monochrome jazz cover, and a busy collage cover. Lower brightness made dark albums unreadable; lower blur exposed identifiable image content under the lyrics, fighting the text. 0.62 keeps the dominant color recognizable without forcing the reader's eye to parse the art.

## [0.10.7] - 2026-05-21

### Fixed
- **Album art now shows on app launch when music is already playing.** Previously, opening the overlay while Spotify / Chrome / YouTube Music was mid-track showed lyrics but no artwork — the album art column stayed blank until the user skipped to a new song and back. The backend was firing `album-art-loaded` correctly on startup; the bug was on the frontend, where the `listen("album-art-loaded", …)` subscription is asynchronous (Tauri's `listen` returns a `Promise<() => void>`) and the event was being emitted before the listener had finished attaching. Tauri events are fire-and-forget — there's no replay for late subscribers — so the art landed in a void. Fix: backend now caches the last `AlbumArtPayload` in shared state alongside the snapshot, and a new `get_current_album_art` Tauri command returns it on demand. Frontend invokes that command on mount (after the listener is set up but in parallel with the existing `get_current_track` / `get_current_lyrics` invokes), populating `albumArt` state and triggering the dominant-color extraction. Works for both SMTC (Spotify, YouTube Music, anything Windows-aware) and iTunes paths.

### Architecture / files
- **`src-tauri/src/smtc.rs`** — new `pub type SharedAlbumArt = Arc<RwLock<Option<AlbumArtPayload>>>` type alias. `pub fn start`, `async fn run`, `emit_full`, and `spawn_art_fetch` all gain an `art: SharedAlbumArt` parameter. In `spawn_art_fetch`, the payload is written to the shared cache BEFORE the `album-art-loaded` event is emitted, so a `get_current_album_art` invocation that races the event listener always sees a value at least as fresh as whatever the listener will receive.
- **`src-tauri/src/itunes.rs`** — same threading. The track-change emit block now builds the `AlbumArtPayload` locally, writes it to the cache, then emits — replacing the previous inline `&AlbumArtPayload { … }` construction.
- **`src-tauri/src/lib.rs`** — new `let album_art: SharedAlbumArt = Arc::new(RwLock::new(None))` initialized in `run()`, managed via `.manage(album_art)`, captured into the `.setup` closure as `art_state`, and passed to both `smtc::start` and `itunes::start`. New `get_current_album_art` Tauri command (`async fn`, returns `Result<Option<AlbumArtPayload>, String>`) registered in `invoke_handler!`. The non-Windows stub `mod smtc` block now also declares `AlbumArtPayload` and `SharedAlbumArt` so the import line and the command signature compile cross-platform.
- **`src/Overlay.tsx`** — new `invoke<…>("get_current_album_art")` call alongside the existing initial-state invokes. On success, calls `setAlbumArt(art)` and kicks off `extractDominantColor(art.data_url).then(setTintColor)` to match the event-listener path's behavior. On `null` (no art yet — no active session or source doesn't expose a thumbnail), does nothing; the listener catches the next emit normally.

### Diagnostic notes
- The race only manifested on fresh app launch with a session already active. Track-change after launch always worked because by then the listener had been attached for tens-of-seconds and was guaranteed to catch new events.
- The cache lives in memory only — no persistence to disk. On next launch the artwork has to be re-fetched from SMTC anyway (the source app exposes the thumbnail; we just bridge it). Persistence would save a few hundred ms but isn't worth the disk I/O / staleness risk.

## [0.10.6] - 2026-05-21

### Changed
- **Update banner no longer overlaps the lyrics.** The "New Update Available: v0.X.Y — Click to update" pill used to be absolutely positioned at the top-left of the inner row (`position: absolute; top: -4; left: 0`), which placed it on top of the previous lyric line. Now it sits in normal flow as the first child of a new vertical-stack container, with the art+lyrics row below it and a 4px gap between. When no update is pending the banner returns `null` and the stack collapses to just the row — no visual change from before. When an update is pending the overlay window auto-resizes up by ~24px to accommodate the banner (the `ResizeObserver` target was moved from the inner row onto the outer stack), and the gold dot + label sit cleanly above the "It's always half and never whole" / previous-line position instead of crashing into it.

### Architecture / files
- **`src/Overlay.tsx`** — new `outerStackStyle` (column flex, align-stretch, 4px gap) wraps the existing `innerRowStyle` (now row-only, `position: relative` removed since nothing's absolute inside anymore) and the `<UpdateBanner>` component. The `setInnerRowEl` ref (which feeds the `ResizeObserver` → `setSize(LogicalSize)` chain that auto-sizes the overlay window to content) moves from the row to the outer stack so banner height is included in the resize math. `UpdateBanner`'s `wrapperStyle` loses `position: absolute`, `top`, `left`, `zIndex`; gains `alignSelf: "flex-start"` so the dot+label hug the left edge instead of stretching to full width. Inner padding tightens from `4px 6px` to `2px 4px` so the banner sits flush against the top edge of the window.
- **`src-tauri/src/lib.rs`** — ghost-mode "click hole" zone moves from a top-left ~360px-wide band covering the middle 60% vertical (which targeted the OLD centered-banner position) to a top-left 360×48px rectangle at the very top of the window content. Catches cursor over the banner's new flow position. New `BANNER_ZONE_H: i32 = 48` const inside the polling closure. Vertical math simplified from `(height/5, height*4/5)` to `(0, BANNER_ZONE_H)`.

## [0.10.5] - 2026-05-21

### Fixed
- **LRCLib search now retries with YouTube-style noise stripped when the first pass returns zero records.** Specific case the 0.10.4 release missed: a YouTube video titled `"T-Pain - Bartender (Official HD Video) ft. Akon"` cleans to `"T-Pain - Bartender ft. Akon"` after `clean_title` (which only strips parenthesised/bracketed noise). LRCLib's fulltext `/api/search?track_name=...` returns zero records for that 5-token query when the canonical stored track is just `"Bartender"` (1 token). The overlay surfaced as "♪ no lyrics for T-Pain - Bartender (Official HD Video) ft. Akon" even though LRCLib has the song. Now: when the first `/api/search` call returns empty, we retry once with `strip_youtube_noise()` — drops the leading `Artist - ` prefix and trailing ` ft. X` / ` feat. X` / ` featuring X` (case-insensitive, without parens requirement). For the T-Pain case this becomes `"Bartender"`, which finds the right record. `pick_best` then confirms via duration ±5s.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — `try_search_lrclib` is now a wrapper that calls a new `try_search_lrclib_once`. First pass uses the cleaned title as-given; if empty, retries once with the output of `strip_youtube_noise`. New `strip_youtube_noise(title) -> String` function: (1) regex strips trailing ` (feat\.?|ft\.?|featuring)\s+.+$`, (2) `String::find(" - ")` strips the leading prefix when the post-strip candidate still has ≥2 non-whitespace chars (avoids eating the whole title for fragments like `"A - B"`). Conservative on purpose — runs only as fallback when the baseline search already returned zero. User-Agent bumped to `hum/0.10.5`.

### Known limitations
- Titles with legit embedded ` - ` like `"Born In The U.S.A. - 1984 Remaster"` will, on the retry fallback, strip to `"1984 Remaster"` and almost certainly still find nothing on LRCLib. Net result: NotFound, no worse than the status quo. The retry only fires when the baseline already returned zero, so the false-positive cost is "we still don't find lyrics" — never worse.
- LRCLib `/api/search` does not currently get a retry with the aggressive form when the FIRST pass returned records but `pick_best` filtered them all out (e.g., duration mismatch). Could matter for medleys / extended remixes where the YouTube duration diverges from LRCLib's stored version by >5s. Out of scope for this fix.

## [0.10.4] - 2026-05-21

### Fixed
- **LRCLib now finds the right lyrics for YouTube tracks where SMTC reports a punctuation-stripped artist.** Specific case: YouTube's Chrome SMTC bridge reports T-Pain's channel as `"TPainVEVO"`. `clean_artist` strips the trailing `VEVO` → `"TPain"`. But LRCLib stores the canonical artist as `"T-Pain"` (with the hyphen). Passing `artist_name=TPain` to `/api/search` applied a strict filter that returned zero results — a false NotFound for an extremely common track. Fix: `/api/search` no longer takes an `artist_name` param at all. We rely on `pick_best`'s bidirectional title-substring filter + ±5s duration filter to disambiguate downstream. Cost: slightly noisier search results for ambiguous titles ("Closer" matches both Chainsmokers and Nine Inch Nails), mostly absorbed by the duration filter. Benefit: false NotFounds from artist-string drift (`TPainVEVO` → `TPain` ≠ `T-Pain`, ` - Topic` artists, abbreviated channel names) no longer happen.
- **"♪ error fetching lyrics" no longer fires when only a peer source errored.** Previously the resolver treated `errors.is_empty() == false` as "fetch failed" — meaning a single simpmusic timeout would override LRCLib's perfectly clean "no match" reply and surface as the red error state on the overlay. Now we distinguish two cases: (a) at least one source authoritatively replied NotFound → status is `NotFound` ("♪ no lyrics for X") even when peers errored; the peer errors still flow through to `CurrentLyrics.errors` so the dev console surfaces them as debugging signal. (b) Every source errored → status is `Error` ("♪ error fetching lyrics"), as before. This matches the actual semantic: "error fetching" should mean we couldn't get *any* signal, not "one source out of three is having a bad day."

### Changed
- **NotFound state in the dev console can now show per-source peer errors.** When at least one source replied NotFound cleanly but a peer timed out / 5xx'd, the dev console's LYRICS section now shows the not-found message plus an amber monospace box listing the peer-source errors. Hover tooltip reads: "Authoritative miss from at least one source; these are peer-source errors that didn't change the outcome." Color is amber (`#fbbf24` on `#1a1610`) instead of the harder red of the actual-error box, since these errors didn't change the resolved status.
- **NotFound dev-console copy** changed from "No lyrics on LRCLib for X" to "No lyrics found for X" since the resolver checks three sources, not just LRCLib.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — `try_search_lrclib` signature drops the `artist: &str` param (caller in `fetch_lrclib` updated accordingly). Cache-skip behavior in `resolve_lyrics` reworked around a new `any_clean_notfound: bool`: each source's Ok-NotFound branch sets it to true; final decision branches on `any_clean_notfound` (→ Outcome::NotFound with errors carried through) vs all-errored (→ Outcome::error). Persist-to-store happens only when fully clean (no peer errors), so a false-NotFound caused by a peer-source-down day doesn't get cached and re-served on next track-change. User-Agent bumped to `hum/0.10.4`.
- **`src/DevConsole.tsx`** — NotFound branch now wraps in an outer `<div>` and conditionally renders the amber monospace error box when `lyrics.errors?.length > 0`. Color palette distinct from the red error-state box.

### Diagnostic notes
- The full chain for the T-Pain Bartender case before this fix: SMTC reported `artist='TPainVEVO'` (no hyphen, no space). `clean_artist` stripped `VEVO` → `"TPain"`. `clean_title("T-Pain - Bartender (Official HD Video) ft. Akon")` stripped `(Official HD Video)` → `"T-Pain - Bartender ft. Akon"` (the dangling `ft. Akon` outside brackets is not stripped by the existing `cleaner` regex). LRCLib `/api/get` 4xx'd (no exact match) → `Ok(None)`. LRCLib `/api/search?track_name=...&artist_name=TPain` returned zero records because LRCLib's artist filter wanted `T-Pain`. `fetch_lrclib` returned `Ok((NotFound, "lrclib"))`. simpmusic timed out → `Err`. NetEase returned `Ok(NotFound)`. Old logic: `errors.is_empty() == false` → `Outcome::error` → red overlay. New logic: `any_clean_notfound = true` from lrclib + netease → `NotFound` with simpmusic timeout passed through as a peer error in the dev console. New `try_search_lrclib` (no artist param) returns LRCLib's "Bartender" records by T-Pain (and others); `pick_best` keeps records where the cleaned title contains the record title (`"bartender"` ⊆ `"t-pain - bartender ft. akon"`), filters by duration ±5s, prefers synced over plain, and returns T-Pain's record. Overlay should now show scrolling synced lyrics for this track.

## [0.10.3] - 2026-05-21

### Fixed
- **"♪ error fetching lyrics" no longer fires on YouTube tracks with auto-generated channel chrome.** The overlay's middle-line red error state was firing on YouTube Music / Topic-channel videos because LRCLib's `/api/search` endpoint returns HTTP 400 when given noisy query params (e.g. an artist like `"Foo Bar - Topic"` or a title containing characters its query parser rejects), and `try_search_lrclib` was treating ANY 4xx as a transient `Err` instead of a clean "no match." With three lyric sources cascading through `last_error.is_some()`, a single 400 from search was enough to push the resolver into `Outcome::error()` even when the other two sources cleanly returned `NotFound`. Two YouTube tracks in a row showing "error fetching lyrics" is the symptom; the fix is making `/api/search` 4xx behave like `/api/get` 4xx (return `Ok(empty)` instead of `Err`). Source: `src-tauri/src/lyrics.rs::try_search_lrclib` no longer calls `.error_for_status()?`.
- **YouTube Topic-channel videos now resolve lyrics.** Previously the resolver hard-skipped any track with an empty artist field (`src-tauri/src/lyrics.rs:139-141`'s `if snap.artist.trim().is_empty() { continue; }`), which meant Topic channels — where YouTube ships song titles with no artist metadata — silently kept showing the *previous* track's lyrics with no UI indication of the staleness. The skip is removed; an empty-artist track now flows into the resolver, which runs a title-only LRCLib `/api/search` (omitting the empty `artist_name` param) plus SimpMusic + NetEase (both already tolerate empty artist in their pick-best filters). The result either lands a lyric or surfaces an honest "no lyrics for X" instead of stale lyrics.
- **Artist noise is now stripped before fetching.** New `clean_artist()` regex strips trailing ` - Topic`, ` VEVO`, ` - Official Artist Channel`, ` - Official`, ` (Official Artist Channel)`, ` (Official)`, ` [Topic]`, plus dangling dashes/whitespace. Mirrors the existing `clean_title` cleaner (which strips parenthesised noise like `(Official Music Video)`, `[Lyrics]`, `(Audio)`, `(feat. X)`, remaster/live/acoustic markers). Interior text is intentionally untouched so legitimate hyphenated band names ("Crosby, Stills, Nash & Young", "Earth, Wind & Fire") are not corrupted. Applied once at the top of `resolve_lyrics` so all three sources see the cleaned strings.

### Added
- **Dev console now shows the actual per-source error when lyric fetch fails.** Open the dev console (system tray → "Show / Hide dev console") → the **LYRICS** section. When status is `error`, a red monospace box renders beneath the existing error message, listing one line per failed source: `lrclib: /api/search returned 502`, `simpmusic: connection timed out`, `netease: dns failure for music.163.com`, etc. Each entry is the wrapped `anyhow` chain captured during `resolve_lyrics`. Implementation: new `errors: Vec<String>` field on the `CurrentLyrics` struct (Rust) and `errors?: string[]` on the matching TS type, flowing from `Outcome::error(errors)` → `apply_outcome` → emitted `lyrics-state` / `lyrics-not-found` events. Cleared on each new track to avoid stale entries leaking between fetches.

### Architecture / files
- **`src-tauri/src/lyrics.rs`** — main file. `try_search_lrclib` no longer panics on 4xx (explicit `is_client_error()` → `Ok(Vec::new())`); also now omits `artist_name` param entirely when artist is blank so LRCLib doesn't see an empty-string param. `try_get_lrclib` now early-returns `Ok(None)` on blank artist (no point firing a doomed exact-match against blank metadata when search runs in parallel anyway). New `artist_cleaner()` static regex + `clean_artist()` helper, structured like the existing `cleaner()` + `clean_title()`. New `errors: Vec<String>` field on `CurrentLyrics` (Rust struct, serde-default + skip-if-empty) and `Outcome` (resolver internal). `Outcome::error(errors)` takes the collected per-source error list. `resolve_lyrics` now collects errors instead of overwriting a single `Option<String>`. `apply_outcome` writes `s.errors = out.errors`. The track-changed worker's "Mark fetching" block now sets `errors: vec![]` so stale errors from a previous track don't leak into the dev console. The empty-artist skip in the worker loop is removed (now handled by the resolver itself). User-Agent bumped to `hum/0.10.3`.
- **`src/types.ts`** — `CurrentLyrics` gains `errors?: string[]`.
- **`src/DevConsole.tsx`** — the `status === "error"` branch now renders a red monospace box listing `lyrics.errors` (when present). Plain text join, one error per line, no truncation. Background `#1a0d0d`, border `#3a1a1a`, text `#fca5a5` to keep it visually distinct from the existing error message.
- **Not changed:** `fetch_simpmusic` and `fetch_netease` already handled 4xx correctly (their `if status.is_client_error()` branches return `Ok((NotFound, source))`). Their pick-best functions already skip artist filtering when the artist string is empty. No changes needed there.

### Diagnostic path for future "error fetching lyrics" reports
1. Open the dev console (tray → "Show / Hide dev console").
2. Replay the track that errored.
3. The red monospace box under the LYRICS section will show the exact per-source failure (lrclib / simpmusic / netease + the wrapped reqwest error).
4. If all three say "no match" rather than network errors → the metadata cleaning isn't catching that case; extend `clean_title` or `clean_artist`.
5. If one says network error and the others succeed → that source is having a bad day; the overlay still resolved correctly via the other two.

## [0.10.2] - 2026-05-21

### Changed
- **App renamed from "Lyric Overlay" to "Hum".** Every user-visible string carrying the old name is updated: the system tray tooltip ("Hum — edit/locked/ghost mode"), the tray menu's "Quit" item ("Quit Hum"), the three Tauri window titles (overlay window: "Hum"; dev console: "Hum — Dev console"; settings: "Hum — Settings"), the dev console `<h1>` ("Hum — SMTC + iTunes + LRCLib dev console"), the OBS streamer browser-source page title ("Hum — OBS source"), and the Settings footer line showing where settings live ("Stored at %APPDATA%\com.syvr.hum\settings.json"). Underlying identifier `com.syvr.lyric-overlay` → `com.syvr.hum`. NSIS installer filename changes to `Hum_0.10.2_x64-setup.exe`. Updater endpoint moves to `https://github.com/basezero-projects/Hum/releases/latest/download/latest.json`. **Existing settings on disk do not migrate** — the new install reads from a fresh `%APPDATA%\com.syvr.hum` directory; the old `%APPDATA%\com.syvr.lyric-overlay` directory is left in place untouched (delete it manually if you don't want it lingering).

### Architecture / files
- **Identifier surface**: `package.json::name` (`lyric-overlay` → `hum`), `src-tauri/Cargo.toml` (`name`, `default-run` → `hum`; `lib.name` → `hum_lib`), `src-tauri/src/main.rs` (`lyric_overlay_lib::run()` → `hum_lib::run()`), `src-tauri/tauri.conf.json` (`productName`, `identifier`, 3 window titles, updater endpoint URL).
- **User-facing strings**: `index.html` `<title>`, `src/DevConsole.tsx` `<h1>`, `src/Settings.tsx` settings-path display, `src-tauri/src/lib.rs` tray quit-item + tooltip format string, `src-tauri/src/mode.rs` tray tooltip format string, `src-tauri/src/streamer_overlay.html` `<title>`.
- **Internal references**: `src-tauri/src/lyrics.rs` LRCLib User-Agent string (`lyric-overlay/0.1.0` → `hum/0.10.2`, repo URL updated), `src-tauri/src/itunes.rs` temp-file prefix (`lyric-overlay-itunes-` → `hum-itunes-`), `src-tauri/scripts/itunes_poll.ps1` comment.
- **Repo move**: project pushed to `https://github.com/basezero-projects/Hum` for the first time (prior commits were local-only). Folder path on disk moves from `D:\Work\App_Projects\All_Projects\lyric-overlay\` to `D:\Work\App_Projects\All_Projects\Hum\`.

## [0.10.1] - 2026-05-21

### Changed
- **Overlay window height now hugs its content.** The window no longer has empty vertical space below or above the lyrics row — height auto-resizes the instant content height changes (track change, font-size tweak in Settings, banner appearing/disappearing). Default window height dropped from 200px to 130px on first launch. You can still drag the right edge to widen the overlay (wider = bigger text via auto-fit), but dragging the bottom edge is effectively a no-op — the next layout fire snaps height back to content. Implemented in `src/Overlay.tsx` with a `ResizeObserver` on the inner row element calling `getCurrentWindow().setSize(new LogicalSize(w, h))` whenever the row's `offsetHeight` changes.

### Added
- **Ghost mode keeps the update banner clickable.** Previously, in ghost mode (where the whole overlay is click-through and your mouse passes right through to whatever's behind it), the gold "update v0.X.Y → click to install" banner pinned to the top-right couldn't actually be clicked — clicks went straight through to the app underneath. Now a small banner-shaped zone in the top-right corner stays clickable while the rest of the overlay remains pass-through. Implementation: a background worker in `lib.rs` polls the Windows cursor position every ~40ms while in ghost mode AND the banner is visible, and toggles `set_ignore_cursor_events` on/off based on whether the cursor sits in the banner zone. No effect outside ghost mode (edit + locked modes already had normal click handling).
- **Tray menu item label flips when an update is detected.** The "Check for updates" item in the system tray menu now changes to "Install update v0.X.Y" once the overlay's startup check (or a manual click) finds a newer version. Clicking it in either state does the right thing — runs a fresh check if no update is known, or triggers the install + relaunch sequence if one is already downloaded and ready. Makes the tray actionable on its own without needing to see the overlay banner.

### Architecture / files
- **`src/Overlay.tsx`** — new `innerRowEl` ref + `ResizeObserver` effect drives `setSize(LogicalSize)` on every height change. `updateStateRef` mirrors `updateState` so the single `updater-check-requested` tray-event listener (created once on mount) can branch on the latest phase without re-subscribing. Two new `invoke()` calls — `set_update_indicator({ pendingVersion })` whenever the update phase enters/exits "available", and `set_update_banner_visible({ visible })` whenever the banner mounts/unmounts — feed the Rust side.
- **`src-tauri/src/lib.rs`** — new `UpdateMenuItem` managed-state wrapper holds the `MenuItem<Wry>` handle for the "Check for updates" tray entry so `set_update_indicator` can rewrite its text. New `Arc<AtomicBool>` managed-state holds the banner-visibility flag for the cursor-poll worker. New background `tauri::async_runtime::spawn` worker polls `GetCursorPos` from `windows::Win32::UI::WindowsAndMessaging` every ~40ms; when mode is ghost AND banner is visible, it computes the overlay window's screen rect, derives a banner-shaped zone (top-right corner, height-fifth bands), and flips `set_ignore_cursor_events(!in_zone)`. Two new Tauri commands registered in the `invoke_handler!`: `set_update_indicator`, `set_update_banner_visible`.
- **`src-tauri/Cargo.toml`** — `windows` crate gains `Win32_UI_WindowsAndMessaging` + `Win32_Foundation` features for `GetCursorPos`.
- **`src-tauri/tauri.conf.json`** — overlay window default `height: 200` → `height: 130` so first-launch isn't a giant black bar.

## [0.10.0] - 2026-05-14

### Removed
- **AI Commentary window + Claude API integration removed entirely.** Cost concern: an Anthropic API key was required, every unique track triggered a paid call, and the value (decorative trivia) didn't justify the recurring expense for keeping the app free. Removed: `commentary.rs` module, `Commentary.tsx` window, `commentary` window declaration in `tauri.conf.json` + capabilities allowlist, `claude_api_key` field from `Settings`, "AI Commentary…" tray menu item, "Commentary" section from the Settings UI. Saved `claude_api_key` value in any existing user's `settings.json` is now an orphan field — serde ignores unknown fields on load, so no errors. To fully clean it from disk, just open Settings and toggle anything (next save writes only the current schema).

### Added
- **In-overlay auto-update banner.** A small gold pill — `update v0.X.Y → click to install` — pinned to the top-right of the overlay window appears whenever a newer version is available. No popup dialogs, no dev console intrusion: the banner lives inside the overlay you already see. Click it → the app downloads the new NSIS installer in the background (banner switches to `installing v0.X.Y…`), installs it (`v0.X.Y installed → restarting`), and relaunches itself within ~1s. Total UX cost of an update = one click. If an update fails (e.g. network drop mid-download), the banner switches to red `update failed` instead of crashing.
- **Tray menu item: Check for updates.** Sits between **Settings…** and **Show / Hide dev console**. On click, re-runs the same update check the overlay performs at startup. Useful for forcing a check between official releases or after fixing a network problem.

### Architecture / files
- **`src-tauri/Cargo.toml`** — added `tauri-plugin-updater = "2.10.1"` and `tauri-plugin-process = "2.3.1"`. Removed the in-house Claude API client (was using the existing `reqwest` dep).
- **`package.json`** — added `@tauri-apps/plugin-updater` and `@tauri-apps/plugin-process`.
- **`src-tauri/src/lib.rs`** — registers both plugins on the Tauri Builder. New tray menu handler for `"check-updates"` emits the `updater-check-requested` event so the overlay's frontend code owns the entire check + install + relaunch sequence (single source of UI feedback). Dropped: `mod commentary`, the `commentary::CommentaryCache` managed state, the `commentary::get_track_commentary` and `open_commentary_window` Tauri command registrations, and the AI Commentary tray menu item + handler.
- **`src-tauri/tauri.conf.json`** — `bundle.createUpdaterArtifacts: true` tells Tauri's bundler to emit the updater manifest alongside the NSIS installer. New `plugins.updater` config: `endpoints` points at `https://github.com/syvrstudios/lyric-overlay/releases/latest/download/latest.json` (the standard `tauri-action` GHA workflow output path — works once the repo is pushed and a release exists), `windows.installMode: "passive"` so the user sees a brief installer progress bar but no clicks required. `pubkey` is intentionally empty for now; required when the repo + release pipeline is set up. Removed the `commentary` window declaration.
- **`src-tauri/capabilities/default.json`** — added `updater:default` and `process:allow-restart`. Removed `commentary` from the windows allowlist.
- **`src/Overlay.tsx`** — imports `check` from `@tauri-apps/plugin-updater` and `relaunch` from `@tauri-apps/plugin-process`. New `updateState` state machine (`idle → available → downloading → ready → restart`, with `error` as a terminal alt branch). `useEffect` runs `check()` once on mount + on `updater-check-requested` events. New `UpdateBanner` component handles rendering for each phase (gold pill, position absolute, top-right of overlay). Click-to-install triggers `update.downloadAndInstall()` then `relaunch()` after an 800ms beat so the user sees the "ready" badge.
- **Removed:** `src-tauri/src/commentary.rs`, `src/Commentary.tsx`, all references in `lib.rs` / `main.tsx` / `Settings.tsx` / `types.ts` / `Overlay.tsx::DEFAULT_SETTINGS`.

### Notes / what's needed before the updater actually works
- Push the repo to GitHub (Tauri policy still says ask first — Wes hasn't OK'd this).
- Set up a `tauri-action` GitHub Actions workflow that builds the NSIS installer + updater manifest on every release tag (`v*`) and publishes both to the GitHub Release.
- Generate a Tauri updater signing keypair (`pnpm tauri signer generate -- -w ~/.tauri/lyric-overlay.key`), commit the public key into `tauri.conf.json::plugins.updater.pubkey`, and add the private key + password as `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` GHA secrets.
- Until then, the in-overlay update check returns silently (no endpoint reachable) and no banner ever appears. The infrastructure works as soon as the first release exists.

## [0.9.1] - 2026-05-14

### Fixed
- **AI Commentary no longer fires Claude API calls when the window is hidden.** Previously, once you'd opened the Commentary window once, its `track-changed` listener stayed active even after you closed (hid) the window — every new track meant another API call you couldn't see and didn't asked for. Now the React component checks `document.visibilityState` and skips the fetch when hidden. When you re-open the window it fetches the current track immediately so the body isn't stale.
- **Commentary text now reads less like an AI wrote it.** The Claude prompt has hard bans on the dead-giveaway patterns: em-dashes, "essentially / basically / ultimately / really / truly / arguably", sentence shapes like "It's not just X, it's Y" and "X is a Y that Z", lyric-summarizing back at the listener, vague attributions ("many critics"), promotional adjectives ("iconic / legendary / timeless / groundbreaking / classic"), and triple-list rule-of-three structures. The prompt asks for concrete specifics — sample names, years, producers, places, beefs, chart positions, cover origins — over general vibes. Cached responses generated with the old prompt clear on next binary restart (in-memory cache).

### Changed
- **Commentary window typography is no longer a wall of text.** The body now splits the response into paragraphs of ~1-2 sentences each (sentence-boundary detection looks for `.`, `!`, `?` followed by space + capital letter), with the lead paragraph rendered slightly larger (15px) and at full opacity, follow-on paragraphs at 14px and 78% opacity for visual hierarchy. Body has a 56-character readable measure (max-width capped) and 20×22px padding so nothing slams into the card edges. The `FRESH FROM CLAUDE` / `CACHED` indicator at the bottom shrunk from 11px to 10px and dropped to 35% opacity since it's metadata, not content.

## [0.9.0] - 2026-05-14

### Added
- **AI Commentary window** — a new tray menu item, **AI Commentary…**, opens a dedicated window (480×280, decorated, opens centered) that displays a 2-3 sentence Claude-generated context note for whatever track is currently playing. The note covers what the lyrics reference, the era / cultural moment, notable samples or callbacks the listener might miss, or one unusual fact. Updates automatically when the track changes. Layout is dark themed to match the Settings window: gold accent for the track title in the header, a card body containing the commentary text, and a small lowercase "fresh from claude" or "cached" tag at the bottom-right of the card so you know whether a fresh API call was made or the answer came from in-process cache.
- **`Settings → Commentary` section** with a single password-style input for your **Claude API key** (Anthropic). Stored in plaintext in `settings.json` (rotate via the Anthropic console if compromised). Empty key = the Commentary window shows "No Claude API key set" and never makes a network call. Get a key at console.anthropic.com — they have a generous free tier and Sonnet 4.5 calls for this 200-token-max prompt cost a fraction of a cent each.

### Architecture / files
- **`src-tauri/src/commentary.rs` (new)** — Tauri command `get_track_commentary(title, artist, album)` that hits Anthropic's `/v1/messages` endpoint via the existing `reqwest` client (no new crates), with the `claude-sonnet-4-5` model and `max_tokens=220`. In-memory `CommentaryCache` (managed Tauri state, `Arc<RwLock<HashMap<String, String>>>`) keyed by `artist|title|album` so replays don't re-spend API tokens. Returns `Commentary { track_key, text, source: "cache"|"api"|"empty"|"error", error }` so the frontend can surface why a result is empty.
- **`src-tauri/src/lib.rs`** — `mod commentary;`, manages the cache, registers `commentary::get_track_commentary` and `open_commentary_window` Tauri commands, adds **AI Commentary…** tray menu item between **Settings…** and **Show / Hide dev console**.
- **`src-tauri/src/settings.rs`** — new `claude_api_key: String` field (default empty), trimmed + length-capped at 500 chars in `sanitize`.
- **`src-tauri/tauri.conf.json`** — new `commentary` window declared (decorated, 480×280, `visible: false`, `skipTaskbar: false`).
- **`src-tauri/capabilities/default.json`** — `commentary` added to the windows allowlist.
- **`src/Commentary.tsx` (new)** — listens for `track-changed`, dedupes against the last-fetched key (iTunes pause/resume sometimes fires the same track-changed twice), invokes `get_track_commentary`, renders text. Three placeholder states: no API key set, loading, error.
- **`src/main.tsx`** — `commentary` window label routes to the new component.
- **`src/Settings.tsx`** — new **Commentary** section with password input + hint explaining where to get a key and the per-track caching behavior.
- **`src/types.ts`** + **`src/Overlay.tsx`** — `claude_api_key` added to `Settings` type and `DEFAULT_SETTINGS`.

## [0.8.0] - 2026-05-14

### Added
- **OBS / browser-source streamer mode.** A new section in **Settings → OBS / Streamer** with a toggle (**Expose lyrics as a browser source**), a port input (default 38247), and a **Browser source URL** row with a Copy button. When enabled, the desktop app spins up a local HTTP server on `http://localhost:<port>` that exposes:
  - **`GET /overlay`** (also at `/`) — a self-contained HTML page with inline CSS + JS that polls `/state` four times per second and renders the same 3-line lyrics layout (prev / cur / next, white text, dark drop shadow) the desktop overlay does. **Background is fully transparent**, so OBS browser source shows it cleanly over your stream without any chroma-key tricks. Recommended OBS source size: 1100×200.
  - **`GET /state`** — JSON snapshot of `{ track, lyrics, cursor, server_now_ms }`. The server computes the current cursor position itself (mirroring the desktop's rAF logic) so consumers don't have to interpolate. Useful for any third-party tool that wants the lyric state — not just OBS.
  - **`GET /healthz`** — minimal liveness probe returning `"ok"`.
- The streamer toggles take effect immediately — no app restart needed. Toggling off cleanly shuts down the server task and frees the port. Settings persist across restarts (saved to `settings.json`); if you had it on at last close, it auto-starts on launch.
- **Off by default** since enabling opens a TCP port on localhost.

### Architecture / files
- **`src-tauri/src/streamer.rs` (new)** — `start(app, port)` boots an axum 0.8 server on `127.0.0.1:<port>` with the three routes above, spawned on the existing tokio runtime via `tauri::async_runtime::spawn`. Returns a `ServerHandle` whose `Drop` impl sends a `oneshot::channel` shutdown signal to the server's `with_graceful_shutdown` so toggling streamer off cleanly stops it. `StreamerSupervisor` (held in Tauri-managed state) wraps a `Mutex<Option<ServerHandle>>` so `apply_settings(enabled, port)` can stop / start idempotently.
- **`src-tauri/src/streamer_overlay.html` (new)** — embedded via `include_str!`. ~150 lines of inline HTML/CSS/JS. Polls `/state` every 250ms; renders prev/cur/next with the same fonts and shadow as the desktop overlay; status fallback for fetching/not_found/instrumental/etc. Album art is hidden in v1 (would need an extra `/art` endpoint serving the data URL — easy follow-up if streamers ask).
- **`src-tauri/src/lib.rs`** — `mod streamer;`, manages `Arc<StreamerSupervisor>`, calls `streamer::apply_settings` once at setup with the loaded settings, and again from `update_settings` whenever settings change.
- **`src-tauri/src/settings.rs`** — `streamer_enabled: bool` (default false), `streamer_port: u16` (default 38247, clamped to ≥1024 in `sanitize`). `update_settings` now also calls `streamer::apply_settings` so toggling the UI live-starts / stops the server.
- **`src-tauri/Cargo.toml`** — `axum = "0.8.9"` added (with the `tokio` feature; pulls in `tower`, `hyper-util`, `serde_urlencoded`).
- **`src/Settings.tsx`** — new **OBS / Streamer** section with toggle + port input + `CopyableUrl` component (gold "Copy" button that flips to green "Copied" for 1.2s on click).
- **`src/types.ts`** + **`src/Overlay.tsx`** — `streamer_enabled` + `streamer_port` added to the `Settings` type and `DEFAULT_SETTINGS`.

## [0.7.7] - 2026-05-14

### Fixed
- **Text shadow now inverts with the text color when auto-contrast is on.** Previously the shadow was hardcoded black (a 6px drop + 14px halo), which worked great over dark backgrounds with white text but became invisible-or-worse when auto-contrast flipped the text to dark over a light background — the dark text + black shadow blurred together into mush. Now the shadow color tracks the text color: dark text gets a WHITE 6px drop + 14px halo (so the dark glyphs stay crisp against any light background); light text keeps the existing black drop + halo (works against any dark / mid-tone background). Wired through both `LineRow` and `TranslationRow` via a new `textShadow` prop computed in `Overlay.tsx`.

## [0.7.6] - 2026-05-14

### Changed
- **Auto-contrast text is now ON by default for fresh installs.** v0.7.0 shipped this feature off-by-default to avoid surprising existing users with color changes after upgrading. In practice the whole point of the overlay is "show lyrics over whatever you're doing on the desktop" — which means the background underneath is unpredictable, which is exactly when auto-contrast is most valuable. New installs will pick up the toggle as ON; existing settings.json files keep whatever value they had saved (no surprise change). To opt out, flip Settings → Extras → **Auto-contrast text** off.

## [0.7.5] - 2026-05-14

### Added
- **Live lyric offset nudge via global hotkeys.** When LRCLib (or one of the fallback sources) returns a lyric file with timestamps that don't match the audio — common on radio edits, remastered versions, and crowdsourced LRC for less-popular tracks — you can now correct it on the fly without opening Settings:
  - **Ctrl+Alt+]** pushes the lyrics 250ms LATER (use when the lyrics are showing ahead of the audio).
  - **Ctrl+Alt+[** pulls the lyrics 250ms EARLIER (use when the lyrics are lagging behind the audio).
  - Each press stacks; mash either key 4 times to shift ±1000ms, etc. The current offset value flashes briefly at the top-right of the overlay as a small gold-on-translucent indicator (`lyric offset +500 ms`) for 1.5 seconds after each press, so you know how much you've nudged.
  - **The nudge is session-only and resets to 0 automatically when the track changes**, so a one-off fix for a bad-LRC song doesn't bleed into the next one. There's no per-track persisted offset yet — that's a candidate for a later version if you find yourself nudging the same songs repeatedly.

### Architecture / files
- **`src-tauri/src/lib.rs`** — `build_global_shortcut_plugin` registers two new shortcuts: `Modifiers::CONTROL | Modifiers::ALT + Code::BracketLeft` and `BracketRight`. Each handler emits `lyric-offset-nudge` with a `-250` or `+250` payload. `register_hotkey` now registers all three shortcuts (cycle + nudge-back + nudge-forward) in a loop, with per-shortcut error logging if the OS denies the registration.
- **`src/Overlay.tsx`** — `nudgeMsRef` (read by the rAF tick), `nudgeBanner` state for the on-screen indicator, listener for `lyric-offset-nudge`, reset on `track-changed`. `lookupPositionMs` now returns `interpolatedPositionMs() + anticipate_ms - nudgeMs` (positive nudge = lyrics later = subtract from cursor lookup). New `NudgeBanner` component for the brief top-right flash.

## [0.7.4] - 2026-05-14

### Changed
- **Default overlay window width is now 1100px (was 720px)** so most lyric lines fit on one row without ellipsis truncation. The previous 720px default cut off mid-line on a lot of songs (anything beyond ~10–11 mid-length words at 26px font), which forced manual resizing on every install. 1100px fits the long-line case the previous size missed without becoming overwhelming on a 1920px screen. Height stays at 200px. Existing installs that already saved a custom width via the `tauri-plugin-window-state` plugin keep their saved value; only fresh installs (no `.window-state.json` yet) pick up the new default. The width-/height-based text scaling baseline in `Overlay.tsx` is also bumped to 1100×200 so the text size at the new default window matches what the **Current line size** slider value says.

## [0.7.3] - 2026-05-14

### Changed
- **Position / size persistence is now overlay-only.** v0.7.1 saved and restored window position + size for ALL three windows (overlay, dev console, settings). Now restricted to just the **overlay** window via the `tauri-plugin-window-state` plugin's `with_filter` callback. The dev console and settings windows always open at the position declared in `tauri.conf.json` (centered, default size) — they're transient debug / configuration windows that don't need their own preference memory.

## [0.7.2] - 2026-05-14

### Fixed
- **Dev console window stops popping up at launch (for real this time).** v0.6.2 set `visible: false` on the main window in `tauri.conf.json`, but two paths bypassed it in practice: (1) Tauri dev mode's hot-reload binary restart sometimes leaves the window visible briefly, and (2) the v0.7.1 `tauri-plugin-window-state` plugin's default behavior also restored last-known visibility, undoing the conf-file setting. Both paths now neutralized: the plugin's state-flag set is restricted to `POSITION | SIZE | MAXIMIZED` (no VISIBLE), and `setup()` calls `main.hide()` explicitly at the end of startup as a belt-and-suspenders guard. The dev console only appears when you right-click the tray and pick **Show / Hide dev console**.

## [0.7.1] - 2026-05-14

### Changed
- **Lyric text now scales with the overlay window in BOTH dimensions, not just height.** Previous v0.6.5 used `vh` units which only respond to height changes; dragging just the window's width narrower would crop the text instead of shrinking it. Switched to a JS-driven scale factor `min(window.innerWidth / 720, window.innerHeight / 200)` that's the smaller of the two ratios — so whichever side becomes the tighter constraint is the one that drives text sizing. Drag the window to half-width = text is half-size. Drag to half-height = text is half-size. Drag a wider but shorter window = text is bounded by height ratio. Album art on the side picks up the new lyrics-column height via the existing `ResizeObserver` chain and scales with it.

### Added
- **Overlay window position + size now persist across app restarts.** Drag the overlay to your preferred spot, resize it however you like, quit the app — next launch it comes back exactly where you left it. Implemented via the official `tauri-plugin-window-state` plugin (saves to `%APPDATA%\com.syvr.lyric-overlay\.window-state.json` on app close, restores on next launch). Same persistence covers the dev console and settings windows too — they remember their last position when you re-open them via the tray.

### Architecture / files
- **`src-tauri/src/lib.rs`** — `tauri_plugin_window_state::Builder::default().build()` registered on the Tauri Builder right after the existing `tauri_plugin_store` registration.
- **`src-tauri/Cargo.toml`** — `tauri-plugin-window-state = "2.4.1"` added.
- **`src-tauri/capabilities/default.json`** — `window-state:default` permission added so the plugin can call its own save/restore commands.
- **`src/Overlay.tsx`** — new `winSize` state (initialized to `window.innerWidth/innerHeight`), updated by a `resize` listener. New `scale` derivation `min(winSize.w/720, winSize.h/200)`. The existing `settingsForRender` derivation now also pre-scales `font_size_px` and `line_padding_px` by `scale` before passing to `LineRow` / `TranslationRow`, so all the size-using styles get the right pixel value without having to know about scaling. Removed the v0.6.5 `pxToVh` helper and its `vh` unit usages.

## [0.7.0] - 2026-05-14

### Added
- **Auto-contrast text color toggle in Settings → Extras → Auto-contrast text (read background, invert if needed).** When on, the lyrics overlay reads what's behind it every ~2 seconds via a Windows desktop-capture worker (`xcap` crate, sampling a 240×30 strip just below the overlay window — falls back to a strip above when the below sample lands off-screen) and flips the text color based on the average background luminance: light desktop → near-black text (`#0a0a0a` for current line, `rgba(0,0,0,0.45)` for prev / next dim) so lyrics stay readable over a white browser tab; dark desktop → near-white text (`#ffffff` and `rgba(255,255,255,0.45)`) so they stay readable over a game / dark IDE / dark browser. Hysteresis around the 0.5 luminance threshold (only flips light → dark below 0.45, dark → light above 0.55) prevents the text from flickering when the bg sits near mid-gray. **Off by default** because it overrides the Text color settings while active — turn it on if you find the lyrics hard to read over varying backgrounds.

### Architecture / files
- **`src-tauri/src/contrast.rs` (new)** — `start(app)` spawns a tokio task that wakes every 2s, queries the overlay window's outer position + size, samples a 240px-wide strip outside it, computes average RGB + luminance via the standard `0.299 R + 0.587 G + 0.114 B` weighting, emits `bg-luminance` Tauri event with `{ luminance: 0..1, r, g, b }`. Sampling outside the overlay (rather than inside) avoids a feedback loop where the overlay's own text glyphs would skew the read. First sample failure is logged once; subsequent failures are silent to keep stderr quiet.
- **`src-tauri/Cargo.toml`** — `xcap = "0.9.4"` added (cross-platform desktop capture; on Windows uses Direct3D 11 / DXGI desktop duplication).
- **`src-tauri/src/lib.rs`** — `mod contrast;` + `contrast::start(app.handle().clone())` called in `setup()` after the overlay window exists.
- **`src-tauri/src/settings.rs`** — `auto_contrast: bool` field added (default false). No new validator entry needed — bool is bool.
- **`src/Overlay.tsx`** — listens for `bg-luminance` events, maintains `bgIsLight: boolean | null` state with hysteresis, derives `effectiveTextColor` / `effectiveTextColorDim` when toggle is on, passes a copied `settingsForRender` (with the overrides applied) into all `LineRow` and `TranslationRow` instances.
- **`src/Settings.tsx`** — new toggle in Extras section with hint explaining the override.

## [0.6.5] - 2026-05-14

### Changed
- **Lyric text now scales with the overlay window size when you resize it in edit mode.** Previously dragging the window's bottom-right corner only changed the visible viewport — the text stayed the same fixed pixel size, so a smaller window just clipped it. Now font sizes (current line, prev / next dim lines, translation row) and the line-padding gap render in viewport-height units (`vh`) anchored to a baseline 200px window height. Drag the overlay shorter → everything in the lyric column shrinks proportionally → the side-by-side album art ResizeObserver picks up the new height and shrinks too. Drag taller → everything scales up. The **Current line size** slider value in Settings is the literal pixel size at the baseline 200px height; window 100px tall = half size, window 400px tall = double, linear in between.

## [0.6.4] - 2026-05-14

### Fixed
- **Album art no longer changes size as lyrics change.** When a long current-line lyric wrapped to 2 visual lines (the previous behavior — `-webkit-line-clamp: 2`), the lyrics column got taller, the side-by-side album art tracked that height via `ResizeObserver`, and the art card pulsed bigger then smaller as long lines came and went. Now the current line renders single-line just like prev / next, so the column height is constant across the entire song and the art card is rock-steady. Long lines that don't fit the overlay width get ellipsis-truncated (`…`) at the end. Trade-off: long-line songs (e.g. "Have You Ever Seen the Rain" — "Someone told me long ago there's a calm before the storm") will show the truncated version instead of the wrapped full text. If long-line readability matters more than visual steadiness, drag the overlay window wider in edit mode to fit more characters per line.

## [0.6.3] - 2026-05-14

### Fixed
- **Side-by-side album art is now exactly the same height as the lyrics column**, no longer slightly taller. The previous CSS-only approach (`align-self: stretch + aspect-ratio: 1`) ended up taking row height from the album art image's intrinsic dimensions during flex's hypothetical-size resolution pass, which produced a square that was a few px taller than the lyrics block next to it. The art now reads its size from a `ResizeObserver` + `useLayoutEffect` measurement of the lyrics column's bounding-rect height — exact-pixel match, updates live as font size / line padding / line wrap changes.

## [0.6.2] - 2026-05-14

### Changed
- **Album art now sits to the LEFT of the lyrics, sized to match the lyrics column height** in the **3-line scroll** and **Single-line karaoke** layouts. Was a 40×40 absolute-positioned thumbnail in the top-left corner that overlapped the start of left-aligned lyric lines and looked tacked-on. Now the art is a square card whose height equals the natural height of the lyrics block (computed at render time via flexbox `align-self: stretch` + `aspect-ratio: 1`), with the lyrics flowing in their own column to its right. Result: the leading edge of every line is the same horizontal position regardless of art presence, the art scales with the user's font size, and rounded corners + subtle shadow give it card-like prominence without competing with the text. **Full-page scroll** layout still uses the small corner-pinned 40×40 thumbnail (the side-by-side layout would fight the scrolling column).
- **Dev console window no longer pops up at launch.** The "Lyric Overlay (dev)" window with its CURRENT TRACK / LYRICS / EVENT LOG cards is now hidden by default and removed from the taskbar — most users never need to see it. To open it on demand, right-click the system tray icon and pick **Show / Hide dev console**. The same menu item closes it when you're done. Useful when something looks wrong and you want to confirm what `track-changed` events the app is receiving.

### Added
- **Tray menu item: Show / Hide dev console.** Sits between **Settings…** and **Quit Lyric Overlay** in the tray context menu. Toggles visibility of the main dev-console window. The window itself is the same one as before — just no longer auto-shown.

## [0.6.1] - 2026-05-14

### Changed
- **Default Text alignment is now Left, not Center.** The first character of every lyric line now lands in the same horizontal position regardless of line length, so your eye doesn't have to chase the leading edge as lines change. Center alignment looked nicer when idle but actively hurt readability while singing along — short lines started further right, long lines started further left, and the constant jitter forced you to re-find the start of every line. The Settings → Typography → **Text alignment** dropdown still offers Left / Center / Right, so anyone who preferred Center can switch back. Existing installs with a saved `text_align: "center"` in their settings.json keep that value (no surprise change on upgrade); new installs and Reset-to-defaults now produce Left.

## [0.6.0] - 2026-05-14

### Added
- **iTunes album art now appears in the overlay.** Previously the 40×40 rounded thumbnail at the top-left of the overlay only worked for SMTC sources (Spotify desktop, Chrome with YouTube, Edge, the new Apple Music app) — iTunes-COM-sourced tracks silently had no artwork. The PowerShell COM poller (`src-tauri/scripts/itunes_poll.ps1`) now reads the first entry of `track.Artwork`, saves it via `IITArtwork.SaveArtworkToFile()` to a temp file, base64-encodes the bytes, detects MIME from `IITArtwork.Format` (1=jpeg, 2=png, 3=bmp), embeds it in the JSON line as `art_data_url`, then deletes the temp file. The Rust side emits the same `album-art-loaded` event SMTC uses, so the frontend's existing badge component picks it up with no changes. Artwork is only re-extracted when the iTunes track key (`title|artist|album`) actually changes — once per track, not once per poll, so the stdin pipe doesn't carry hundreds of KB per second.
- **Per-word karaoke sweep on synced lines with word-level timing.** When the active line came from a source with rich (enhanced) LRC data — currently only SimpMusic's `richSyncLyrics` field — the current line now renders word-by-word with three visual states:
  - **Past words** (cursor has moved past them): full **Text color (current line)** from settings.
  - **Current word** (cursor is inside it): smoothly transitions from **Text color (prev / next, dim)** → **Text color (current line)** via a CSS `transition: color <Nms> linear` where N = the word's playback duration (next word's start - this word's start, floored at 80ms; for the last word, until the next line starts or +4000ms fallback). The result is a Spotify-style sweep across each word as the singer reaches it.
  - **Future words**: dim **Text color (prev / next, dim)**.
  - Lines without word-level data (LRCLib-only tracks, NetEase-only tracks) keep the existing line-granularity highlight unchanged. No new setting — the karaoke sweep just appears whenever the data is available. Active in all three layout modes (3-line scroll, single-line karaoke, full-page scroll).
- **Tint background from album art** toggle in **Settings → Background**. When on AND the current track has album art, samples the dominant color from the artwork's data URL via a 32×32 offscreen canvas (skips near-transparent and near-black border pixels so the average leans toward the real artwork color), blends it 50/50 in RGB with the user's **Background color**, and renders that as the overlay background. To make the toggle actually visible by default, the effective opacity is clamped to a minimum of 22% when the user's **Background opacity** slider is below that — the user's higher opacity values are still respected. Changes smoothly on track-change via the existing 160ms `background` transition. Default: off (existing users won't see surprise color changes after upgrading).

### Architecture / files
- **`src-tauri/scripts/itunes_poll.ps1`** — adds `Get-ArtworkDataUrl` helper, a `$lastTrackKey` track-change tracker so artwork extraction only fires on track changes (not every 1Hz poll), and a 10MB size cap matching SMTC's `MAX_THUMBNAIL_BYTES`.
- **`src-tauri/src/itunes.rs`** — `Line` struct gains `art_data_url: Option<String>`. When set, the Rust worker emits `album-art-loaded` with the same `AlbumArtPayload` shape SMTC uses (now `pub struct` in `smtc.rs`). Adds `[itunes] track-changed → ...` and `[itunes] album-art-loaded for '...'` log lines for visibility.
- **`src-tauri/src/smtc.rs`** — `AlbumArtPayload` is now `pub` so iTunes can construct one.
- **`src-tauri/src/settings.rs`** — `Settings` gains `tint_bg_from_album_art: bool` (default false). No new validator entry needed — bool is bool.
- **`src/types.ts`** — `Settings` type adds `tint_bg_from_album_art: boolean`.
- **`src/Overlay.tsx`** — new `currentWordIdx` state + `wordIdxRef`, rAF tick advances/rewinds the per-word cursor, resets on line/track change. New `karaoke` prop on `LineRow` swaps the single-text render for per-word `<span>` array when present. New `extractDominantColor` (32×32 canvas average), `mixHexWithRgb` (linear-interpolated hex+rgb in RGB space), updated `colorWithOpacity` (now accepts `rgb(...)` input alongside `#rrggbb`). Container background uses the tinted color when toggle is on AND tint extraction succeeded.
- **`src/Settings.tsx`** — adds **Tint background from album art** toggle + hint to the **Background** section.

### Fixed (incidental, surfaced during build)
- **iTunes worker now logs track-change events to stderr** (`[itunes] track-changed → title='X' artist='Y' state=Z`). Previously only album-art and stderr lines were logged. Makes it easier to spot whether the iTunes COM bridge is actually reaching the snapshot when something looks wrong.

### Notes / known limitations
- **Per-word sweep depends on SimpMusic data.** LRCLib (the primary source) returns line-level only. NetEase fallback also returns line-level only. So tracks that LRCLib has → per-line highlight. Tracks LRCLib lacks but SimpMusic has rich data for → per-word sweep. This means the visual experience is inconsistent across your library; that's a source-data limitation, not a render bug.
- **Tint extraction skips near-white pixels** (lum > 720) too, since pure-white pop-album backgrounds would otherwise dominate the average and produce a near-white tint that's invisible against the default white text. Heavy-white-art tracks may produce subtler tints than expected.
- **Tint requires album art to be successfully extracted.** No art → no tint, even with the toggle on. iTunes art now works (fixed this version); SMTC art works via the existing `MediaProperties.Thumbnail()` path; YouTube tabs via SMTC often have no thumbnail at all.

## [0.5.2] - 2026-05-14

### Fixed
- **iTunes tracks now show up in the dev console + flow into the lyrics pipeline.** The classic-iTunes COM bridge has been completely silent since v0.5.0 — the dev console **CURRENT TRACK** card stayed on `(no title) / (no artist) / unknown 0:00 / 0:00` and `EVENT LOG` showed `No events yet` even with iTunes actively playing music. Root cause: the v0.5.0 audit M2 "TOCTOU fix" switched the embedded PowerShell poll script from `fs::write` → `tempfile::NamedTempFile` but kept the file's writable handle open via `_tmp_guard = tmp` while spawning `powershell.exe -File <same path>`, which Windows rejects with a sharing violation (`The process cannot access the file ... because it is being used by another process`). PowerShell exited immediately on every spawn. Now `tmp.into_temp_path()` closes the writable handle BEFORE spawn, and the returned `TempPath` still auto-deletes the script on drop, so the random-suffix TOCTOU mitigation is preserved. Affects every user with classic iTunes for Windows — Spotify / Chrome / Edge / new Apple Music app users were unaffected because they go through SMTC, not the COM bridge.

### Changed
- **Dev-time diagnostic logging** now writes to stderr from both source bridges. The dev server's terminal (or wherever the binary's stderr is captured) prints:
  - `[smtc] worker starting` / `[smtc] manager acquired` / `[smtc] CurrentSessionChanged handler registered` at startup so it's visible the SMTC side initialized.
  - `[smtc] startup: session attached, source='<AUMID>', state=<X>` when a media app was already reporting media to SMTC at app launch (Spotify desktop, Chrome with audio, Edge, etc.).
  - `[smtc] startup attach_session failed (probably no active SMTC session)` when no app was reporting at launch — this is normal when no music is playing yet; the worker still listens for `Msg::SessionChanged`.
  - `[smtc] Msg::SessionChanged` / `Msg::MediaChanged` / `Msg::PlaybackChanged` when SMTC fires those events. `MediaChanged` includes the new title/artist/album/duration. `TimelineChanged` fires ~1Hz during playback and is intentionally NOT logged to keep noise down.
  - `[smtc] emit_full → title='X' artist='Y' state=Z pos=Nms dur=Mms` whenever the bootstrap or session-change path emits the full snapshot.
  - `[itunes] poller spawned (pid=NNN)` when the COM bridge starts the PowerShell child.
  - `[itunes:stderr] <line>` for every stderr line PowerShell produces — previously stderr was `Stdio::null()` so script crashes / ExecutionPolicy denials / COM errors were invisible. This is how the v0.5.0 sharing-violation bug was finally diagnosed.
- These logs are permanent (not gated behind `#[cfg(debug_assertions)]`) so production users reporting "no track shows up" can paste their stderr and you can see exactly where the chain breaks. The volume is low: a handful of lines at startup, then ~1 line per song change.

## [0.5.1] - 2026-05-14

### Fixed
- **Full-page scroll layout no longer silently falls back to 3-line scroll** when lyrics aren't in the `synced` state (fetching, not_found, instrumental, plain, idle, error). Previously the **Layout mode → Full-page scroll** dropdown choice in Settings only honored your selection while a synced LRC was loaded; the moment the track changed and lyrics were still being fetched, or when LRCLib had no result, the overlay reverted to the 3-line view without warning. Now the full-page container always renders for the selected layout — when no synced lines are available, the same status fallback the other layouts use (`♪ fetching — Track`, `♪ no lyrics for Track`, `♪ instrumental`, etc.) appears as a single centered line inside the scrollable container instead of the layout silently changing under you.
- **Album art badge now appears in all three layout modes**, not only **3-line scroll**. The 40×40 rounded thumbnail at the top-left of the overlay was previously only wired into the 3-line layout's render branch — switching to **Single-line karaoke** or **Full-page scroll** would hide the album art even with **Show album art** still toggled on. Now `AlbumArtBadge` is rendered in all three layout branches; since it's positioned `absolute` over the lyric area, it doesn't push lines around in any layout.

### Notes
- These were the two render-layer issues a code-audit subagent found while reviewing v0.5.0's manual-verify checklist items. Pure frontend changes — no Rust changes, no settings schema change, no new dependencies.

## [0.5.0] - 2026-05-14

### Added — Phase 5 (settings window)
- **Settings window** opens from the tray menu (the **Settings…** item, no longer disabled). Live-applies every change to the overlay so you can drag the slider and watch the result in real time. Persisted to `%APPDATA%\com.syvr.lyric-overlay\settings.json`. Window is 560×680 (resizable, minimums 480×560), centered, decorated, hidden until requested. Sections, top to bottom:
  - **Mode & startup**: dropdown for the **Last mode (restored on launch)** value (Edit / Locked / Ghost). The hotkey hint at the bottom of the section reads "Hotkey to cycle modes: Ctrl+Alt+L (system-wide)".
  - **Lyrics timing**: **Anticipation** slider (0–1500ms, step 25ms, default 500ms) — how far ahead the cursor looks up the active line; karaoke convention. **Seek-jitter tolerance** slider (500–5000ms, step 100ms, default 2000ms) — backward jumps under this threshold are treated as source-counter staleness, not real seeks.
  - **Typography**: text input for **Font family** (default `Inter`), sliders for **Current line size** (14–48px, default 26), **Current line weight** (300–900, default 600), color picker + hex text box for **Text color (current line)** (default `#ffffff`), text input for **Text color (prev / next, dim)** that accepts hex or `rgba()` (default `rgba(255,255,255,0.45)`), dropdown for **Text alignment** (Left / Center / Right).
  - **Background**: color picker + hex text box for **Background color**, slider for **Background opacity** 0–100% (default 0% = fully transparent — useful for rendering over dark games / videos).
  - **Layout**: dropdown for **Layout mode** with three choices — **3-line scroll** (prev / current / next, the original behavior), **Single-line karaoke** (only the current line, larger), **Full-page scroll** (all lines visible, current line auto-scrolled into view). Slider for **Line padding** (0–24px, default 6).
  - **Extras**: toggle **Show album art (when available)** and toggle **Show translated lyrics (when available)** — both default on / off respectively.
  - Footer shows the storage path and a **Reset to defaults** button (with a confirm prompt).
- **Live preview** — every slider/picker emits a debounced `update_settings` IPC (200ms coalesce) which fires `settings-changed`; the overlay listens and reapplies fonts, colors, opacity, layout, anticipation, jitter tolerance, and translation visibility immediately, no restart.
- **Mode persistence** — the overlay now restores your last-used mode at cold start instead of always defaulting to Edit. Tray icon, tooltip, menu checkmarks, and the click-through window flag are all driven by the loaded `last_mode` before first paint.
- **Tray menu's "Settings…" item enabled.** Previously disabled placeholder; now opens / focuses / unminimizes the settings window.

### Added — Phase 6 (lyric source fallbacks + album art + translations)
- **SimpMusic + NetEase fallback after LRCLib NotFound.** When LRCLib has no result for the current track, the lyrics worker now falls through to SimpMusic (`https://api-lyrics.simpmusic.org/v1/search/title`), then NetEase (`music.163.com/api/search/get` → `/api/song/lyric`). Each source is filtered client-side by artist name and duration (±5s SimpMusic, ±5s NetEase) so a different song with the same title can't sneak in. The overlay's source attribution (`lyrics.source` field) now reads `lrclib` / `lrclib-search` / `simpmusic` / `netease` / `all-sources` so the dev console shows where a hit came from. Cached results carry the source through restarts.
- **Word-level (enhanced) LRC support.** SimpMusic's `richSyncLyrics` field uses `<mm:ss.xx>word` per-word timestamps; the new `parse_enhanced_lrc` parses these into `LyricLine.words: WordSpan[]` (3 unit tests cover basic, line-prefix, and empty-line cases). The frontend type now exposes `words?: WordSpan[]` on every `LyricLine`. **No UI yet** — the rendered overlay still highlights at line granularity. Phase 7 (unwritten) would add per-word color sweep using these timestamps.
- **Translated lyrics (NetEase only).** When the NetEase fallback succeeds AND the song has a `tlyric` field (typically Chinese), the lyrics state now carries an aligned `translation: LyricLine[]` array. With the **Show translated lyrics** setting on, the translation text appears as a small italic dim line under the current lyric in the 3-line and single-line layouts (replaces the "next" line slot in 3-line mode when translation is present, since they'd compete for the same vertical space).
- **Album art badge** in the overlay corner. Extracted from SMTC's `MediaProperties.Thumbnail()` stream as raw bytes, converted to a base64 `data:` URL, sent to the frontend via a new dedicated `album-art-loaded` event (kept separate from the per-tick `track-changed` payload to avoid 60 × 200KB IPC bloats per minute). Renders as a 40×40 rounded thumbnail at top-left with subtle shadow when **Show album art** is on AND the loaded art matches the currently playing track. Falls back silently when SMTC has no thumbnail (most YouTube tabs, some streamers). MIME auto-detected from bytes (PNG / GIF / WebP / JPEG default).

### Fixed (security audit findings — pre-launch sweep)
- **Settings input validation** (audit H1, H2). The `update_settings` Tauri command now sanitizes every patch field after merging: hex colors must match `^#[0-9a-fA-F]{6}$` or fall back to defaults; `text_color_dim` accepts hex or `rgb()/rgba()` only (no CSS expressions like `url(...)`); `text_align` and `layout_mode` are validated against an allowlist; `font_family` is filtered to ASCII alphanumerics + safe punctuation, capped at 80 chars; numeric fields are clamped to sensible ranges (anticipate −2000…5000ms, font 8–96px, etc.). The same `sanitize()` runs on `load_from_store` so a hand-edited `settings.json` can't bypass.
- **NetEase URL built unsafely** (audit M1). Switched `lyrics.rs::fetch_netease`'s lyric-by-id endpoint from `format!()` string concat to `reqwest::Url::parse_with_params`, matching the LRCLib + SimpMusic paths. No exploit was possible (the song id is a `u64`), but the pattern is now consistent and defense-in-depth.
- **PowerShell child script TOCTOU + orphan-on-shutdown** (audit M2 + L2). `itunes.rs` now stages the iTunes COM poll script via `tempfile::NamedTempFile` (random suffix, auto-cleanup) instead of a fixed `%TEMP%\lyric-overlay-itunes-poll.ps1`. The child process is spawned with `kill_on_drop(true)` so any clean exit / panic / future cancellation kills the PowerShell child + removes the temp script. (Externally-killed-parent case is still in BUGS.md, needs a Windows JobObject for full cleanup.)
- **SMTC manager-level event token leak** (audit M3). The `CurrentSessionChanged` registration is now wrapped in a `ManagerHook` struct with a `Drop` impl that calls `RemoveCurrentSessionChanged`. Mirrors the existing `SessionHooks` pattern for per-session tokens. Prevents dangling COM callbacks if the worker future is ever cancelled.
- **Album art memory cap** (audit M4). `read_thumbnail_bytes` now rejects thumbnails reporting > 10MB before allocating, so a misbehaving (or hostile) media source can't balloon the buffer. Real album art is well under 1MB.
- **CSP `img-src` tightened** (audit M5). Removed `https:` from the `img-src` allowlist in `tauri.conf.json` — album art is delivered as `data:` URLs from Rust, no external image origins are needed by the renderer. Now `img-src 'self' asset: data:` only.

### Architecture / files
- **`src-tauri/src/settings.rs` (new)** — `Settings` struct (Serde, with defaults for every field so older store entries auto-fill), `SharedSettings = Arc<RwLock<Settings>>`, `load_from_store` + `save_to_store`, `sanitize()` validation, `persist_last_mode()` helper called by `mode.rs`, and four Tauri commands (`get_settings`, `update_settings(patch: Value)`, `reset_settings`, `open_settings_window`).
- **`src-tauri/src/lyrics.rs` (large refactor)** — `LyricLine` gains an optional `words: Option<Vec<WordSpan>>`, `CachedLyrics::Synced` gains an optional `translation: Option<Vec<LyricLine>>`, both `#[serde(default, skip_serializing_if)]` so existing cache files stay readable. New `fetch_simpmusic` + `fetch_netease` functions (each with its own `pick_best_*` filter), new `parse_enhanced_lrc` for SimpMusic's rich format, `resolve_lyrics` rewritten to chain LRCLib → SimpMusic → NetEase. `reqwest::Client` now built with `cookie_store(true)` for NetEase's NMTID handshake.
- **`src-tauri/src/smtc.rs`** — adds `spawn_art_fetch` background task that reads SMTC's thumbnail stream off-thread and emits the new `album-art-loaded` event with `{ title, artist, data_url }`. New `ManagerHook` Drop guard.
- **`src-tauri/src/mode.rs`** — `apply_mode` now calls `crate::settings::persist_last_mode` so toggling mode (tray, hotkey, command) updates the saved value automatically.
- **`src-tauri/src/lib.rs`** — loads settings before building the tray (so initial check items + tooltip + icon reflect persisted `last_mode`), manages `SharedSettings`, registers the new commands, opens the settings window from the tray menu.
- **`src-tauri/tauri.conf.json`** — adds the third window `settings` (decorated, 560×680, centered, `visible: false` so it only appears when summoned).
- **`src-tauri/capabilities/default.json`** — adds the `settings` window to the windows allowlist plus `core:window:allow-set-focus` and `core:window:allow-unminimize` for the tray-open path.
- **`src/Settings.tsx` (new)** — the React settings UI. Inline-styled, dark theme, gold (`#d4af37`) accent on sliders + hex value labels (matches SYVR brand). Shared primitives: `Section`, `Row`, `Slider`, `Select`, `Toggle`, `ColorInput`. Debounced 200ms write + live emit.
- **`src/Overlay.tsx`** — listens to `settings-changed`, `mode-changed`, `album-art-loaded`. All previously-hardcoded constants (font, colors, anticipate, jitter, padding, alignment) now read from settings state; rAF closure reads via `settingsRef` to stay stable. Three layout-mode renderers (three-line / single-line / full-page-scroll-to-cur). New `AlbumArtBadge` + `TranslationRow` components.
- **`src/main.tsx`** — third route: window label `settings` → `<SettingsView />`.
- **`src/types.ts`** — exports `OverlayMode`, `LayoutMode`, `TextAlign`, `WordSpan`, `Settings`. `LyricLine.words` and `CurrentLyrics.translation` added.

### Dependencies added
- `tauri-plugin-global-shortcut = "2"` was Phase 4. Phase 5+6 added: `base64 = "0.22"` (album art encoding), `tempfile = "3"` (audit M2 fix), and `cookies` feature on `reqwest`.

### Tray icons regenerated
The Phase 4 ♪-glyph icons washed out at the actual tray render size (~16px). Replaced with three visually distinct shapes that read at any size:
- **Edit** — bright yellow filled circle (`rgb(255,200,0)`) with a dark diagonal slash (pencil-tip mark).
- **Locked** — white rounded square (`rgb(245,245,245)`) with a dark padlock keyhole.
- **Ghost** — gray dashed circle outline only (no fill), 1.5px stroke.

Same `include_bytes!` + `Image::from_bytes` plumbing as before. `tray.set_icon` swaps on every `apply_mode` call.

### Notes / known limitations
- Word-level data is parsed and exposed in `LyricLine.words` but the overlay still renders at line granularity. Per-word color sweep would need a future Phase 7.
- Translated lyrics field is currently NetEase-only (LRCLib has no translation field; SimpMusic doesn't either). Most English-language tracks won't surface a translation.
- Mode persistence is per-machine (lives in `%APPDATA%\com.syvr.lyric-overlay\settings.json`). No cross-device sync.
- The `Full-page scroll` layout assumes the overlay window is taller than ~3 lines tall. Resize the window via the edit-mode drag corners to suit.
- Cache key `artist|title|duration_secs` collides if `|` appears in artist or title. Logged in `BUGS.md` as L1; fix is to switch delimiter or hash.

## [0.4.0] - 2026-05-14

### Added
- **Phase 4 — overlay modes (edit / locked / ghost), system tray, and global hotkey.** The overlay window can now be put into one of three modes that change how it interacts with the cursor:
  - **Edit mode** (default at startup): the overlay is movable. Hovering the window draws a thin gold dashed border (color `rgba(212, 175, 55, 0.85)`, 1px, rounded 8px corners, 160ms fade in/out) so it's obvious the window is "live" and grabbable. Cursor over the window is `move`. Click and drag anywhere on the lyrics to reposition.
  - **Locked mode**: the overlay no longer accepts drags (the `data-tauri-drag-region` attribute is removed from every drag-target element while locked). Cursor over the window reverts to `default`. No hover border. The window still receives clicks (so right-click could open a future context menu), it just can't be moved.
  - **Ghost mode**: the overlay becomes fully click-through via `WebviewWindow::set_ignore_cursor_events(true)`. Mouse clicks, scrolls, and hovers pass straight through to whatever window sits behind the overlay (your browser, IDE, game, etc.). The lyrics keep rendering and updating; you just can't interact with the window directly anymore. Hover border never shows because the frontend never receives a `mouseenter`.
- **System tray icon** added (Windows notification area, near the clock). Single left- or right-click opens the menu. The icon itself indicates the current mode at a glance:
  - Edit → solid gold-filled circle with a dark eighth-note glyph (♪)
  - Locked → solid dark-gray circle with a bright white eighth-note glyph
  - Ghost → no fill, dashed gray ring outline with a dim gray eighth-note glyph
  Tooltip on hover reads `Lyric Overlay — <mode> mode`.
- **Tray menu** items, top to bottom:
  - **Show / Hide overlay** — toggles the overlay window's visibility (uses Tauri's `Window::show()` / `Window::hide()`). Useful when you want to temporarily hide lyrics without quitting.
  - **Mode ▸** submenu with three checkable items: **Edit**, **Locked**, **Ghost (click-through)**. Exactly one is checked at any time, reflecting the current mode. Clicking switches modes immediately.
  - **Settings… (coming soon)** — placeholder, disabled. Will open the Phase 5 settings window.
  - **Quit Lyric Overlay** — exits the app.
- **Global hotkey `Ctrl+Alt+L`** cycles modes in order: edit → locked → ghost → edit. Works system-wide regardless of which app has focus. Registered via `tauri-plugin-global-shortcut` (added at `2.3.1`).
- **New Tauri commands** exposed for frontend use: `get_overlay_mode`, `set_overlay_mode(mode)`, `cycle_overlay_mode`, and `toggle_overlay_visibility`. The overlay UI uses `get_overlay_mode` on mount to seed its initial state and listens for the new `mode-changed` event payload (`"edit" | "locked" | "ghost"`) to react to mode changes from any source (tray click, hotkey, command).

### Changed
- **Overlay border behavior** — the overlay now reserves a 1px border slot at all times (transparent in locked/ghost, gold-dashed when hovered in edit). This keeps line layout from jumping by 2px between modes. The border has `border-radius: 8px` and a 160ms ease transition on `border-color`.
- **`data-tauri-drag-region` is now mode-gated.** Previously the entire overlay body and every line row was always drag-active; now those attributes are conditionally rendered only when `mode === "edit"`. The container's cursor is also mode-driven (`move` in edit, `default` otherwise).

### Architecture / files
- **`src-tauri/src/mode.rs` (new)** — single source of truth for mode state. `OverlayMode` enum (`Edit | Locked | Ghost`, repr `u8`, serialized as lowercase string), `SharedMode = Arc<AtomicU8>` for lock-free reads from any thread (sync hotkey handlers + async commands both share the same atomic), and `apply_mode(app, mode)` which: writes the atomic, calls `set_ignore_cursor_events` on the overlay window, swaps the tray icon + tooltip, syncs the three submenu checkmarks (held via managed `ModeMenuItems` state), and emits `mode-changed`.
- **`src-tauri/src/lib.rs`** — wires the tray (`build_tray`), the global-shortcut plugin (`build_global_shortcut_plugin` + `register_hotkey`), the new commands, and calls `apply_mode(default)` at startup so the tray icon, tooltip, menu checkmarks, and window flag all line up with the stored state on first paint.
- **`src-tauri/icons/tray-edit.png`, `tray-locked.png`, `tray-ghost.png` (new)** — three 32×32 PNGs with transparency, generated via `System.Drawing` PowerShell. Loaded into the binary at compile time via `include_bytes!` and decoded with `tauri::image::Image::from_bytes` so they're valid in installed builds, not just dev.
- **`src/Overlay.tsx`** — adds `mode` + `hovered` local state, listens to `mode-changed`, conditionally applies `data-tauri-drag-region` and the gold dashed hover border, and passes a `dragRegion` prop down to each `LineRow`.
- **`src/types.ts`** — new exported `OverlayMode = "edit" | "locked" | "ghost"` type matching the Rust enum's serialized form.

### Capabilities added (`src-tauri/capabilities/default.json`)
`core:window:allow-show`, `core:window:allow-hide`, `core:window:allow-set-ignore-cursor-events`, `core:tray:default`, `global-shortcut:allow-register`, `global-shortcut:allow-unregister`, `global-shortcut:allow-is-registered`. (Tauri 2's `core:default` does not include any of these by default.)

### Dependencies
- **`tauri-plugin-global-shortcut = "2"`** added (resolves to 2.3.1) — Phase 4 hotkey requirement.
- **`tauri = "2"` features** extended with `tray-icon` (system tray support) and `image-png` (PNG-decoded tray icon at runtime).

### Notes / known limitations
- Only the cycle direction is supported by the hotkey. Direct hotkey-to-mode (e.g. Ctrl+Alt+G to jump straight to ghost) deferred to Phase 5 settings.
- `Settings…` menu item is intentionally disabled until Phase 5 ships the settings window.
- Mode preference is **not yet persisted** — every cold start begins in edit mode. Phase 5 will save the last-used mode in `tauri-plugin-store` and restore it.
- The tray icon's three visual treatments are intentionally restrained (gold for SYVR brand consistency in edit, neutral grays for the other two) — no rainbow tinting.

## [0.3.2] - 2026-05-14

### Fixed
- **Lyrics no longer briefly jump to the wrong line and snap back.** Previously, when a `timeline-changed` event arrived with a position slightly behind our smoothly-interpolated estimate (iTunes COM and SMTC both report `PlayerPosition` on their own cadence, which can lag the actual audio playback head by 100-800ms), the overlay cursor would rewind to a previous line for one frame and then re-advance on the next tick. Added a **monotonic clamp** in `applyTrack`: timeline events during stable same-track playback that report a position behind the interpolation by less than 2 seconds are now treated as source-counter staleness, and we keep advancing from our existing forward-moving anchor. Real seeks (forward OR backward, magnitude ≥ 2s) and any track or state change still pass through normally. Implemented by passing `kind: "track" | "timeline" | "state"` to `applyTrack` so the clamp applies only to `timeline` events during `playing → playing` transitions.

### Changed
- **`LYRIC_ANTICIPATE_MS` bumped 300 → 500.** Anticipation feels better at 500ms — lines now change clearly *just before* the singer rather than landing right on the syllable. Phase 5 will expose this as a slider so users can tune to their preference.

## [0.3.1] - 2026-05-14

### Fixed
- **Long lyric lines are no longer truncated** in the overlay. The current line now wraps to up to 2 lines (`-webkit-line-clamp: 2`) so songs like "Have You Ever Seen the Rain" can show the full opening line "Someone told me long ago there's a calm before the storm" instead of cutting off at "Someone told me long ago there's a calm befo…". Previous and next lines remain single-line with ellipsis (they're secondary context). Current line font shrunk slightly from 28px to 26px to keep the 2-line case comfortable within the 200px-tall window.
- **Lyric-line transitions now happen ~300ms earlier** to compensate for the lag between iTunes' COM `PlayerPosition` (and SMTC's reported position) and the actual audio playback head. Most karaoke apps "anticipate" lookup by 300-500ms so the line appears just before the singer sings it — which is also what listeners' eyes expect. New constant `LYRIC_ANTICIPATE_MS = 300` in `src/Overlay.tsx`, applied to both the rAF cursor advance and the initial binary-search snap on lyrics-load. Phase 5 will expose this as a user-configurable setting.

## [0.3.0] - 2026-05-14

### Added
- **Phase 3 — overlay window**: a new always-on-top, borderless, transparent window labeled `overlay`, positioned by default at (200, 60) at 720×200. Skips the taskbar. Drag-region is the entire window body so you can reposition by clicking and dragging anywhere on the lyrics. Coexists with the dev console (`main` window) so you can watch both during development.
- **3-line scrolling lyrics view** (`src/Overlay.tsx`): renders the previous line in dim white (45% opacity), the current line bright white at 28px / 600 weight, and the next line dim. Cross-fade transition on line changes (220ms opacity + color). Text shadow (heavy black drop + soft halo) keeps lines readable over any desktop background.
- **rAF-driven position interpolation**: an animation-frame loop computes `interpolated_position = last_known_position + (now - last_update_unix_ms)` while playing, freezes at `last_known_position` when paused/stopped. The cursor advances forward one increment per frame in the normal case (O(1)) and rewinds when SMTC reports a seek backward. Initial cursor on lyrics load uses binary search to jump to the right position immediately. React state updates are throttled — `setDisplayIdx` fires only when the line index actually changes, not on every rAF tick (~once per second of playback, not 60×/second).
- **Hard recalibration** on `track-changed` (clear cursor, wait for new lyrics), `timeline-changed` (snap `last_known_position` + `last_update_unix_ms`), `playback-state-changed` (freeze/resume interpolation). Seeking forward or backward in the player triggers `timeline-changed` which the cursor's rewind/advance logic catches naturally.
- **`src/main.tsx` branches on window label**: `overlay` window renders `<Overlay />`, `main` window renders `<DevConsole />`. Same Tauri events flow to both.
- **`src/types.ts`** — shared `CurrentTrack`, `LyricLine`, `LyricsStatus`, `CurrentLyrics` types + `fmtMs` helper. DevConsole and Overlay both import from here instead of redefining.
- **Status fallback in the overlay's middle line** when no current lyric: shows `♪ fetching — Track Name` during lookup, `♪ no lyrics for Track Name` on not_found, `♪ instrumental`, `♪ unsynced lyrics (no per-line timing)` for plain, `♪ Track Name` when idle. So the overlay always shows something meaningful even before lyrics arrive.

### Changed
- **`src/index.css`** — `html` and `body` are now `background: transparent` and `margin: 0`. Required for the overlay window's OS-level transparency to work. The DevConsole's outer container paints its own dark background, so this doesn't visually change the dev console.
- **`core:window:allow-start-dragging` capability added** — Tauri 2's `core:default` set doesn't include the `start_dragging` IPC call, so `data-tauri-drag-region` was silently a no-op until this was granted. The overlay drag now works.
- **App.tsx renamed → DevConsole.tsx**, types moved out into `types.ts`. Functionally identical.

### Notes
- **No way to close the overlay window yet** since `decorations: false` removes the X button and `skipTaskbar: true` hides it from Alt+Tab. Phase 4 will add a tray menu + Ctrl+Alt+L mode hotkey (edit/locked/ghost) for proper control. For now, killing the Tauri dev server (or the binary) closes both windows together.
- **Drag region is the whole window**, so the entire overlay surface accepts click-to-drag. Phase 4 will gate this behind the "edit" mode; locked/ghost modes will turn it off.

## [0.2.1] - 2026-05-14

### Changed
- **LRCLib lookups are now parallel.** Previously, `/api/get` ran first and only fell back to `/api/search` after it 4xx'd — worst-case wall-clock was up to ~20s on misses because each endpoint takes ~8-10s from this network. The new path issues both requests at once via `tokio::join` and merges results: `/api/get` wins when it has content (canonical metadata match), otherwise `/api/search` is consulted. Worst case is now ~10s. The LYRICS section in the dev console will flip from `fetching` to `synced`/`not_found` roughly twice as fast on songs LRCLib doesn't have on `/api/get`.

### Fixed
- **Search results now duration-filtered to ±5 seconds of the requested track**, in addition to the existing title-substring filter. Covers and remixes of the same name often have very different lengths — without this filter, a search for Duka's "Toxic" (203s) could return Ashnikko's "Toxic" (163s) and we'd display the wrong lyrics. Now those mismatched candidates are dropped before scoring. Genuine unknowns (Duka has no cover indexed anywhere) correctly stay `not_found`.

### Notes
- Considered adding SimpMusic / NetEase as additional sources after LRCLib returns nothing — deferred to Phase 6 per the spec. Considered AI models for fuzzy title resolution or generation — rejected: hallucinated lyrics break trust, Whisper transcription requires audio capture + heavy compute, and the regex title cleaner handles 95% of fuzzy-match cases already.

## [0.2.0] - 2026-05-14

### Added
- **Lyrics fetch + LRC parsing pipeline**: on every `track-changed` event, the new lyrics worker (`src-tauri/src/lyrics.rs`) cleans the title, looks up an in-memory cache, then a persistent cache (tauri-plugin-store), then hits LRCLib. Hits are parsed into `Vec<{ time_ms, text }>` and emitted as Tauri events. The dev console shows the pipeline live.
- **Title cleaner**: strips bracketed/parenthesized chunks containing `Official Video`, `Music Video`, `Lyric Video`, `Lyrics`, `Audio`, `Visualizer`, `feat./ft./featuring`, `Remastered (YYYY)`, `Re-recorded`, `Live (at/from/in ...)`, `Acoustic`, `Unplugged`, `Demo`, `Single/Album version`, `Radio Edit/Version/Mix`, `Extended/Original Mix`, `Bonus Track`, `4K/8K`, `HD/UHD/MV` — case-insensitive, single regex. So "Apocalypse (Official Video)" → "Apocalypse" before LRCLib lookup.
- **LRCLib client**: `GET /api/get?artist_name&track_name&album_name&duration` first; on any 4xx, falls back to `GET /api/search?track_name&artist_name` and picks the best match (prefers records with synced lyrics, title-substring filter). Custom User-Agent identifying the app + project URL per LRCLib's etiquette docs. Client timeout is 30s — LRCLib responses can take 8-10s on the wire from this network.
- **LRC parser**: handles `[mm:ss]`, `[mm:ss.xx]` (centiseconds), `[mm:ss.xxx]` (milliseconds), and multi-timestamp lines like `[00:01.00][01:01.00]Same line`. Metadata tags (`[ti:]`, `[ar:]`, `[al:]`, `[length:]`) are skipped because they don't start with a digit. Output sorted by `time_ms`. Five unit tests cover the cases.
- **Two-tier cache**: in-memory `HashMap<String, CachedLyrics>` + persistent JSON store (`tauri-plugin-store`, file `lyrics-cache.json`). Cache key is `artist|title|duration_secs` (lowercased). NotFound is also cached so we don't keep hammering LRCLib for known-missing tracks. Network/5xx errors are NOT cached — they retry on next track-change.
- **New Tauri events**: `lyrics-state` (fetching), `lyrics-loaded` (synced/plain/instrumental), `lyrics-not-found`. All carry the same `CurrentLyrics` payload (`{ track_key, status, source, line_count, lines, plain, track }`). `status` is one of `idle | fetching | synced | plain | instrumental | not_found | error`. `source` is `memory | store | lrclib | lrclib-search | error`.
- **New Tauri command**: `get_current_lyrics()` returns the current `CurrentLyrics` snapshot for first-paint hydration.
- **Dev console: LYRICS section** (`src/App.tsx`): new card between CURRENT TRACK and EVENT LOG. Shows status + source + line count in the header (color-coded: green `synced`, lime `plain`, gray `instrumental`/`not_found`, amber `fetching`, red `error`). Body renders the first 10 timestamped lines for synced lyrics, first 10 lines of plain text for plain, "♪ instrumental" for instrumental, or a "no lyrics found" / "fetching for X…" / "error" message accordingly. Event log gains three more event types (`lyrics-state` amber, `lyrics-loaded` green, `lyrics-not-found` gray).
- **Worker filters out empty-artist tracks**: YouTube non-music videos (DoodStream/Telegram dumps with blank artist metadata) used to spam LRCLib with 4xx responses. Now silently skipped.

### Changed
- **Source priority semantics renamed**: `smtc_active` → `smtc_playing` to reflect the corrected logic (only suppress iTunes when SMTC is genuinely playing, not just attached). No behavior change beyond Phase 1's already-shipped fix.

### Notes
- Verified end-to-end with iTunes playing James Blunt — You're Beautiful: lyrics fetched from LRCLib (39 synced lines), parsed correctly including Chinese composer-credit lines, rendered in the dev console.
- LRCLib has no rate limit listed but their docs request a descriptive User-Agent. We send `lyric-overlay/0.2.0 (Windows desktop overlay; ...)`.
- Persistent cache file lives in the OS-standard app config dir (Windows: `%APPDATA%\com.syvr.lyric-overlay\lyrics-cache.json`).

## [0.1.0] - 2026-05-14

### Added
- **Initial scaffold**: Tauri 2 + React 19 + Vite + TypeScript baseline, copied from `template-tauri-desktop` and renamed (`com.syvr.lyric-overlay`). Borrowed Wren's icon set as a placeholder — replace with a Lyric Overlay icon before any release.
- **Phase 1 — Windows SMTC reader**: Rust module (`src-tauri/src/smtc.rs`) wraps `GlobalSystemMediaTransportControlsSessionManager`. On startup, requests the manager and subscribes to `CurrentSessionChanged`. Whenever a session is active, it also subscribes to that session's `MediaPropertiesChanged` / `TimelinePropertiesChanged` / `PlaybackInfoChanged`. COM event handlers fire on COM threads and forward through a tokio mpsc channel into a worker task that reads fresh state and emits Tauri events.
- **Tauri events emitted**: `track-changed`, `timeline-changed`, `playback-state-changed`. All three carry the same flat `CurrentTrack` payload (title, artist, album, duration_ms, position_ms, last_update_unix_ms, state, source_app_id) — the consumer reads whichever fields it cares about.
- **Tauri command**: `get_current_track()` returns the current snapshot synchronously for first-paint hydration before the first event arrives.
- **Phase 1 dev console UI** (`src/App.tsx`): replaces the template's greet shell with a dark monospace panel showing (1) the currently playing track — title, artist, album, position/duration, state with color-coded dot (green=playing, amber=paused, gray=stopped), source app ID — and (2) a scrolling event log capped at 80 entries showing each event's color-coded type, payload summary, and local timestamp. All events also stream to the browser dev console via `console.log`.
- **PlaybackState enum** (Rust): typed mapping of `GlobalSystemMediaTransportControlsSessionPlaybackStatus` to lowercase serde strings (`unknown`, `closed`, `opened`, `changing`, `stopped`, `playing`, `paused`).
- **Empty-session handling**: when SMTC has no active session (e.g. all music apps closed), the snapshot is reset to defaults and `track-changed` + `playback-state-changed` fire so the UI clears gracefully.
- **Source app ID surfacing**: `SourceAppUserModelId` is captured in the snapshot for debugging and future per-source quirks (e.g. Spotify reports duration differently than Chrome).

- **iTunes COM bridge** (`src-tauri/src/itunes.rs` + `src-tauri/scripts/itunes_poll.ps1`): classic iTunes for Windows doesn't expose itself to SMTC, so we bridge it via its COM automation interface. A small PowerShell script — embedded in the binary via `include_str!` and staged to `%TEMP%\lyric-overlay-itunes-poll.ps1` at startup — connects via `New-Object -ComObject iTunes.Application` and writes one JSON line per second to stdout. Rust reads stdout and emits the same `track-changed` / `timeline-changed` / `playback-state-changed` events SMTC does. The PowerShell window is hidden via `CREATE_NO_WINDOW`. The script never *launches* iTunes — it only attaches when iTunes is already running. If iTunes closes, the COM ref drops and reconnects on the next poll.
- **iTunes paused-state detection**: iTunes COM's `PlayerState` enum has no separate "paused" value (returns 0 for both stopped and paused). The script derives state by combining `PlayerState` with `PlayerPosition`: state 0 + position > 50ms = paused; state 0 + position 0 = stopped; state 1-3 = playing. The dev console now correctly shows iTunes as paused/playing rather than always "stopped."
- **Source priority**: when SMTC and iTunes both have data, SMTC wins — but only when SMTC is *currently playing*, not just when it has a session attached. Chrome notoriously holds onto SMTC sessions in Paused/Closed states long after a tab closed; treating those as active would mute iTunes. SMTC's playing-status is published via an `Arc<AtomicBool>` that the iTunes worker checks per line.
- **iTunes events labeled in the dev console**: track rows now show `· iTunes (COM)` as the source app ID when the iTunes bridge is the active emitter. SMTC sources show their AUMID (e.g. `Spotify.exe`, `Chrome`).

### Notes
- Phases 2-6 (lyrics fetch, overlay render, modes/tray, settings, polish) are not yet started.
- The `windows` crate 0.58 doesn't auto-implement `IntoFuture` for `IAsyncOperation`, so `RequestAsync` and `TryGetMediaPropertiesAsync` are wrapped in `tokio::task::spawn_blocking` + `.get()`. Both calls resolve in milliseconds and fire infrequently, so the blocking is benign.

### Known limitations
- **PowerShell child orphans on hot-reload**: during `pnpm tauri dev`, every Rust rebuild kills and respawns the parent process. The PowerShell COM poller doesn't get killed with the parent and lingers until manual cleanup. Logged in `BUGS.md`. Doesn't affect installed users (one app run = one child = one shutdown).
- **New Apple Music app (Microsoft Store)** has no COM API — those users go through SMTC, which is supported.
