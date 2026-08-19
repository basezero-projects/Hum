# HUM-00 portable product core design

Date: 2026-08-19
Status: Accepted for implementation
Related phase: [HUM-00](../../roadmap/1.0/HUM-00-portable-product-core.md)
Architecture decision: [ADR-0001](../../decisions/ADR-0001-keep-tauri-and-add-platform-adapters.md)

## Problem

Hum's shared playback model is defined inside the Windows SMTC module. Lyrics, iTunes, browser bridges, the artist panel, OBS, and the frontend all consume that model even though most of those systems do not care how Windows found the track.

The current non-Windows stub defines an empty track. Shared code immediately reads fields that the stub does not have, so the project has no honest macOS or Linux shell build.

More platform assumptions exist around audio outputs, backdrop effects, aspect handling, screen sampling, and Ghost-mode pointer behavior. Moving to Electron would not remove those operating system differences. HUM-00 isolates them while the working Windows product stays on Tauri.

## Goals

- Give shared product code platform-neutral playback and artwork types.
- Preserve the current Windows event and JSON contracts.
- Put media-session writers behind a backend boundary.
- Keep audio-output discovery separate from media playback.
- Put native window behavior behind small testable interfaces.
- Let React render from a platform-capabilities payload.
- Compile and smoke-test the shared shell on Windows, macOS, and Linux.
- Keep every intermediate commit usable on Windows.

## Non-goals

- Shipping a macOS or Linux app during HUM-00
- Changing source priority or playback interpolation
- Adding Linux MPRIS or macOS player adapters
- Automatically switching listening profiles
- Fixing the current aspect-ratio API
- Replacing Windows browser probes
- Redesigning Settings or the overlay

## Contracts that cannot change

Every playback event carries a complete flat track object. React replaces its current object instead of merging a delta.

The event names remain:

- `track-changed`
- `timeline-changed`
- `playback-state-changed`
- `album-art-loaded`

The track payload remains:

```text
title: String
artist: String
album: String
duration_ms: u64
position_ms: u64
last_update_unix_ms: i64
state: unknown | closed | opened | changing | stopped | playing | paused
source_app_id: Option<String>
ad_active: bool
bridge_source: Option<String>
```

Position and duration remain milliseconds. `last_update_unix_ms` remains the observation time used for interpolation. Album artwork stays outside the track object because timeline events are frequent and artwork payloads are large.

The raw shared snapshot and effective blended snapshot remain separate. Browser metadata can enrich a returned or emitted clone without silently replacing the raw SMTC state.

## Target structure

```text
src-tauri/src/
  media/
    mod.rs
    model.rs
    backend.rs
    publisher.rs
  audio_output/
    mod.rs
    backend.rs
    model.rs
  platform/
    windows/
    macos/
    linux/
  window_effects/
    mod.rs
    backdrop.rs
    aspect.rs
    pointer.rs
    screen_sampler.rs
  platform_info.rs
```

The directory appears gradually. HUM-00 does not create empty abstractions before a slice can test them.

## Shared media model

`media/model.rs` owns `PlaybackState`, `CurrentTrack`, `SharedSnapshot`, `AlbumArtPayload`, and `SharedAlbumArt`.

The first slice keeps `CurrentTrack` as the serialized name because changing it would create churn with no product value. A later internal rename to `TrackSnapshot` is allowed only if wire-format tests remain unchanged.

The Windows SMTC module re-exports the shared types during migration. Existing imports keep compiling while later slices move consumers to `crate::media` directly.

## Media backend

The backend boundary owns lifecycle and publication, not lyric lookup.

```text
MediaBackend
  start(context) -> running backend

MediaPublisher
  publish_track(snapshot)
  publish_timeline(snapshot)
  publish_playback(snapshot)
  publish_artwork(payload)
```

The Windows manager initially wraps the existing startup order:

1. SMTC
2. iTunes suppression and fallback
3. Browser and Pandora bridges
4. Lyrics worker

The current authority rules remain in place until pure tests cover them. Active SMTC wins unless an authoritative bridge provides the timeline. iTunes remains suppressed while SMTC is active. Bridge enrichment does not become raw snapshot ownership by accident.

## Audio-output backend

Pandora's WASAPI peak meter is a media-state heuristic. It is not output-device discovery and stays inside the Windows Pandora adapter.

Audio outputs get a separate interface:

```text
AudioOutputBackend
  list_outputs()
  active_output()
  subscribe_active_output_changes()

AudioOutputDevice
  id: opaque String
  display_name: String
  route: wired | speakers | bluetooth | hdmi | unknown
```

HUM-00 publishes stable platform-neutral device records but does not change the selected listening mode. Automatic profile switching belongs to HUM-30.

## Native window boundaries

Small interfaces are preferable to one broad platform object:

- `WindowEffects` applies supported backdrop and aspect behavior.
- `PointerLocator` supplies cursor position for the Ghost-mode update exception.
- A pure hit-test function owns the current banner geometry.
- `ScreenSampler` captures a requested region for auto-contrast.

The first implementations wrap existing Windows code. Behavior such as sampling cadence, banner location, and aspect handling stays unchanged until a later product slice addresses it.

## Platform capabilities

Rust exposes one tested payload:

```text
PlatformInfo
  platform
  media capabilities
  audio-output capabilities
  window capabilities and supported backdrops
  tray, shortcut, autostart, and updater support
  resolved application data and settings paths
```

Capabilities describe what the current build can actually do. Audio-output discovery remains false until its backend exists. Linux global shortcuts remain false on Wayland unless Hum adds a portal-backed implementation.

React uses this payload to hide unsupported controls and to replace Windows-only copy. It does not infer support from `navigator.platform` or duplicate Rust conditions.

## Cross-platform build proof

After the compile blockers are removed, native CI runners perform:

- Windows: all-target check, clippy, full Rust tests
- Linux: all-target check and platform-neutral library tests
- macOS: all-target check and platform-neutral library tests
- Frontend: frozen install, typecheck, and production build

These jobs prove that the shared shell compiles. They do not claim runtime player support or publish non-Windows artifacts.

## Slice order

### HUM-00A: Shared media model

Move the five shared types into `media/model.rs`, keep compatibility re-exports, and add exact JSON contract tests. No runtime behavior changes.

### HUM-00B: Build boundary

Target-gate Windows-only dependencies, repair shared-code cfg usage, provide an unsupported stub for the UI Automation diagnostic tool, and make non-Windows all-target checks honest.

### HUM-00C: Media backend and publisher

Wrap current Windows startup and event publication behind interfaces. Add pure authority tests before moving source arbitration.

### HUM-00D: Native window interfaces

Wrap backdrop, aspect behavior, Ghost pointer handling, and screen sampling. Preserve current Windows geometry and polling.

### HUM-00E: Platform information and React

Return capabilities and resolved paths from Rust. Update Settings and copy to use the payload.

### HUM-00F: Audio-output backend

Add Windows endpoint discovery and active-output events without changing the user's selected listening profile.

### HUM-00G: CI and system documentation

Add native compile and smoke jobs, then document the implemented event flow and platform boundaries.

## Failure policy

If a slice changes source priority, event payloads, timing math, or visible Windows behavior, stop and split that change into its own reviewed slice. Compatibility shims are preferred over a repository-wide import rewrite when they keep the current slice mechanical.
