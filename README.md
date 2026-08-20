# Hum

Hum is a Windows lyric overlay for listeners and streamers. It follows the active media session, resolves synchronized lyrics, and keeps the current line above the apps the listener already uses. The desktop overlay has compact ribbon and square presentations. A local server provides the same playback state to an OBS browser source.

Current development version: v0.13.77

Hum is still working toward its paid 1.0 release. The phase order and release gates live in the [1.0 roadmap](docs/ROADMAP.md).

## Current capabilities

- NetEase YRC word timing when title, artist, and duration match the recording
- LRCLib and NetEase line-timing fallbacks
- Plain lyrics and distinct instrumental, unavailable, error, ad, and unsupported states
- Three-line, single-line, and full-page ribbon layouts
- A square focused-lyrics layout
- Edit, Locked, and Ghost interaction modes
- Wired, Speakers, and Bluetooth delay profiles
- Temporary per-track timing nudges
- Album artwork, artwork-derived surfaces, Windows backdrops, and automatic contrast
- Optional translated lyrics when provider data includes them
- Artist biographies, photos, and upcoming Ticketmaster dates
- A loopback-only OBS browser source with saved timing parity
- Tray controls, global shortcuts, autostart, persistent settings, and Windows updates

## Playback flow

Hum uses the active Windows System Media Transport Controls session when a player publishes one. Source-specific adapters fill gaps when Windows metadata or timing is incomplete.

| Source | Current path | Notes |
|---|---|---|
| Spotify desktop | Windows SMTC | Native metadata, artwork, playback state, and timeline |
| Spotify web | Windows SMTC | Uses the browser's media session |
| YouTube Music and music videos | SMTC plus browser title cleanup | Background tabs can limit bridge enrichment |
| iTunes desktop | PowerShell and iTunes COM bridge | Publishes only when SMTC is not playing |
| Pandora web | Chromium UI Automation bridge | Adds track metadata, timing, and ad state |
| Pandora desktop | UI Automation plus playback estimation | Seeking and joining mid-track are less reliable than SMTC |
| Other Windows players | Windows SMTC | Quality depends on what the player publishes |

A playing SMTC session with a real title has authority over bridge estimates. When SMTC is inactive or incomplete, a fresh browser or Pandora bridge can provide the effective track. The shared media policy, payloads, and event ordering are covered by Rust tests.

## Lyrics and timing

The lyric resolver checks a bounded memory cache and `lyrics-cache.json` before it contacts providers. LRCLib and NetEase requests run together. Valid NetEase YRC word timing wins only after strict metadata and duration matching. LRCLib supplies the normal synchronized or plain result, with NetEase line timing as a later fallback.

Saved timing uses this equation:

```text
saved_offset_ms = anticipate_ms - selected_profile_delay_ms
```

Wired defaults to 0 ms, Speakers to 250 ms, and Bluetooth to 350 ms. `Ctrl+Alt+[` and `Ctrl+Alt+]` apply a session-only 250 ms nudge to the current track. That temporary nudge resets on track change and currently affects only the desktop overlay.

The full timing and source-authority rules are in [Media and timing](docs/systems/media-and-timing.md).

## Audio-output discovery

Windows discovers active render endpoints and the default multimedia output on a dedicated COM thread. The backend publishes stable endpoint IDs, display names, and Wired, Speakers, Bluetooth, HDMI, or Unknown routes through cached commands and change events.

Discovery is separate from saved listening profiles. Changing the Windows default output does not automatically change `listening_mode` or a profile delay in v0.13.61.

## Overlay controls

- Edit mode allows dragging and resizing.
- Locked mode keeps the overlay interactive but prevents accidental movement.
- Ghost mode makes the overlay click-through.
- `Ctrl+Alt+L` cycles the interaction mode.
- `Ctrl+Alt+B` toggles the blurred artwork background.
- `Ctrl+Alt+T` toggles transparent lyrics-only mode.
- `Ctrl+Alt+H` toggles the media information column.

The tray can show or hide Hum, change the interaction mode, switch the saved listening mode, open Settings, check for updates, and quit.

## OBS browser source

Enable OBS / Streamer in Settings, then copy the local URL into an OBS Browser Source. The default address is `http://localhost:38247/overlay` unless the port has changed.

The server binds to `127.0.0.1` and accepts only loopback Host headers. It serves current track and lyrics state, the saved timing projection, artwork, local assets, server-sent events, and a health endpoint. It does not need Spotify credentials or a cloud relay.

## Portable core status

Hum stays on Tauri for 1.0. The shared Rust shell, media contracts, timing policy, audio-output contracts, platform information, and native window interfaces compile on Windows, macOS, and Linux runners. `SharedShellState::new()` also provides a non-GUI smoke test for the exact neutral state used by production startup.

That boundary does not make Hum a macOS or Linux product yet:

- Windows has the media backend and audio-output discovery.
- macOS and Linux have no playback adapter.
- The portable workflow does not launch the GUI.
- The workflow does not package, sign, upload, or publish an installer.

The architecture decision is recorded in [ADR-0001](docs/decisions/ADR-0001-keep-tauri-and-add-platform-adapters.md). Current capability truth lives in `get_platform_info` and [Media and timing](docs/systems/media-and-timing.md).

## Architecture

| Layer | Current implementation |
|---|---|
| Interface | React 19, Vite 7, TypeScript 5.9 |
| Desktop shell | Tauri 2 |
| Shared media core | Platform-neutral Rust models, authority policy, publisher, and startup state |
| Windows playback | `Windows.Media.Control`, iTunes bridge, UI Automation, and WASAPI where needed |
| Lyrics | LRCLib plus strict NetEase YRC enrichment |
| Audio outputs | Cached neutral contract with a Windows MMDevice polling backend |
| Native windows | Platform-neutral backdrop, aspect, pointer, and screen-sampling seams |
| OBS | Axum on loopback with JSON, artwork, settings projection, and server-sent events |
| Persistence | Tauri store plus bounded or versioned application caches |
| Desktop services | Official Tauri plugins for shortcuts, autostart, updates, process control, and window state |

## Development

Install dependencies and run the Windows desktop app:

```bash
pnpm install --frozen-lockfile
pnpm tauri dev
```

Run the frontend checks:

```bash
pnpm typecheck
pnpm build
node --test src/platform-info-retry.test.mjs
```

Run the Rust checks from `src-tauri`:

```bash
cargo fmt --all --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cargo test shared_shell_state_smoke_preserves_neutral_defaults
```

The Windows NSIS installer is built separately with:

```bash
pnpm tauri build
```

The portable-core workflow compiles and tests. It never runs that packaging command.

Desktop app push policy: push after every commit.

## Planning and project records

- [1.0 roadmap](docs/ROADMAP.md)
- [System documentation](docs/systems/INDEX.md)
- [Architecture decisions](docs/decisions/INDEX.md)
- [HUM-00 Windows regression evidence](docs/verification/hum-00-windows-regression.md)
- [1.0 release checklist](docs/verification/1.0-release-checklist.md)
- [Changelog](docs/CHANGELOG.md)
- [Known defects](BUGS.md)
