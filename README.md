# Lyric Overlay

Real-time synced lyrics overlay for Windows streamers and music listeners. It reads whatever is playing via Windows SMTC — Spotify, Chrome, YouTube Music, iTunes, anything — fetches time-synced lyrics from LRCLib, and renders them as a transparent always-on-top window. Text color auto-adjusts to stay readable over any background. Streamers get a dedicated OBS/browser-source mode that serves lyrics as a local HTTP page.

**Current version:** v0.10.0 — shipping

---

## Features

- **Synced lyrics** — 3-line karaoke-style scroll with per-word sweep highlighting, fetched from LRCLib with a duration-filtered search fallback
- **Player-agnostic** — reads SMTC (Windows system media transport controls), so it works with Spotify, Chrome-based players, YouTube Music, and anything else that registers with the OS; iTunes is bridged separately via COM
- **Auto-contrast text** — reads the pixels behind the overlay at runtime and inverts text color when needed; on by default
- **OBS streamer mode** — exposes lyrics as a local browser source (`axum` HTTP server) so they render cleanly as an OBS browser capture without capture-card bleed
- **Overlay modes** — edit (drag/resize), locked (click-through), ghost (semi-transparent passthrough)
- **Album art** — pulled from SMTC or iTunes, shown beside the lyrics column; used to tint the overlay background
- **Global hotkeys** — `Ctrl+Alt+L` to lock/unlock overlay; `Ctrl+Alt+[` / `Ctrl+Alt+]` to nudge lyric timing offset live
- **Settings window** — source selection (SMTC / iTunes), text alignment, offset, background opacity, font size scaling
- **Auto-updater** — checks GitHub Releases on launch; installs in passive mode without interrupting playback

---

## Install

Download the latest `.exe` installer from [GitHub Releases](https://github.com/syvrstudios/lyric-overlay/releases/latest). Installs per-user (no admin required). The app auto-updates on subsequent launches.

---

## Usage

1. Launch Lyric Overlay. The overlay window appears in the top-left area; the dev console is hidden by default.
2. Play something in Spotify, Chrome, YouTube Music, or iTunes.
3. Lyrics appear and scroll in sync. Resize or drag the overlay to position it.
4. `Ctrl+Alt+L` — toggle locked mode (click-through) / edit mode.
5. `Ctrl+Alt+[` / `Ctrl+Alt+]` — nudge lyrics earlier or later if sync is off.
6. Right-click the tray icon to open Settings or quit.

**OBS streamer mode:** Enable in Settings → Sources → Streamer mode. Add a Browser Source in OBS pointed at `http://localhost:<port>` (shown in Settings). The browser source renders lyrics as a clean overlay layer.

---

## Tech stack

| Layer | Choice |
|---|---|
| Frontend | React 19 + Vite 7 + TypeScript 5.9 + Tailwind 4 |
| Desktop shell | Tauri 2 |
| SMTC bridge | `windows` crate 0.58 (`Media_Control` + `Foundation`) |
| iTunes bridge | COM via `itunes.rs` |
| Lyrics source | LRCLib (`/api/get` + `/api/search` fallback) |
| Streamer server | `axum` 0.8 (local HTTP) |
| Settings store | `tauri-plugin-store` |
| Hotkeys | `tauri-plugin-global-shortcut` |
| Auto-updater | `tauri-plugin-updater` → GitHub Releases |

---

## Development

```bash
pnpm install
pnpm tauri dev       # opens overlay + dev console
pnpm typecheck       # tsc --noEmit
cd src-tauri && cargo check
cd src-tauri && cargo clippy
```

To build a release installer:

```bash
pnpm tauri build
# outputs: src-tauri/target/release/bundle/nsis/Lyric Overlay_0.10.0_x64-setup.exe
```

> Push policy: desktop app — never push without Wes asking.
