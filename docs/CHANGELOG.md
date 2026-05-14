# Changelog — Lyric Overlay

All notable changes to this project. Updated on **every commit**, not at the end of a task.

Versions follow `X.Y.Z` (bump all of `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` per commit).

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
