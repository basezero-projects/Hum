@../CLAUDE.md

# Hum

> Originally scaffolded as **Lyric Overlay**; renamed to **Hum** in v0.10.2 (2026-05-21). Identifier is `com.syvr.hum`; settings live at `%APPDATA%\com.syvr.hum\settings.json`.

Windows desktop overlay that displays real-time synced lyrics for whatever music is currently playing on the system. Reads track metadata from Windows SMTC, fetches lyrics from LRCLib, renders as an always-on-top window. Player-agnostic — works with Spotify, Chrome-YouTube, iTunes, anything that exposes itself to SMTC.

## Stack

| Layer | Choice |
|---|---|
| Frontend | React 19 + Vite 7 + TypeScript 5.9 + Tailwind 4 (cn() helper via `lib/utils.ts`); Overlay uses inline styles for hot-path rendering |
| Desktop | Tauri 2 |
| SMTC bridge | `windows` crate 0.58 (`Media_Control` + `Foundation`) |
| Browser bridges | `uiautomation` crate 0.25 (Chrome UIA for Pandora web + YouTube; Pandora desktop UIA + WASAPI peak meter) |
| iTunes bridge | PowerShell subprocess (`itunes.rs` spawns + reads JSON lines) |
| Lyrics source | LRCLib primary; SimpMusic + NetEase fallback chain |
| Artist info | Wikipedia REST + TheAudioDB + Ticketmaster Discovery API |
| Streamer server | `axum` 0.8 (loopback HTTP, serves `streamer_overlay.html`) |
| Promo source | `https://syvrstudios.com/hum/promos.json` (hot-swappable, 6h refresh, disk-cached) |
| Settings store | `tauri-plugin-store` |
| Hotkeys | `tauri-plugin-global-shortcut` |
| Auto-updater | `tauri-plugin-updater` → GitHub Releases (signing key not yet wired) |
| Package manager | pnpm |

## Architecture

Four layers, source → blend → resolve → render:

1. **Sources** — owns the now-playing snapshot.
   - `src-tauri/src/smtc.rs` — Windows SMTC reader. Owns `GlobalSystemMediaTransportControlsSessionManager`, subscribes to `CurrentSessionChanged` + per-session `MediaPropertiesChanged` / `TimelinePropertiesChanged` / `PlaybackInfoChanged`. COM event handlers dispatch through mpsc into a tokio worker. Detects Spotify ads via title heuristics + duration < 35s.
   - `src-tauri/src/itunes.rs` — iTunes adapter via PowerShell subprocess.
   - `src-tauri/src/web_bridge.rs` + `youtube_bridge.rs` — Chrome UIA bridges for Pandora web + YouTube ad detection.
   - `src-tauri/src/pandora_desktop.rs` — Pandora native desktop app via UIA tree walks + WASAPI peak meter.
2. **Blend** (`smtc.rs::emit_blended` + `web_bridge.rs::blend_bridge_into_snapshot`) — merges SMTC and bridge state into a `SharedSnapshot`. Owns the `ad_active` flag set/clear semantics (Spotify ad → true; real song → false; bridge sources owned by the bridge path).
3. **Lyrics** (`src-tauri/src/lyrics.rs`) — on `track-changed` / `web-bridge-updated` / `timeline-changed`, fetches from LRCLib (`/api/get` + `/api/search`), falling back to SimpMusic + NetEase. Parses LRC into `Vec<{ time_ms, text }>`. Caches in-memory by track key. Short-circuits to `Status::Ad` when `ad_active` is true.
4. **Render** — multi-window React app:
   - `src/Overlay.tsx` — main always-on-top lyric overlay (the user-facing window). PromoCard renders during ad breaks.
   - `src/DevConsole.tsx` — developer window for live event/state inspection.
   - `src/Settings.tsx` — settings panel (sources, alignment, theming, hotkeys, streamer mode).
   - `src/artist-panel/ArtistPanel.tsx` — side panel that opens on album-art click, fed by `artist_info.rs`.
   - `src-tauri/src/streamer_overlay.html` — OBS browser source served by the `axum` server in `streamer.rs`.
   - `main.tsx::pickComponent()` routes by Tauri window label.

## Tauri events emitted by the Rust side

All three carry the same flat `CurrentTrack` payload — frontend reads whichever fields it cares about:

```ts
type CurrentTrack = {
  title: string;
  artist: string;
  album: string;
  duration_ms: number;
  position_ms: number;
  last_update_unix_ms: number;  // for client-side interpolation
  state: "unknown" | "closed" | "opened" | "changing" | "stopped" | "playing" | "paused";
  source_app_id: string | null;
};
```

| Event | Fires when |
|---|---|
| `track-changed` | Title/artist/album/duration changes (or session changes) |
| `timeline-changed` | Position update (Windows pushes these every few seconds while playing) |
| `playback-state-changed` | Play/pause/stop transition |

Plus the Tauri command `get_current_track()` returns the current snapshot synchronously.

## Build & run

```bash
pnpm install
pnpm tauri dev       # full desktop dev (opens app window with dev console)
pnpm typecheck       # tsc --noEmit
cd src-tauri && cargo check
cd src-tauri && cargo clippy
```

## Phase status

Current version: **v0.13.0**. Shipped through 2026-05-22.

- Phase 1 (SMTC source) — shipped.
- Phase 2 (lyric fetch via LRCLib + fallback chain) — shipped.
- Phase 3 (overlay render, 3-line karaoke scroll, per-word sweep) — shipped.
- Phase 4 (global hotkeys for lock/unlock + offset nudge) — shipped.
- Phase 5 (settings store + Settings window) — shipped.
- Phase 6 (streamer mode via local axum HTTP server) — shipped.
- Phase 7 (browser bridges: Pandora web, Pandora desktop, YouTube web; iTunes via PowerShell) — shipped.
- Phase 8 (artist info panel: Wikipedia bio + TheAudioDB photo + Ticketmaster tour dates with impact.com affiliate links) — shipped.
- Phase 9 (ad-break detection across Spotify / Pandora / YouTube + SYVR promo card rotation, hot-swappable via `promos.json`) — shipped (v0.12.0–v0.12.4).
- Phase 10 (image-driven PromoCards — advertisers can ship a designed 1920×240 hero image) — shipped (v0.13.0).

**Next planned slices** (see top-level task list):
- Cross-platform refactor (extract `MediaSource` trait, add Linux MPRIS + macOS MediaRemote).
- Updater signing key + GitHub Actions release CI + Claude-desktop-style relaunch UX.
- Analytics foundation on Hetzner (install + heartbeat + daily aggregate; local SQLite for per-user history).

See `BUGS.md` for known limitations and `docs/CHANGELOG.md` for per-commit detail.

## Branch & push policy

Tauri desktop app — `git push` only when Wes asks. Auto-push is **off**.

## Icons

Currently using Wren's icon set as a stand-in (SYVR Studios branded). Replace with a Hum icon via `pnpm tauri icon path/to/source.png` before any release.
