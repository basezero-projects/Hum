# Changelog — Lyric Overlay

All notable changes to this project. Updated on **every commit**, not at the end of a task.

Versions follow `X.Y.Z` (bump all of `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` per commit).

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
