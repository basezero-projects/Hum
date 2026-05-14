# Lyric Overlay

Real-time synced lyrics overlay for Windows. Reads from system SMTC, so it works with whatever music app you're using — Spotify, Chrome-YouTube, iTunes, anything.

## Status

Phase 1 (SMTC reader) is built. Phases 2-6 (lyrics fetch, overlay render, modes/tray, settings, polish) are pending.

## Quickstart

```bash
pnpm install
pnpm tauri dev
```

Play music in any app. The dev console shows the current track and a live event log of what SMTC reports.

## Stack

Tauri 2 · React 19 · TypeScript · `windows` crate (0.58) for SMTC. See [`CLAUDE.md`](CLAUDE.md) for architecture.
