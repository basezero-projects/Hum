# Media and timing

Last verified against v0.13.60 source on 2026-08-19.

## Purpose and boundary

This system turns native playback observations into the track, artwork, lyrics, timing, and audio-output state used by Hum's desktop overlay and local OBS browser source.

The shared Rust models and policies compile on Windows, macOS, and Linux. Windows is the only platform with a playback adapter and audio-output discovery today. Native CI proves compilation and non-GUI state initialization. It does not prove that a player, transparent window, tray, shortcut, or OBS session worked on physical hardware.

## Key files

| Area | Files |
|---|---|
| Shared startup | `src-tauri/src/lib.rs` |
| Media contracts and policy | `src-tauri/src/media/model.rs`, `backend.rs`, `publisher.rs` |
| Windows media sources | `src-tauri/src/platform/windows/media_backend.rs`, `smtc.rs`, `itunes.rs`, `web_bridge.rs`, `youtube_bridge.rs`, `pandora_desktop.rs` |
| Lyrics | `src-tauri/src/lyrics.rs` |
| Saved timing | `src-tauri/src/settings.rs` |
| Desktop timing | `src/Overlay.tsx` |
| Audio outputs | `src-tauri/src/audio_output/`, `src-tauri/src/platform/windows/audio_output_backend.rs` |
| Native windows and capabilities | `src-tauri/src/window_effects/`, `src-tauri/src/platform/info.rs` |
| OBS | `src-tauri/src/streamer.rs`, `src-tauri/src/streamer_overlay.html` |

## Startup and ownership

`SharedShellState::new()` creates six neutral values in the same order used by production:

1. Default `CurrentTrack`
2. Empty album-art cache
3. Default `CurrentLyrics`
4. SMTC active flag set to false
5. Overlay mode set to Edit
6. Empty audio-output inventory with no active output

`run()` moves the snapshot, artwork, lyrics, mode, and audio-output values into Tauri managed state before setup begins. The `smtc_active` flag stays captured by setup and is passed to the Windows media backend. Setup then loads saved settings, reconciles autostart, creates the artist cache, and starts the platform media path.

On Windows, `WindowsMediaBackend` starts SMTC, iTunes, the browser and Pandora bridge, and the lyric resolver. The audio-output backend starts a separate standard thread with its own COM apartment and endpoint enumerator. Its runtime guard lives in managed state. On `RunEvent::Exit`, Hum takes and drops that guard, wakes the worker, joins it, and lets the worker call `CoUninitialize` before Tauri cleanup or restart.

On macOS and Linux, the lyric resolver starts against the neutral snapshot, but there is no native playback source. This keeps shared startup testable without implying media support that does not exist.

After the source layer is running, setup starts promo refresh, Windows contrast sampling, the optional OBS supervisor, tray state, backdrop and aspect behavior, the saved interaction mode, shortcuts, and window event handlers. This order matters because saved settings must exist before the tray, OBS projection, and native window state are applied.

## Raw and effective track snapshots

`CurrentTrack` is the shared wire model. It contains title, artist, album, duration, position, the timestamp of the last position update, playback state, source application ID, ad state, and an optional bridge source.

Native source workers own the raw snapshot. Consumers receive an effective clone:

- A nonempty, playing SMTC session is authoritative and publishes without browser enrichment.
- A fresh bridge with position data can suppress an inactive SMTC observation and publish its own timeline.
- If neither path is authoritative, Hum may blend fresh bridge metadata into the SMTC clone before publication.
- `get_current_track` also blends the latest bridge data into its returned clone.

This separation keeps the source-owned state stable while allowing Pandora and browser metadata to correct incomplete Windows session data.

## Windows source authority

SMTC is the normal Windows source. A playing session with a nonempty title wins over browser estimates. Paused, stopped, unknown, stale, or titleless SMTC state can yield to an authoritative bridge.

iTunes publishes only while the shared SMTC playing flag is false. That rule prevents classic iTunes polling from replacing an active SMTC player.

The browser and Pandora bridge can provide cleaned metadata, position, duration, playback state, and ad state. A bridge timeline publishes only when it has a position and the raw SMTC snapshot is not a nonempty playing session. Bridge data expires, so a closed or stalled probe cannot hold authority forever.

Spotify ad detection belongs to the SMTC path. Browser, Pandora, and YouTube ad state belongs to the corresponding bridge. Both paths write the effective ad flag back to shared state because the lyric resolver reads that state before deciding whether to make provider requests.

## Media and lyric events

The three media events carry the complete `CurrentTrack` payload, not partial patches.

| Event or command | Payload or result | Behavior |
|---|---|---|
| `track-changed` | `CurrentTrack` | Metadata or active-session change |
| `timeline-changed` | `CurrentTrack` | Position or timeline change |
| `playback-state-changed` | `CurrentTrack` | Play, pause, stop, or related state change |
| `get_current_track` | `CurrentTrack` | Current effective snapshot |
| `album-art-loaded` | `{ title, artist, data_url }` | Artwork is cached before the event fires |
| `get_current_album_art` | Artwork or `null` | Replays cached artwork to a late subscriber |
| `lyrics-state` | `CurrentLyrics` | Current resolver state |
| `lyrics-loaded` | `CurrentLyrics` | Synced, plain, instrumental, or ad result |
| `lyrics-not-found` | `CurrentLyrics` | Unsupported, not found, or provider error result |
| `get_current_lyrics` | `CurrentLyrics` | Current cached lyric state |

A full SMTC refresh publishes in this exact order: `track-changed`, `timeline-changed`, then `playback-state-changed`.

## Lyrics providers and cache flow

The resolver wakes at startup and on `track-changed`, `timeline-changed`, and Windows `web-bridge-updated` events. It reads the freshest shared snapshot, applies bridge metadata when appropriate, and deduplicates repeated work by track key.

The lookup order is:

1. A 256-entry in-memory LRU cache
2. The persistent `lyrics-cache.json` store
3. LRCLib and NetEase requests started together

NetEase has a 2.5 second bound. It wins only when strict normalized title, artist, and duration matching produces valid YRC word timing. LRCLib supplies the normal synchronized or plain result. A NetEase line-timed result is used when LRCLib has no usable result.

Successful lyric results are cached. Authoritative misses, unsupported results, and provider failures are not retained, so later metadata cleanup or network recovery gets another chance. Cache keys are versioned so word-timing changes do not reuse older line-only records.

When `ad_active` is true, the resolver skips provider requests and emits the ad state. The overlay can then show the configured promo treatment instead of searching for lyrics for an advertisement.

## Timing equations

While playback is active, both desktop and OBS interpolate from the last native update:

```text
interpolated_position_ms = position_ms + max(0, now_ms - last_update_unix_ms)
```

Paused and stopped snapshots use `position_ms` without wall-clock interpolation.

Saved timing is shared by desktop and OBS:

```text
saved_offset_ms = anticipate_ms - selected_profile_delay_ms
saved_lookup_ms = max(0, interpolated_position_ms + saved_offset_ms)
```

The selected delay comes from the saved Wired, Speakers, or Bluetooth profile. Positive anticipation shows lyrics earlier. A positive output delay moves lookup backward because the listener hears that audio after the player reports it.

The desktop adds a temporary per-track nudge:

```text
desktop_lookup_ms = max(
  0,
  interpolated_position_ms
    + anticipate_ms
    - selected_profile_delay_ms
    - temporary_nudge_ms
)
```

`Ctrl+Alt+[` and `Ctrl+Alt+]` change the temporary value in 250 ms steps. It resets when the track changes. OBS does not receive that session-only value, so its lookup uses the saved equation.

The desktop accepts small forward timeline drift inside `jitter_tolerance_ms` instead of snapping backward. Larger changes, seeks, and track transitions replace the interpolated position with the new native observation.

## Audio-output discovery

`AudioOutputDevice` exposes the unchanged native endpoint ID, a friendly name or `Unknown audio output`, and one route: `wired`, `speakers`, `bluetooth`, `hdmi`, or `unknown`.

Windows polls active render endpoints and the default multimedia endpoint immediately, then every two seconds. Classification checks Bluetooth text and native `BTHENUM` or `BTHHFENUM` identifiers before endpoint form factor. Remaining form factors map display audio to HDMI, speakers to Speakers, headphones, headset, and line level to Wired, and everything else to Unknown.

Commands read managed cache only:

- `get_audio_outputs`
- `get_active_audio_output`

Changed state is published through:

- `audio-outputs-changed`
- `active-audio-output-changed`

The complete cache is written before either event. Inventory is sorted by opaque ID and duplicate IDs are resolved deterministically. Identical polls emit nothing. Active removal emits `null` once.

An unreadable endpoint ID drops only that endpoint. Missing friendly or route properties retain the device with fallback values. A failed complete poll preserves the last good cache and emits nothing. Hum logs the first error in a consecutive failure run, retries after two seconds, and resets the log guard after recovery.

Discovery does not change `listening_mode` or any saved profile delay. Automatic profile selection and per-device delay storage are later roadmap work.

## Native windows and PlatformInfo

Platform-neutral traits isolate backdrop application, aspect behavior, cursor lookup, and screen sampling from their Windows implementations. Shared modules do not expose HWND, COM, WinRT, WASAPI, or other native types.

`get_platform_info` reports capabilities and resolves both the application data directory and canonical `settings.json` path. The frontend uses those capability fields before enabling native behavior.

| Capability | Windows | macOS | Linux |
|---|---|---|---|
| Playback adapter | Yes | No | No |
| Audio-output discovery | Yes | No | No |
| Native backdrops | Acrylic, Mica, Tabbed Mica, None | None | None |
| Aspect lock | Yes | No | No |
| Click-through | Yes | Yes | No |
| Update-banner pointer exception | Yes | No | No |
| Screen sampling | Yes | No | No |
| Tray | Yes | Yes | Yes |
| Global shortcuts | Yes | Yes | X11 only |
| Autostart | Yes | Yes | Yes |
| Updater | Yes | No | No |

The macOS and Linux entries describe current capability declarations and compile boundaries. HUM-00 does not package those platforms or claim that their desktop UX has passed a physical test.

## OBS state flow

The optional Axum server binds to `127.0.0.1` and accepts only loopback Host headers. Its main routes are:

- `/overlay` and `/` for the browser-source page
- `/state` for track, lyrics, cursor, artwork key, source label, and timing state
- `/settings` for the subset of appearance and timing settings used by OBS
- `/events` for server-sent state updates
- `/art` for current artwork bytes
- `/hum-logo.png` and `/logos/{slug}` for local assets
- `/healthz` for liveness

The server reads the same managed track, lyrics, artwork, and settings state as the desktop. It computes the cursor with the saved timing equation. `/settings` includes `anticipate_ms`, `effective_offset_ms`, `listening_mode`, and the selected profile delay.

The SSE stream samples shared state every 100 ms and emits only when its event identity changes. Its render fingerprint includes every serialized lyrics field; track title, artist, album, duration, playback state, source application ID, bridge source, and ad state; plus cursor, artwork key, source label, anticipation, and effective offset. The fingerprint intentionally excludes `position_ms`, `last_update_unix_ms`, and `server_now_ms` because the client interpolates those ticks locally.

Each connection also keeps a position revision and the position anchor from the last state event actually sent to that client. For every polled native anchor, the server projects that published position to the new `last_update_unix_ms`. Observations within `jitter_tolerance_ms` do not replace the published baseline, so repeated small deviations accumulate. Once the reported `position_ms` differs from the projection by more than the tolerance, the revision changes and SSE emits even when a seek stays within the same lyric line. Any emitted SSE state, including a lyric, track, or playback change, replaces the published position baseline. An equivalent naturally advancing anchor within tolerance leaves the revision unchanged, so normal position ticks do not create SSE churn. The revision is internal event identity only and does not add or change response wire fields. A 15 second keepalive maintains quiet OBS connections without creating duplicate state events.

Changing the enabled setting or port goes through `StreamerSupervisor`, which stops the old server before replacing it. Dropping a server handle also signals graceful shutdown.

## Retry, cache, and shutdown behavior

- Frontend platform information retries one second after a failed command while the owning component remains mounted.
- Artwork is cached before notification, which closes the late-listener startup race.
- Lyrics keep successful memory and disk entries, but transient failures remain retryable.
- Audio-output polling keeps the last successful state through complete poll failures.
- The audio-output worker receives an explicit stop signal and is joined on application exit.
- OBS uses graceful shutdown when disabled, rebound, or dropped.
- Capability checks keep unsupported native work out of macOS and Linux frontend paths.

## Known limitations

- macOS and Linux have no playback or audio-output adapter.
- Native CI does not launch a transparent window or exercise hardware players.
- Windows audio-output changes arrive by polling, with a maximum normal delay of one polling interval.
- Pandora desktop and other UI Automation sources can estimate timing when the player does not publish a native timeline.
- Temporary per-track timing nudges affect the desktop overlay only.
- Physical compatibility, layout, mode, and OBS checks remain pending until recorded on Windows hardware.

## Verification

- [HUM-00 Windows regression evidence](../verification/hum-00-windows-regression.md)
- [Hum 1.0 release checklist](../verification/1.0-release-checklist.md)
- [Portable core workflow](../../.github/workflows/portable-core.yml)
- `tests::shared_shell_state_smoke_preserves_neutral_defaults`
