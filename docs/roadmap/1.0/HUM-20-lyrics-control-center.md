# HUM-20: Lyrics Control Center

Status: Proposed
Target release: 0.16.x
Depends on: HUM-00
Blocks: HUM-40, HUM-50, HUM-60, HUM-70
Last reviewed: 2026-08-19

## Outcome

A user can understand where the current lyrics came from and fix an incorrect, missing, or poorly timed result without leaving Hum.

## Why this phase exists

Automatic matching will never be perfect. A premium lyric app needs a recovery path that is as polished as its automatic path. Local corrections should remain available even when a provider changes or the computer is offline.

## In scope

- Current provider and timing-quality status
- Manual search and alternate result selection
- Per-song provider choice and timing correction
- Import for LRC, LRCX, SRT, VTT, and plain text
- Local text and timestamp editing
- Retry and current-song cache controls
- Local override priority and backup format
- A clear incorrect-lyrics action

## Out of scope

- Public community editing
- Uploading corrections to providers without their supported API
- Cloud synchronization
- Collaborative editing
- Lyric video authoring

## Architecture constraints

- Preserve the resolver order and fallbacks documented by the lyric system.
- Store local corrections separately from provider cache entries.
- Version imported and edited lyric formats.
- A malformed import cannot replace a working cached result without confirmation.
- Desktop and OBS consume the same selected lyric record.

## Acceptance criteria

- [ ] HUM-20-AC1: The current song shows provider, word, line, or plain timing, translation availability, and local-edit state.
- [ ] HUM-20-AC2: A user can search, preview, and choose an alternate match.
- [ ] HUM-20-AC3: The selected match remains attached to the correct recording across restarts.
- [ ] HUM-20-AC4: Supported files import with a preview and validation report.
- [ ] HUM-20-AC5: A user can edit lyric text and line timing, save it locally, and restore the provider version.
- [ ] HUM-20-AC6: A saved per-song offset applies in addition to the active audio-device profile.
- [ ] HUM-20-AC7: Retry and clear-current-song actions do not remove unrelated cached lyrics.
- [ ] HUM-20-AC8: Local corrections work offline and appear identically in the desktop overlay and OBS.

## Required test matrix

- Correct match, wrong match, duplicate title, remix, live version, and missing duration
- Word-timed, line-timed, plain, translated, instrumental, and malformed provider data
- Every import format with valid, partial, and invalid samples
- Offline launch with provider cache and local correction
- Unicode, right-to-left text, punctuation, and blank timing lines

## Slice map

No implementation plan written yet.

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
