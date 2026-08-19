# Hum 1.0 roadmap

Last reviewed: 2026-08-19

This file is the authority for what remains before Hum 1.0. It defines the order of work and links to the contract for each phase. Completed work belongs in `docs/CHANGELOG.md`, not here.

## Product promise

Hum gives Windows listeners an Apple-quality lyric experience across supported players. It combines accurate timing, a flexible desktop overlay, and creator-ready output without an account or subscription.

## What 1.0 means

Hum 1.0 is ready when a new customer can buy, install, understand, and trust the app without developer help. Lyrics must be accurate when provider data allows it, and every incorrect or missing result must have a clear recovery path. The overlay must feel deliberate across layouts, displays, audio devices, and OBS.

Windows is the 1.0 release platform. The architecture must preserve a clean path to macOS and Linux, but those ports do not block the Windows release.

## Framework direction

Hum stays on Tauri 2. The React interface, lyric engine, settings, caches, artwork services, and OBS server remain shared. Platform-specific media capture and native window behavior move behind Rust interfaces before more operating systems are added.

The reasoning and constraints are recorded in [ADR-0001](decisions/ADR-0001-keep-tauri-and-add-platform-adapters.md).

## Status legend

- Proposed: The outcome is accepted, but the contract is not locked.
- Ready: Scope and acceptance criteria are locked. Work may begin.
- In progress: A slice is actively being implemented.
- Blocked: A named dependency or decision prevents progress.
- Complete: Every acceptance criterion has evidence.
- Dropped: The phase is no longer part of 1.0, with a reason recorded.

## Phase map

| ID | Phase | User outcome | Status | Depends on | Contract |
|---|---|---|---|---|---|
| HUM-00 | Portable product core | Windows behavior stays intact while platform assumptions are isolated | In progress | None | [Contract](roadmap/1.0/HUM-00-portable-product-core.md) |
| HUM-10 | Purchase, trust, and first run | A customer can buy, activate, install, learn, update, and get help | Proposed | HUM-00 | [Contract](roadmap/1.0/HUM-10-purchase-trust-first-run.md) |
| HUM-20 | Lyrics Control Center | A user can inspect, replace, import, correct, and save lyrics | Proposed | HUM-00 | [Contract](roadmap/1.0/HUM-20-lyrics-control-center.md) |
| HUM-30 | Automatic audio profiles | Hum follows the active output device and applies its saved delay | Proposed | HUM-00 | [Contract](roadmap/1.0/HUM-30-automatic-audio-profiles.md) |
| HUM-40 | Premium presentation | Presets and direct lyric controls make every layout feel finished | Proposed | HUM-20, HUM-30 | [Contract](roadmap/1.0/HUM-40-premium-presentation.md) |
| HUM-50 | Language and offline resilience | Translation, romanization, and cached lyrics remain understandable and available | Proposed | HUM-20 | [Contract](roadmap/1.0/HUM-50-language-and-offline.md) |
| HUM-60 | Creator Studio | Streamers get independent, named, testable OBS outputs | Proposed | HUM-20, HUM-40 | [Contract](roadmap/1.0/HUM-60-creator-studio.md) |
| HUM-70 | Accessibility and release hardening | The signed 1.0 build is fast, recoverable, accessible, and verified | Proposed | HUM-10 through HUM-60 | [Contract](roadmap/1.0/HUM-70-release-hardening.md) |

## Build order

```text
HUM-00
  -> HUM-10
  -> HUM-20
  -> HUM-30
  -> HUM-40
  -> HUM-50
  -> HUM-60
  -> HUM-70
  -> Hum 1.0
```

The phase dependencies allow some work to overlap in theory. In practice, Hum completes and closes one implementation slice before starting the next. This keeps every commit releasable and makes regressions easier to locate.

## Requirements that apply to every phase

- Keep the Windows experience working while platform boundaries are introduced.
- Store customer corrections and personal settings locally by default.
- Do not require an account for normal use.
- Do not show promotional cards by default in the paid product.
- Avoid a runtime service cost that cannot be supported by a one-time purchase.
- Give every failure state a useful explanation and recovery action.
- Keep desktop and OBS timing behavior consistent.
- Make platform-specific features capability-driven instead of hiding operating system checks throughout React.
- Keep Windows-only crates in target-specific dependency tables and keep Windows types out of shared models.
- Compile and smoke-test the shared shell on Windows, macOS, and Linux after HUM-00, even though only Windows ships in 1.0.
- Add tests for every new persistence format, resolver rule, and timing calculation.
- Record consequential architecture choices in `docs/decisions/`.

## Explicit 1.0 non-goals

- Shipping macOS or Linux builds
- Mobile apps
- User accounts or cloud synchronization
- Social feeds or community profiles
- AI translation with an ongoing per-request cost
- Lyric video export
- Expanding artist discovery beyond the existing panel
- A Mac App Store release

## Portability after 1.0

The first porting proof should be a Linux MPRIS backend because it has a public cross-player media standard. A cross-platform browser bridge should follow. macOS comes after a supported-player strategy is chosen, since Apple does not publish a general API for reading any other app's active media session.

## Release authority

The final release gate lives in [docs/verification/1.0-release-checklist.md](verification/1.0-release-checklist.md). Hum does not ship 1.0 until every required phase is complete and every stop condition in that checklist passes.

## Document responsibilities

- `docs/ROADMAP.md`: Remaining 1.0 work and dependency order.
- `docs/roadmap/1.0/`: User outcomes and acceptance criteria for each phase.
- `docs/decisions/`: Decisions that are expensive to reverse.
- `docs/superpowers/specs/`: Focused feature and system designs.
- `docs/superpowers/plans/`: Dated implementation plans for one slice.
- `docs/systems/`: Architecture that exists in the current code.
- `docs/verification/1.0-release-checklist.md`: Final pass or stop release gate.
- `docs/CHANGELOG.md`: Completed work by version.
- `BUGS.md`: Known defects outside the active slice.
- `brain/Hum/STATE.md`: Current work cursor, next actions, and blockers.

## Working rule

A phase moves to Ready only after its contract is reviewed. A slice begins with a dated implementation plan and ends with audit, validation, changelog, version bump, commit, BUGS review, and brain updates. A phase moves to Complete only when its contract contains the version, commit, and validation evidence.
