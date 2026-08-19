# HUM-00F audio-output discovery plan

Date: 2026-08-19
Status: Complete in v0.13.59
Related phase: [HUM-00](../../roadmap/1.0/HUM-00-portable-product-core.md)
Progress ledger: [HUM-00 ledger](../../roadmap/1.0/HUM-00-progress-ledger.md)

## Outcome

Hum publishes the available Windows audio outputs and the current default output through a platform-neutral interface. Discovery never changes the user's saved Wired, Speakers, or Bluetooth listening profile.

## Planned production files

- Add `src-tauri/src/audio_output/mod.rs`
- Add `src-tauri/src/audio_output/model.rs`
- Add `src-tauri/src/audio_output/backend.rs`
- Add `src-tauri/src/platform/windows/audio_output_backend.rs`
- Modify `src-tauri/src/platform/windows/mod.rs`
- Modify `src-tauri/src/platform/info.rs`
- Modify `src-tauri/src/lib.rs`
- Modify `src-tauri/Cargo.toml`
- Modify `src/types.ts`

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, `BUGS.md` when needed, this plan, the HUM-00 roadmap and ledger, plus Hum brain records.

## Locked separation

- Audio-output discovery is independent from playback media backends.
- Pandora's WASAPI peak meter remains in the Pandora adapter and is not reused or moved.
- Discovery does not write `listening_mode`, profile delays, or any other setting.
- Discovery does not emit `settings-changed`.
- Settings, tray profile checks, overlay timing, and OBS timing remain unchanged.
- Automatic profile switching belongs to HUM-30, not this slice.

## Neutral contract

`AudioOutputRoute` serializes as `wired`, `speakers`, `bluetooth`, `hdmi`, or `unknown`.

`AudioOutputDevice` contains:

- `id`, the unchanged opaque native endpoint ID
- `display_name`, the friendly endpoint name or `Unknown audio output`
- `route`, the classified output route

`AudioOutputState` contains a normalized output inventory and an optional active output.

Commands:

- `get_audio_outputs` returns the cached complete inventory.
- `get_active_audio_output` returns the cached active device or null.

Events:

- `audio-outputs-changed` carries the complete sorted inventory.
- `active-audio-output-changed` carries the complete active device or null.

## Windows backend behavior

- Start one dedicated standard thread that owns its COM apartment and endpoint enumerator.
- Query immediately, then poll every two seconds through a stop-channel timeout.
- Enumerate active render endpoints with `EnumAudioEndpoints`.
- Resolve the default multimedia render endpoint with `GetDefaultAudioEndpoint`.
- Use the raw `IMMDevice::GetId` value as the stable opaque identity.
- Read the friendly name, enumerator name, and endpoint form factor from the property store.
- Sort inventory by opaque ID before comparing or publishing it.
- Cache the complete state before emitting changed portions.
- Emit nothing for identical consecutive snapshots.
- Store a runtime guard in Tauri managed state. Dropping it wakes and joins the worker thread.
- Initialize COM once on the worker thread and uninitialize it at exit.

## Route policy

Apply these rules in order:

1. Bluetooth text in the enumerator or friendly name maps to `bluetooth`.
2. `DigitalAudioDisplayDevice` maps to `hdmi`.
3. `Speakers` maps to `speakers`.
4. `Headphones`, `Headset`, or `LineLevel` maps to `wired`.
5. Every other form maps to `unknown`.

## Failure behavior

- Skip one endpoint only when its opaque ID cannot be read.
- Keep a device with `Unknown audio output` when its friendly name cannot be read.
- Keep a device with route `unknown` when route properties cannot be read.
- A failed complete poll preserves the last successful cache, emits nothing, and retries after two seconds.
- Log only the first failure in a consecutive failure run, then reset the log guard after recovery.
- An empty inventory is valid and has no active output.

## Implementation steps

1. Add red-first exact wire, normalization, classification, state-transition, failure, lifecycle, capability, and cached-command tests.
2. Add the target-neutral audio-output model, shared cache, backend interface, publisher, and runtime guard.
3. Add the Windows COM polling backend with native reads isolated in the Windows platform module.
4. Add only the required features to the existing `windows` dependency. Add no package dependency.
5. Register the runtime, commands, and exact TypeScript contract.
6. Set both audio-output capability fields true on Windows and leave them false on macOS and Linux.
7. Prove that timing settings and their current tests remain unchanged.
8. Run the full gate: frontend typecheck and build, all Rust tests, all-target Cargo check, Clippy with warnings denied, full Rust formatting, and diff checks.
9. Run an independent review focused on COM ownership, shutdown, stable identity, deduplication, cache-before-event ordering, failures, capability truth, and listening-profile isolation.
10. Audit every changed file, bump to v0.13.59 across all manifests, update changelog and roadmap records, log out-of-scope findings, update the ledger and Hum brain records, commit, and push.

## Required red-first tests

- [x] All five route values and output-device fields preserve their exact wire contract.
- [x] Opaque Windows endpoint IDs round-trip unchanged.
- [x] Inventory normalization sorts by ID and resolves duplicate IDs deterministically.
- [x] Route classification covers Bluetooth precedence, HDMI, speakers, wired form factors, and unknown.
- [x] The cache is updated before listeners observe either event.
- [x] Identical consecutive snapshots emit nothing.
- [x] Inventory-only and active-only changes emit only their matching event.
- [x] Active output removal emits JSON null exactly once.
- [x] A failed sample preserves the last successful state and emits nothing.
- [x] Runtime stop wakes the polling loop without waiting for the full interval.
- [x] Windows reports both audio-output capabilities true while macOS and Linux remain false.
- [x] Commands return managed cached values without invoking native APIs.
- [x] Existing timing tests remain unchanged and green.

## Acceptance checks

- [x] Windows publishes active render endpoints with stable raw endpoint IDs.
- [x] A default multimedia output change arrives within one polling interval.
- [x] Duplicate polls produce no duplicate events.
- [x] PlatformInfo reports discovery and active-output changes true only on Windows.
- [x] Pandora peak-meter behavior remains untouched.
- [x] Listening mode, profile delays, tray checks, overlay timing, and OBS timing remain untouched.
- [x] No new package dependency is added.
- [x] HUM-00-AC9 is complete.
- [x] HUM-00F is committed and pushed before HUM-00G begins.

## Amendment gate

Stop for approval if implementation needs `IMMNotificationClient`, SetupAPI, device topology, per-application routing, automatic listening-profile changes, Settings or tray UI, changes to overlay or OBS timing, a new package dependency, or expansion beyond roughly twice this production file list.
