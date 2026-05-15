# Changelog — Lyric Overlay

All notable changes to this project. Updated on **every commit**, not at the end of a task.

Versions follow `X.Y.Z` (bump all of `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` per commit).

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
