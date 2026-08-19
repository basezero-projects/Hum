# Hum

Hum is a Windows lyric overlay for listeners and streamers. It follows the active media session, resolves synchronized lyrics, and keeps the current line visible above the apps the listener already uses. The desktop overlay supports compact ribbon and square presentations, while the local OBS server provides a clean browser source for creators.

Current development version: v0.13.53

Hum is preparing for a paid 1.0 release. The current scope, phase order, and release gates live in [docs/ROADMAP.md](docs/ROADMAP.md).

## Current capabilities

- Word timing from NetEase YRC when the recording has a strict title, artist, and duration match
- LRCLib and NetEase line timing fallbacks
- Plain lyrics and explicit instrumental, unavailable, error, and unsupported states
- Ribbon layouts for three-line, single-line, and Full page lyrics
- A square focused-lyrics presentation inspired by Apple Music
- Edit, Locked, and Ghost interaction modes
- Wired, Speakers, and Bluetooth delay profiles
- Temporary timing nudges and expert calibration
- Album artwork, artwork-derived surfaces, native Windows backdrops, and automatic contrast
- Optional translated lyrics when provider data includes them
- Artist biographies, photos, and upcoming Ticketmaster dates
- A loopback-only OBS browser source with desktop timing parity
- Tray controls, global shortcuts, autostart, and persistent settings

## Playback sources

Hum uses the active Windows System Media Transport Controls session when a player publishes one. It also has source-specific adapters where Windows media metadata is incomplete.

| Source | Current path | Notes |
|---|---|---|
| Spotify desktop | Windows SMTC | Primary tested desktop source |
| Spotify web | Windows SMTC | Works through supported Chromium browsers |
| YouTube Music and music videos | Windows SMTC plus title normalization | Background browser tabs can limit source-specific enrichment |
| iTunes desktop | COM adapter | Used because classic iTunes does not publish a normal SMTC session |
| Pandora web | Chromium UI Automation bridge | Adds Pandora-specific metadata and ad state |
| Pandora desktop | UI Automation plus playback-state estimation | Seeking and joining mid-track remain less reliable than SMTC sources |
| Other Windows players | Windows SMTC | Compatibility depends on the metadata and timeline the player publishes |

Recognized source labels include Spotify, Pandora, iTunes, Apple Music, YouTube Music, TIDAL, Amazon Music, Deezer, VLC, foobar2000, MusicBee, Winamp, Windows Media, and common browsers.

## Timing controls

Settings > Lyrics timing separates three different corrections:

- Listening mode compensates for the active audio path. Wired defaults to 0 ms, Speakers to 250 ms, and Bluetooth to 350 ms.
- Expert calibration adjusts every listening mode.
- `Ctrl+Alt+[` and `Ctrl+Alt+]` temporarily move the current song in 250 ms steps.

Desktop and OBS use the same saved timing calculation. Temporary song nudges currently apply only to the desktop overlay.

## Overlay controls

- Edit mode allows dragging and resizing.
- Locked mode keeps the overlay interactive but prevents accidental movement.
- Ghost mode makes the overlay click-through.
- `Ctrl+Alt+L` cycles the interaction mode.
- `Ctrl+Alt+B` toggles the blurred artwork background.
- `Ctrl+Alt+T` toggles transparent lyrics-only mode.
- `Ctrl+Alt+H` toggles the media information column.

The tray can show or hide Hum, change the interaction mode, switch listening mode, open Settings, check for updates, and quit.

## OBS browser source

Enable OBS / Streamer in Settings, then copy the local URL into an OBS Browser Source. The default address is `http://localhost:38247/overlay` unless the port has been changed.

The server binds to the local computer and exposes lyric state, settings, artwork, service assets, server-sent events, and health information. It does not require Spotify credentials or a cloud relay.

## Install status

Development installers are published through [GitHub Releases](https://github.com/basezero-projects/Hum/releases). Hum is not at its commercial 1.0 release gate yet. License activation, signed update verification, onboarding, and the final customer installer are tracked in HUM-10 and HUM-70 of the roadmap.

## Technology

| Layer | Choice |
|---|---|
| Interface | React 19, Vite 7, TypeScript 5.9 |
| Desktop shell | Tauri 2 |
| Windows media sessions | `Windows.Media.Control` through the Rust `windows` crate |
| Windows source adapters | COM, UI Automation, and WASAPI where needed |
| Lyrics | LRCLib and NetEase YRC |
| OBS server | Axum on loopback with server-sent events |
| Persistence | Tauri store plus versioned application caches |
| Shortcuts and startup | Official Tauri plugins |

Hum stays on Tauri for 1.0 and future ports. Platform-specific media capture will move behind Rust interfaces before macOS or Linux work begins. The decision is recorded in [ADR-0001](docs/decisions/ADR-0001-keep-tauri-and-add-platform-adapters.md).

## Development

```bash
pnpm install
pnpm tauri dev
pnpm typecheck
pnpm build
cd src-tauri
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

The Windows NSIS installer is built with:

```bash
pnpm tauri build
```

Desktop app push policy: commit completed work locally, but do not push unless Wes asks.

## Planning and project records

- [1.0 roadmap](docs/ROADMAP.md)
- [Architecture decisions](docs/decisions/INDEX.md)
- [1.0 release checklist](docs/verification/1.0-release-checklist.md)
- [Changelog](docs/CHANGELOG.md)
- [Known defects](BUGS.md)
