# HUM-70: Accessibility and release hardening

Status: Proposed
Target release: 1.0.0
Depends on: HUM-10 through HUM-60
Blocks: Hum 1.0
Last reviewed: 2026-08-19

## Outcome

The public Hum 1.0 build is accessible, signed, fast, recoverable, documented, and tested across the player and display combinations customers will actually use.

## Why this phase exists

Feature completion is not release readiness. The final phase measures the whole product under clean-install, long-session, failure, accessibility, and upgrade conditions.

## In scope

- Large text, high contrast, keyboard navigation, screen-reader labels, and shortcut customization
- Performance budgets and long-session testing
- Crash recovery and customer-safe diagnostics export
- Player, source, display, audio-device, network, OBS, and license matrices
- Signed installer, update-feed withdrawal, manual prior-build reinstall, clean-install, and upgrade verification
- Current README, release notes, website claims, privacy, support, and refund information
- Final execution of the 1.0 release checklist

## Out of scope

- New headline features
- macOS or Linux release builds
- Mobile apps
- Post-launch analytics experiments

## Architecture constraints

- Accessibility features must work in every built-in preset.
- Diagnostics use an allowlist. They may include app version, platform, source type, timing mode, status codes, and sanitized log messages. They exclude secrets, license tokens, settings values, full local paths, raw lyrics, and provider responses. Track and artist metadata are excluded unless the customer explicitly includes them.
- Performance testing uses release builds and repeatable scenarios.
- A failed stop condition blocks release. It is not waived by changing documentation after the test.

## Acceptance criteria

- [ ] HUM-70-AC1: Every required phase contains complete version, commit, validation, and changelog evidence.
- [ ] HUM-70-AC2: The overlay and Settings are usable by keyboard and expose meaningful screen-reader labels.
- [ ] HUM-70-AC3: Large text and high-contrast modes remain readable in every supported layout.
- [ ] HUM-70-AC4: Release CPU, memory, startup, lookup, and idle budgets pass on the defined test hardware.
- [ ] HUM-70-AC5: A long playback session survives track changes, ads, pauses, seeks, output changes, sleep, wake, and display changes.
- [ ] HUM-70-AC6: Clean install, license activation, update from the prior release, update-feed withdrawal, manual reinstall of the prior signed build, and uninstall pass with signed artifacts while preserving compatible customer data.
- [ ] HUM-70-AC7: Offline, provider failure, bad metadata, port conflict, and update failure paths explain what happened and offer recovery.
- [ ] HUM-70-AC8: README, release notes, website, privacy, pricing, refund, and support claims match the shipped build.
- [ ] HUM-70-AC9: Every required item in `docs/verification/1.0-release-checklist.md` passes with recorded evidence.

## Required test matrix

The authoritative matrix is `docs/verification/1.0-release-checklist.md`.

## Slice map

No implementation plan written yet.

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
