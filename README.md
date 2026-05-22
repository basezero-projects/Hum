# Hum

Real-time synced lyrics overlay for Windows streamers and music listeners. It reads whatever is playing via Windows SMTC — Spotify, Chrome, YouTube Music, iTunes, anything — fetches time-synced lyrics from LRCLib, and renders them as a transparent always-on-top window. Text color auto-adjusts to stay readable over any background. Streamers get a dedicated OBS/browser-source mode that serves lyrics as a local HTTP page.

**Current version:** v0.11.7 — shipping

> Renamed from "Lyric Overlay" → "Hum" in v0.10.2 (2026-05-21). Identifier and install path moved from `com.syvr.lyric-overlay` → `com.syvr.hum`. Existing settings on disk are not migrated automatically — re-set your preferences in the new Settings window on first launch.

---

## Source compatibility

Hum reads "now playing" data from three layers, fallthrough in priority order:

| Source | How Hum reads it | Status |
|---|---|---|
| **iTunes desktop app** | COM bridge (`itunes.rs`) | ✅ Confirmed working |
| **Spotify desktop app** | Windows SMTC | ✅ Confirmed working |
| **Spotify web (Chrome / Edge / etc.)** | Windows SMTC | ✅ Confirmed working |
| **YouTube web (Chrome / Edge / etc.)** | Windows SMTC | ✅ Confirmed working |
| **Pandora web (Chrome / Edge / etc.)** | Chrome UIA bridge → reads Pandora's React DOM for the now-playing widget | ✅ Confirmed working |
| **Pandora desktop app (Microsoft Store)** | Direct UIA tree walk + WASAPI peak-meter for pause state | ⚠️ Semi-working: track + artist + pause detection work, but playback position is estimated from "when Hum first saw the track" — Hum can't read Pandora's seek bar so lyrics scroll from 0:00, and joining mid-track or seeking inside Pandora desn't re-sync |
| Anything else that publishes to SMTC | Windows SMTC | 🤷 Untested but should work — every SMTC-publishing player follows the same path |

---

## Features

- **Synced lyrics** — 3-line karaoke-style scroll with per-word sweep highlighting, fetched from LRCLib with a duration-filtered search fallback
- **Player-agnostic** — see the source-compatibility table above
- **Auto-contrast text** — composites blurred album art + tint + user bg + screen pixels behind the overlay, computes a luminance × (1 − saturation) lightness score with hysteresis, and only flips to dark text when the surface is genuinely white-ish (not just bright-and-tinted)
- **OBS streamer mode** — exposes lyrics as a local browser source (`axum` HTTP server) so they render cleanly as an OBS browser capture without capture-card bleed
- **Overlay modes** — edit (drag/resize), locked (click-through), ghost (semi-transparent passthrough)
- **Album art** — pulled from SMTC, iTunes COM, or the iTunes Search art fallback; shown beside the lyrics column; used to tint the overlay background
- **Artist info panel** — click the album art to open a side panel with the artist's Wikipedia bio (auto-fetched, disambiguator-aware), photo from TheAudioDB, and upcoming Ticketmaster tour dates with affiliate-routed ticket links
- **Global hotkeys** — `Ctrl+Alt+L` to lock/unlock overlay; `Ctrl+Alt+[` / `Ctrl+Alt+]` to nudge lyric timing offset live
- **Settings window** — source selection (SMTC / iTunes), text alignment, offset, background opacity, font size scaling, blurred-album-art background toggle, artist-info-panel toggle
- **Auto-updater** — checks GitHub Releases on launch; installs in passive mode without interrupting playback

---

## Install

Download the latest `.exe` installer from [GitHub Releases](https://github.com/basezero-projects/Hum/releases/latest). Installs per-user (no admin required). The app auto-updates on subsequent launches.

---

## Usage

1. Launch Hum. The overlay window appears in the top-left area; the dev console is hidden by default.
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
| Pandora web bridge | UIA via `uiautomation` crate 0.25, walks Chrome's accessibility tree (`web_bridge.rs`) |
| Pandora desktop bridge | UIA tree walk + WASAPI peak meter for pause detection (`pandora_desktop.rs`) |
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
# outputs: src-tauri/target/release/bundle/nsis/Hum_0.11.7_x64-setup.exe
```

> Push policy: desktop app — never push without Wes asking.
