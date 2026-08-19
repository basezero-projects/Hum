# ADR-0001: Keep Tauri and add platform adapters

Status: Accepted
Date: 2026-08-19
Related phases: HUM-00, HUM-30, HUM-40, HUM-60, HUM-70
Supersedes:
Superseded by:

## Context

Hum is a Tauri 2 desktop app with a React interface and a Rust core. The current release is Windows-only, but future macOS and Linux versions should reuse the product instead of becoming separate rewrites.

Most of Hum is already portable. The React overlay, lyric provider logic, word timing, settings model, caches, artist data, network services, and local OBS server do not depend on Windows. The main Windows dependencies are now-playing capture, iTunes COM, browser UI Automation, WASAPI playback detection, native backdrop effects, and aspect-ratio handling.

Tauri supports Windows, macOS, and Linux packaging, tray menus, autostart, signed updates, transparent windows, always-on-top behavior, and click-through windows. It uses WebView2 on Windows, WKWebView on macOS, and WebKitGTK on Linux, so rendering still needs platform testing. See the official [distribution guide](https://v2.tauri.app/distribute/), [system tray guide](https://v2.tauri.app/learn/system-tray/), [autostart plugin](https://v2.tauri.app/plugin/autostart/), [updater plugin](https://v2.tauri.app/plugin/updater/), and [WebView version reference](https://v2.tauri.app/reference/webview-versions/).

The difficult portability problem exists below the framework. Windows exposes other apps' media sessions through [Windows.Media.Control](https://learn.microsoft.com/en-us/uwp/api/windows.media.control). Linux has the public [MPRIS Player interface](https://specifications.freedesktop.org/mpris/latest/Player_Interface.html). Apple's public [MPNowPlayingInfoCenter](https://developer.apple.com/documentation/mediaplayer/mpnowplayinginfocenter) describes publishing information for media an app plays, not reading the active session from any other app. Public macOS coverage therefore requires per-player adapters such as [Scripting Bridge](https://developer.apple.com/documentation/scriptingbridge), a browser bridge, or a narrower support promise.

## Decision

Hum will remain on Tauri 2.

Before another operating system is supported, Hum will separate shared product state from platform capture and window effects:

```text
media/model.rs
  TrackSnapshot
  PlaybackState
  AlbumArtPayload
  MediaCapabilities

media/backend.rs
  MediaBackend
  BackendEvent
  BackendManager

audio_output/backend.rs
  AudioOutputBackend
  AudioOutputDevice
  ActiveOutputChanged

platform/windows/
  SMTC
  iTunes COM
  UI Automation probes
  WASAPI fallback

platform/macos/
  Public player adapters
  Browser bridge
  Native window effects and permissions

platform/linux/
  MPRIS
  Desktop capability handling

window_effects/
  Surface effects
  Aspect behavior
  Screen sampling
```

React will receive one platform-capabilities object from Rust. Settings will show only supported effects and controls. Shared features will consume `TrackSnapshot` and media capabilities without importing Windows modules.

Audio-output discovery will use a separate platform interface. Windows uses its native audio endpoint APIs, macOS can later use Core Audio, and Linux can use PipeWire or PulseAudio. Shared timing settings store stable profile records without importing platform device types.

Windows 1.0 remains the immediate release. HUM-00 creates the boundary but does not add macOS or Linux builds.

## Consequences

- Existing Windows behavior and most of the codebase remain intact.
- New media integrations implement a defined backend instead of leaking source-specific fields into lyrics and UI code.
- Linux can use MPRIS without changing the lyric engine or React interface.
- macOS support must publish an honest supported-player list unless a stable public system-wide option appears.
- Platform visual effects become optional capabilities. Linux cannot promise Windows Mica or macOS vibrancy.
- Transparent macOS windows require Tauri's `macos-private-api` feature under current Tauri guidance. That prevents Mac App Store distribution, so direct signed and notarized distribution is the default assumption.
- Tauri's current Linux global-shortcut dependency supports X11, not Wayland. Hum must disable those shortcuts on Wayland unless a portal-backed implementation is added. Tray behavior, positioning, and click-through still require compositor testing.
- The app needs WebView visual regression tests for karaoke masks, font metrics, blur, transparency, and high-DPI scaling.
- Windows-only crates must stay in target-specific Cargo dependency tables, and shared models cannot contain Windows types.
- CI must compile and smoke-test the shared shell on Windows, macOS, and Linux after HUM-00. Shipping installers for macOS and Linux remains out of scope for 1.0.

## Alternatives considered

### Electron

Electron would keep React and provide one bundled Chromium renderer. It would not provide a portable API for other apps' media sessions. Hum would still need Windows, macOS, and Linux media adapters while also replacing working Tauri commands, Rust integration, tray behavior, updater configuration, and packaging. The larger runtime and migration cost do not solve Hum's actual portability constraint.

### Separate native applications

WinUI or WPF could provide tighter Windows integration. AppKit or SwiftUI and GTK would then require separate interfaces and product implementations. This gives the least reuse and the highest chance that platforms drift apart.

### Stay Windows-only indefinitely

This avoids near-term abstraction work but lets Windows assumptions spread further into shared code. The cost of a later port would keep increasing.

## Verification

This decision remains sound when:

- HUM-00 preserves current Windows behavior through a shared snapshot and backend interface.
- CI compiles the shared shell on Windows, macOS, and Linux without platform types leaking into shared modules.
- The frontend passes smoke and visual regression tests in WebView2, WKWebView, and WebKitGTK.
- A Linux MPRIS proof can drive the existing overlay and OBS server without changes to lyric resolution.
- Platform-specific settings come from capabilities instead of direct OS checks in React.

Review this ADR again if Tauri blocks a required overlay behavior after a reproducible platform test, or if macOS gains a documented public API for reading the active media session across third-party apps.
