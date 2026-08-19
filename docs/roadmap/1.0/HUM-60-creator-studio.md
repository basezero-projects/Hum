# HUM-60: Creator Studio

Status: Proposed
Target release: 0.20.x
Depends on: HUM-20, HUM-40
Blocks: HUM-70
Last reviewed: 2026-08-19

## Outcome

A streamer can create, preview, copy, and monitor independent lyric outputs without fragile Spotify credentials or manual HTML editing.

## Why this phase exists

Hum already has a capable local OBS page. Turning that page into a managed creator workflow gives Hum a second audience and a feature that broad Windows overlays rarely handle well.

## In scope

- A dedicated Creator Studio screen with live preview
- Named output profiles with independent appearance settings
- OBS output profiles that consume the shared visual presets completed in HUM-40
- Transparent, solid, and chroma backgrounds
- Safe-area guides and stream resolution presets
- Multiple simultaneous local output URLs
- Copy URL, setup guidance, connection health, and reconnect state
- Import and export for creator profiles

## Out of scope

- Cloud-hosted overlays
- Remote collaboration
- Video rendering or lyric video export
- Direct OBS scene collection editing
- Spotify account or cookie authentication

## Architecture constraints

- The server remains loopback-only by default.
- Each output profile has a stable opaque identifier in its URL.
- Desktop and creator profiles share timing and selected lyrics but may style them independently.
- HUM-40 owns visual preset definitions and renderer behavior. HUM-60 owns output profiles, canvas settings, URLs, health, and OBS-specific backgrounds.
- Multiple outputs must not multiply provider requests or media polling.
- Profile data is versioned and safe to import.

## Acceptance criteria

- [ ] HUM-60-AC1: A user can create, rename, duplicate, delete, import, and export an output profile.
- [ ] HUM-60-AC2: Creator Studio previews the exact output OBS receives.
- [ ] HUM-60-AC3: Each shared HUM-40 stream preset works at 720p, 1080p, and 4K canvas sizes.
- [ ] HUM-60-AC4: Multiple browser sources can render different styles from one playback session.
- [ ] HUM-60-AC5: The app reports server health, port conflicts, connected clients, and reconnection state clearly.
- [ ] HUM-60-AC6: Copy URL and setup guidance produce a working OBS browser source without account credentials.
- [ ] HUM-60-AC7: Word timing, line timing, translations, local edits, audio profiles, seeking, and pausing match the desktop overlay.

## Required test matrix

- OBS on 720p, 1080p, 1440p, and 4K canvases
- One, two, and four simultaneous output profiles
- Transparent, chroma, and solid backgrounds
- Server restart, port conflict, sleep and wake, and OBS reconnect
- Word, line, plain, translated, instrumental, unavailable, and ad states
- Import of current and older profile versions

## Slice map

No implementation plan written yet.

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
