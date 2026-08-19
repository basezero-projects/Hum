# HUM-40: Premium presentation and interaction

Status: Proposed
Target release: 0.18.x
Depends on: HUM-20, HUM-30
Blocks: HUM-60, HUM-70
Last reviewed: 2026-08-19

## Outcome

Hum looks finished immediately, stays easy to read, and lets the user control lyrics without hunting through Settings.

## Why this phase exists

Hum has deep visual settings, but a long form is not a design system. Strong presets, stable reading positions, direct interaction, and display-aware placement turn those options into a coherent product.

## In scope

- Apple Sheet, Focus Card, Desktop Ribbon, Minimal Capsule, Karaoke, Stream Lower Third, and Stream Full Sheet visual presets
- Named custom presets with duplicate, reset, import, and export
- Direct lyric seeking where the player supports it
- Free scrolling with a return-to-current control
- Progress and supported playback controls
- Monitor selection, centering, snapping, safe margins, and remembered placement
- Consistent empty, instrumental, loading, error, and unsupported states
- Motion and typography polish across every layout

## Out of scope

- A general-purpose theme marketplace
- Social sharing
- Mobile layouts
- 3D or video backgrounds

## Architecture constraints

- Presets reference the existing settings schema rather than forking renderers.
- The active lyric position and configured font size remain stable during playback.
- Unsupported playback controls stay hidden.
- Placement is stored per display, shape, and preset using stable monitor identity where possible.
- Native effects remain optional. Every preset needs a polished solid or CSS fallback.

## Acceptance criteria

- [ ] HUM-40-AC1: Every built-in preset has a documented purpose and passes at the exact ribbon and square sizes in the required test matrix.
- [ ] HUM-40-AC2: A custom preset can be saved, duplicated, exported, imported, and restored to defaults.
- [ ] HUM-40-AC3: Clicking a lyric seeks when the active media backend reports seek support.
- [ ] HUM-40-AC4: Manual scrolling reveals a clear return-to-current control without stopping playback tracking.
- [ ] HUM-40-AC5: Hum remembers placement across monitor, scaling, shape, and preset changes without opening offscreen.
- [ ] HUM-40-AC6: Current lyrics do not jump or resize when nearby content changes.
- [ ] HUM-40-AC7: Reduced motion removes nonessential movement without removing timing feedback.
- [ ] HUM-40-AC8: Every playback and provider state has an intentional visual treatment and recovery action.

## Required test matrix

- Ribbon presets at 360, 700, 1100, and 1800 logical pixels wide
- Square presets at 480, 620, and 900 logical pixels per side
- 100, 125, 150, and 200 percent scaling
- Single monitor and mixed-DPI multi-monitor setups
- Long lines, translations, right-to-left text, and missing artwork
- Edit, Locked, Ghost, fullscreen app, and task-switch behavior
- Reduced motion and high-contrast Windows settings

## Slice map

No implementation plan written yet.

## Completion record

Version:
Commit:
Validation:
Changelog:
Known deferrals:
