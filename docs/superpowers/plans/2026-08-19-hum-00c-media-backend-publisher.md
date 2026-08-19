# HUM-00C media backend and publisher plan

Date: 2026-08-19
Status: Complete in v0.13.56
Related phase: [HUM-00](../../roadmap/1.0/HUM-00-portable-product-core.md)
Progress ledger: [HUM-00 ledger](../../roadmap/1.0/HUM-00-progress-ledger.md)

## Outcome

Hum starts Windows playback through one backend boundary and publishes the four media event families through one tested publisher. Source authority, raw snapshot ownership, event order, timing, and visible Windows behavior remain unchanged.

## Planned implementation files

New architecture files:

- `src-tauri/src/media/backend.rs`
- `src-tauri/src/media/publisher.rs`
- `src-tauri/src/platform/mod.rs`
- `src-tauri/src/platform/windows/mod.rs`
- `src-tauri/src/platform/windows/media_backend.rs`

Behavior-preserving wiring:

- `src-tauri/src/media/mod.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/smtc.rs`
- `src-tauri/src/itunes.rs`
- `src-tauri/src/web_bridge.rs`

Shared import cleanup:

- `src-tauri/src/lyrics.rs`
- `src-tauri/src/streamer.rs`
- `src-tauri/src/artist_window.rs`
- `src-tauri/src/pandora_desktop.rs`

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, `BUGS.md` when needed, this plan, the HUM-00 roadmap and ledger, plus Hum brain records.

## Locked contracts

- Startup order remains SMTC, iTunes, browser and Pandora bridge, then lyrics.
- Every media event carries the same complete payload and keeps its current name.
- A full SMTC refresh publishes track, timeline, then playback.
- iTunes disappearance publishes track, then playback.
- A normal iTunes poll publishes track conditionally, timeline unconditionally, then playback conditionally.
- Artwork is cached before `album-art-loaded` is emitted.
- Actively playing SMTC remains authoritative.
- An authoritative bridge can suppress inactive SMTC publication.
- Fresh bridge data enriches only an emitted or returned clone. It does not become raw snapshot ownership.
- Only `ad_active` is synchronized from blended state into the raw snapshot.
- iTunes is suppressed only by the existing `smtc_playing` atomic.
- Streamer continues reading the raw snapshot.
- `web-bridge-updated` remains a direct auxiliary event and is not added to the media publisher.
- Existing freshness windows, polling cadence, debounce, and retry delays do not change.

## Implementation steps

1. Add red-first pure policy tests for SMTC authority, iTunes suppression, bridge timeline publication, event names, and ordered publication.
2. Add a platform-neutral `MediaBackend` lifecycle contract and `MediaBackendContext` carrying the existing shared handles without Windows types.
3. Add a cloneable `MediaPublisher` with `publish_track`, `publish_timeline`, `publish_playback`, and `publish_artwork`. Snapshot publication emits only the supplied clone. Artwork preserves cache-before-emit behavior.
4. Add `WindowsMediaBackend`, which starts the existing workers in their current order and gives each source a clone of the shared publisher.
5. Route media-family publication in SMTC, iTunes, and the bridge through the publisher. Keep raw writers, conditionals, and direct `web-bridge-updated` publication unchanged.
6. Move model-only imports from `crate::smtc` to `crate::media`, then remove the non-Windows SMTC type re-export shim when no shared consumer needs it.
7. Run the full gate: frontend typecheck and build, all Rust tests, all-target Cargo check, Clippy with warnings denied, full Rust formatting, and diff checks.
8. Run an independent review focused on source authority, raw ownership, event names, ordering, and platform neutrality.
9. Audit every changed file, bump to v0.13.56 across all manifests, update the changelog and roadmap, record out-of-scope findings, update the ledger and Hum brain records, commit, and push.

## Required red-first tests

- [x] Playing SMTC publishes raw even when a bridge reports authority.
- [x] Inactive SMTC with authoritative bridge position suppresses SMTC publication.
- [x] Inactive SMTC with a non-authoritative bridge blends before publication.
- [x] Paused SMTC is not active authority.
- [x] iTunes publishes only while `smtc_playing` is false.
- [x] Bridge timeline publication requires a bridge position.
- [x] Bridge timeline yields to a non-empty playing SMTC snapshot.
- [x] Bridge timeline can publish against paused, stopped, empty, or stale raw state.
- [x] Publisher event names remain exact.
- [x] Ordered publication remains exact for the full SMTC refresh.

## Acceptance checks

- [x] `lib.rs` makes one Windows media-backend start call.
- [x] SMTC, iTunes, and bridge media-family events use `MediaPublisher`.
- [x] No media publisher writes a supplied effective snapshot into raw shared state.
- [x] Lyrics and streamer no longer import model types through `crate::smtc`.
- [x] No dependency is added.
- [x] HUM-00-AC2 and HUM-00-AC3 are complete.
- [x] HUM-00C is committed and pushed before HUM-00D begins.

## Amendment gate

Stop for approval if implementation needs a channel-based event bus, async trait objects, shutdown handles, a generalized backend registry, file moves for existing Windows sources, publisher ownership of raw snapshots, changes to streamer input, or any timing and authority rule not named above.
