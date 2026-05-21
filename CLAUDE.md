@../CLAUDE.md

# Hum

> Originally scaffolded as **Lyric Overlay**; renamed to **Hum** in v0.10.2 (2026-05-21). Identifier is `com.syvr.hum`; settings live at `%APPDATA%\com.syvr.hum\settings.json`.

Windows desktop overlay that displays real-time synced lyrics for whatever music is currently playing on the system. Reads track metadata from Windows SMTC, fetches lyrics from LRCLib, renders as an always-on-top window. Player-agnostic — works with Spotify, Chrome-YouTube, iTunes, anything that exposes itself to SMTC.

## Stack

| Layer | Choice |
|---|---|
| Frontend | React 19 + Vite 7 + TypeScript 5.9 |
| Styling | Inline styles (Phase 1 dev console). Tailwind 4 wired but unused for the overlay itself. |
| Desktop | Tauri 2 |
| SMTC bridge | `windows` crate 0.58 (`Media_Control` + `Foundation`) |
| Lyrics source | LRCLib (`/api/get` + `/api/search` fallback) — Phase 2 |
| Settings store | `tauri-plugin-store` — Phase 5 |
| Hotkeys | `tauri-plugin-global-shortcut` — Phase 4 |
| Package manager | pnpm |

## Architecture

Three layers, one direction:

1. **Source** (`src-tauri/src/smtc.rs`) — owns the `GlobalSystemMediaTransportControlsSessionManager`, subscribes to its `CurrentSessionChanged` plus per-session `MediaPropertiesChanged` / `TimelinePropertiesChanged` / `PlaybackInfoChanged`. COM event handlers dispatch through an mpsc channel into a tokio worker, which reads fresh state and emits Tauri events.
2. **Lyrics** (Phase 2 — not yet implemented) — on `track-changed`, hits LRCLib, parses LRC into `Vec<{ time_ms, text }>`, caches by track ID + by `artist|title|duration`.
3. **Render** (`src/App.tsx`) — Phase 1 is a dev console (current track + scrolling event log). Phase 3 replaces it with the actual overlay.

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

- **Phase 1: SMTC reader** — implemented. Manual verify across Spotify / Chrome-YouTube / iTunes still required before this is "done."
- Phases 2-6 — not started. See top-level spec.

## Branch & push policy

Tauri desktop app — `git push` only when Wes asks. Auto-push is **off**.

## Icons

Currently using Wren's icon set as a stand-in (SYVR Studios branded). Replace with a Hum icon via `pnpm tauri icon path/to/source.png` before any release.
