# Hum

Hum is a Windows lyric overlay that follows whatever you are playing and keeps the current line on screen, above the apps you already use.

Status: pre-1.0, current development version v0.13.99. A paid 1.0 release is planned.

## Screenshots

> Screenshots and a short GIF have not been added yet. To add them, drop the files in `assets/screenshots/` (for example `overlay-ribbon.png`, `overlay-square.png`, `settings.png`, `obs-source.gif`) and link them here with `![Ribbon overlay](assets/screenshots/overlay-ribbon.png)`.

## Features

- Word-timed lyrics from NetEase YRC when title, artist, and duration match the recording
- LRCLib synced and plain lyrics, with NetEase line timing as a later fallback
- Distinct states for instrumental tracks, missing lyrics, errors, ads, and unsupported sources
- Ribbon layouts (three-line, single-line, full page) and a square focused-lyrics layout
- Edit, Locked, and Ghost (click-through) interaction modes
- Wired, Speakers, and Bluetooth delay profiles, plus a temporary per-track nudge
- Album artwork, artwork-derived surfaces, Windows backdrops, and automatic text contrast
- Optional translated lyrics when the provider includes them
- Artist biography and photo
- A loopback-only OBS browser source that mirrors the overlay
- Tray controls, global shortcuts, autostart, and signed automatic updates

## How it works

Hum is a Tauri 2 app. The Rust backend reads playback state, resolves lyrics, and serves the OBS page. A React 19 frontend renders the overlay, settings, and artist panel.

1. **Playback sources.** Hum reads the Windows System Media Transport Controls (SMTC) session. Adapters fill gaps for iTunes (PowerShell and COM), Pandora web and desktop (UI Automation), and YouTube (browser window title). A playing SMTC session with a real title wins over any bridge estimate.
2. **Lyrics.** The resolver checks a memory cache and an on-disk cache first. LRCLib and NetEase are then queried together. NetEase YRC word timing is used only after strict metadata and duration checks. LRCLib supplies the normal synced or plain result.
3. **Lyrics proxy.** Some networks block `lrclib.net` by hostname. Hum tries LRCLib directly first and falls back to a small Cloudflare Worker at `lyrics.syvr.dev` only when the direct connection fails. The Worker source is in [worker/lyrics-proxy](worker/lyrics-proxy). It proxies two read-only LRCLib paths and nothing else.
4. **Timing.** Saved timing is `saved_offset_ms = anticipate_ms - selected_profile_delay_ms`. Wired defaults to 0 ms, Speakers to 250 ms, Bluetooth to 350 ms. Details are in [Media and timing](docs/systems/media-and-timing.md).
5. **OBS.** An Axum server bound to `127.0.0.1` serves the same state to an OBS Browser Source. It accepts only loopback Host headers and needs no cloud relay. Enable it in Settings and use the local URL shown there (default port 38247).
6. **Updates.** Releases are built in GitHub Actions, Authenticode signed, and the installer is signed again with a Tauri updater key. The app checks the GitHub releases feed and verifies the signature before installing.
7. **Licensing.** The paid release uses Polar for checkout and license keys. The client activates and validates keys through Polar's public customer API and keeps protected offline state. The decision is recorded in [ADR-0002](docs/decisions/ADR-0002-use-polar-and-protected-offline-license-state.md).

The architecture decision to stay on Tauri and add platform adapters is in [ADR-0001](docs/decisions/ADR-0001-keep-tauri-and-add-platform-adapters.md). The shared core (media models, timing policy, platform information, native window interfaces) compiles on Windows, macOS, and Linux in CI. Only Windows has a playback backend, so Hum is Windows only today.

## Install

Hum is not publicly released yet. When it is, installers will be on the [GitHub Releases page](https://github.com/basezero-projects/Hum/releases), and installed copies update themselves from there.

## Build from source

Requirements: Windows 10 or 11, Node.js, pnpm, and a Rust toolchain.

```bash
pnpm install --frozen-lockfile
pnpm tauri dev      # run the desktop app
pnpm tauri build    # build the NSIS installer (unsigned unless signing keys are set)
```

License features need `HUM_POLAR_ORGANIZATION_ID`, `HUM_POLAR_CHECKOUT_URL`, and `HUM_POLAR_CUSTOMER_PORTAL_URL` in the build environment (see `.env.example`). Without them Hum builds with licensing disabled.

## Tests

| Suite | Command | Count |
|---|---|---|
| Rust (from `src-tauri`) | `cargo test --all-targets` | 294 passing, 2 ignored |
| Frontend and release scripts | `pnpm test` | 68 passing |
| TypeScript | `pnpm typecheck` | type check only |
| Rust lint | `cargo clippy --all-targets -- -D warnings` | clean |
| Worker (from `worker/lyrics-proxy`) | `pnpm typecheck` | type check only, no unit tests |

CI runs a portable-core workflow on every push. Release builds run only when a `v*` tag is pushed or the workflow is started by hand.

## Tech stack

- Tauri 2, Rust (`windows`, `uiautomation`, `axum`, `tokio`)
- React 19, Vite 7, TypeScript 5.9, Tailwind 4
- Cloudflare Workers for the lyrics proxy
- Polar for licensing, GitHub Actions for builds and signing

## License

No license file has been added yet, so all rights are reserved by default. The project owner has not chosen one.

## Changelog

See [docs/CHANGELOG.md](docs/CHANGELOG.md) for every change, newest first.
