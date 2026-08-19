# HUM-30: Automatic audio profiles

Status: Proposed
Target release: 0.17.x
Depends on: HUM-00
Blocks: HUM-40, HUM-70
Last reviewed: 2026-08-19

## Outcome

Hum notices when the Windows audio output changes and applies the delay saved for that physical device.

## Why this phase exists

Bluetooth speakers, televisions, headphones, and wired outputs can have noticeably different latency. Generic listening modes solve the problem manually. Device profiles can remove the repeated switch entirely.

## In scope

- Active Windows output-device detection
- Profiles keyed to stable device identity with editable names
- Automatic profile switching
- Per-device delay and a guided calibration tool
- Manual fallback profiles
- Tray and Settings controls
- Desktop and OBS timing parity

## Out of scope

- Measuring acoustic latency through a microphone
- Changing Windows audio routing
- Per-application volume control
- macOS and Linux output-device adapters

## Architecture constraints

- Device identity and enumeration stay behind a platform audio interface.
- Existing Wired, Speakers, and Bluetooth settings migrate without losing values.
- The timing equation keeps device delay, expert calibration, per-song correction, and temporary nudge separate.
- A missing or disconnected device falls back safely instead of resetting saved values.

## Acceptance criteria

- [ ] HUM-30-AC1: Hum shows the active Windows output using its friendly device name.
- [ ] HUM-30-AC2: Switching outputs applies the matching saved delay without reopening Settings.
- [ ] HUM-30-AC3: A new output creates a profile through a short, understandable prompt.
- [ ] HUM-30-AC4: The calibration tool explains and previews earlier versus later lyric movement.
- [ ] HUM-30-AC5: Generic listening modes remain available when device detection is unavailable.
- [ ] HUM-30-AC6: Desktop and OBS use the same automatically selected device delay.
- [ ] HUM-30-AC7: Device rename, reconnect, removal, and default-output changes preserve the right profile.

## Required test matrix

- Wired headphones, USB audio, Bluetooth, HDMI television, and built-in speakers
- Output change while playing, paused, seeking, and during an ad break
- Disconnected default device and rapid device switching
- Existing v0.13.52 settings migration
- Word timing, line timing, and per-song correction combinations

## Slice map

No implementation plan written yet.

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
