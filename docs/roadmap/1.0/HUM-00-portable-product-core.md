# HUM-00: Portable product core

Status: In progress
Target release: 0.14.x
Depends on: None
Blocks: HUM-10 through HUM-70
Last reviewed: 2026-08-19

## Outcome

Hum behaves the same on Windows, but shared product code no longer assumes that every media session or window feature comes from Windows.

## Why this phase exists

React, lyrics, timing, caches, artwork, settings, and OBS can travel to another operating system. Media capture and native window effects cannot. Creating that boundary now is much cheaper than rebuilding after more features import Windows state directly.

## In scope

- Shared track, playback, artwork, source, and capability models
- A media backend interface and Windows backend manager
- An audio-output backend interface with platform-neutral device identity
- Windows SMTC, iTunes, UI Automation, and WASAPI moved behind the interface
- Native window effects, aspect behavior, click-through exceptions, and screen sampling behind platform interfaces
- A platform-capabilities command for React
- Platform-neutral settings paths and user-facing copy
- Target-gated Windows dependencies
- Windows, macOS, and Linux compile plus shared-shell smoke jobs
- Current media and timing system documentation

## Out of scope

- Shipping macOS or Linux builds
- Replacing the Windows media implementations
- Changing lyric provider behavior
- Adding a browser extension
- Redesigning the overlay

## Architecture constraints

- Follow [ADR-0001](../../decisions/ADR-0001-keep-tauri-and-add-platform-adapters.md).
- Shared lyric and streamer code consumes a platform-neutral snapshot.
- Shared timing settings consume platform-neutral audio-output records.
- React receives capabilities from Rust instead of scattering operating system checks.
- The refactor must not change Windows playback priority, event names, lyric timing, or OBS output.

## Acceptance criteria

- [x] HUM-00-AC1: The shared `CurrentTrack` snapshot contains every field used by lyrics, the overlay, artist information, and OBS. Exact wire-contract tests shipped in v0.13.54.
- [ ] HUM-00-AC2: Windows SMTC, iTunes, browser probes, and Pandora playback detection publish through one backend boundary.
- [ ] HUM-00-AC3: Lyrics and streamer modules do not import a Windows-specific snapshot type.
- [x] HUM-00-AC4: Windows-only crates are declared under Windows target dependencies. Completed in v0.13.55.
- [ ] HUM-00-AC5: React receives one tested platform-capabilities payload and hides unsupported native effects from it.
- [ ] HUM-00-AC6: Settings displays the resolved application data path returned by Rust.
- [ ] HUM-00-AC7: Existing Windows player priority, timing, tray, overlay, and OBS behavior pass regression tests.
- [ ] HUM-00-AC8: `docs/systems/media-and-timing.md` describes the implemented boundary and event flow.
- [ ] HUM-00-AC9: Audio-output discovery publishes platform-neutral devices and active-output changes through its own interface.
- [ ] HUM-00-AC10: Backdrop effects, aspect behavior, screen sampling, and click-through exceptions are isolated behind native window interfaces.
- [ ] HUM-00-AC11: Shared models and modules contain no Windows types or unconditional Windows dependencies.
- [ ] HUM-00-AC12: Windows, macOS, and Linux CI jobs compile the shared shell and run its smoke tests.

## Required test matrix

- Spotify desktop through SMTC
- A Chromium player through SMTC and browser normalization
- iTunes desktop through COM
- Pandora desktop and web
- Playing, paused, seeking, track change, ad break, and nothing playing
- Ribbon, Square, and OBS word and line timing
- Edit, Locked, and Ghost modes
- Windows, macOS, and Linux shared-shell compile and startup smoke tests

## Slice map

- [HUM-00A shared media model](../../superpowers/plans/2026-08-19-hum-00a-shared-media-model.md), Complete in v0.13.54
- [HUM-00B build boundary](../../superpowers/plans/2026-08-19-hum-00b-build-boundary.md), Complete in v0.13.55
- [HUM-00 progress ledger](HUM-00-progress-ledger.md), current execution cursor and completion record

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
