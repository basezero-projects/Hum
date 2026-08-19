# HUM-50: Language and offline resilience

Status: Proposed
Target release: 0.19.x
Depends on: HUM-20
Blocks: HUM-70
Last reviewed: 2026-08-19

## Outcome

Listeners can understand supported foreign-language songs and keep using lyrics they have already loaded when the network is unavailable.

## Why this phase exists

Translations and romanization are becoming expected lyric features. Offline behavior also needs to be visible and dependable instead of feeling like an accidental cache hit.

## In scope

- Separate original, translation, romanization, and pronunciation controls
- Provider-supplied translations with clear attribution
- Local deterministic romanization where appropriate
- Original plus translation and original plus romanization layouts
- Visible cache size, song count, and clear controls
- Instant cached replay and explicit offline states
- Offline use of local records created through the Lyrics Control Center

## Out of scope

- Paid per-request AI translation
- Human translation marketplace
- Cloud lyric backup
- Automatic language tutoring features

## Architecture constraints

- Original lyrics are never silently replaced by generated text.
- Generated romanization and provider translation are labeled separately.
- The one-time purchase cannot depend on an unbounded paid service.
- Cache formats remain versioned and recoverable.
- Desktop and OBS use the same selected language layers.

## Acceptance criteria

- [ ] HUM-50-AC1: A user can independently toggle original text, translation, romanization, and pronunciation when available.
- [ ] HUM-50-AC2: The overlay explains why a language layer is unavailable instead of showing an empty row.
- [ ] HUM-50-AC3: Cached lyrics load without a network request. Hum shows an offline indicator when a lookup uses local or cached data while disconnected, or when disconnection prevents a result. The indicator stays hidden during normal connected playback.
- [ ] HUM-50-AC4: Settings show cache size and song count and can clear provider cache without deleting local edits.
- [ ] HUM-50-AC5: Unicode, CJK, diacritics, and right-to-left scripts preserve text order and timing.
- [ ] HUM-50-AC6: Language selections persist per user, with optional per-song overrides.

## Required test matrix

- English, Spanish, Japanese, Korean, Simplified Chinese, and right-to-left samples
- Translation only, romanization only, both, and neither
- Word timing and line timing with secondary text
- Online lookup, cached replay, network loss during lookup, and provider timeout
- Cache migration and clear operations with local edits present

## Slice map

No implementation plan written yet.

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
