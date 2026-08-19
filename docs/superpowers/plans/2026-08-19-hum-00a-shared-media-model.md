# HUM-00A shared media model implementation plan

Date: 2026-08-19
Status: Complete in v0.13.54
Related design: [HUM-00 portable product core](../specs/2026-08-19-hum-portable-product-core-design.md)
Related contract: [HUM-00](../../roadmap/1.0/HUM-00-portable-product-core.md)

## Slice outcome

Playback and artwork types belong to a platform-neutral media module. Existing Windows writers and every current consumer keep the same imports, event payloads, locks, and behavior through compatibility re-exports.

## Files

- Create `src-tauri/src/media/mod.rs`
- Create `src-tauri/src/media/model.rs`
- Edit `src-tauri/src/lib.rs`
- Edit `src-tauri/src/smtc.rs`
- Edit `docs/roadmap/1.0/HUM-00-portable-product-core.md`
- Edit `docs/CHANGELOG.md`
- Bump `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`

## Task 1: Add the neutral model

Create `media/model.rs` with the existing definitions unchanged:

- `PlaybackState`
- `CurrentTrack`
- `SharedSnapshot`
- `AlbumArtPayload`
- `SharedAlbumArt`

Keep every field type, serde name, default, and derive. Do not add `Deserialize`, rename `CurrentTrack`, introduce a source enum, or move album art into the track object.

Create `media/mod.rs` and publicly re-export the five model types inside the crate.

Add model tests:

1. `default_track_preserves_wire_contract`
2. `playback_states_preserve_lowercase_wire_values`
3. `album_art_payload_preserves_wire_contract`

The tests assert exact JSON keys and values, including nullable `source_app_id` and `bridge_source`.

## Task 2: Register the shared module

In `lib.rs`:

- Add unconditional `mod media;`.
- Import the five types from `media` for application state and commands.
- Replace the empty non-Windows model shim with a compatibility `smtc` module that re-exports the shared media types.

Do not add a non-Windows media backend. The non-Windows shell still has no playback source after this slice.

## Task 3: Turn SMTC into a producer

In `smtc.rs`:

- Remove the shared type definitions and aliases.
- Re-export the five types from `crate::media` so existing `crate::smtc::*` imports remain valid.
- Keep the Windows conversion from `GlobalSystemMediaTransportControlsSessionPlaybackStatus` to `PlaybackState` inside SMTC.
- Remove imports that became unused.

Do not change `ReadTrack`, source arbitration, event emitters, art fetching, ad classification, or tests outside the new wire-contract coverage.

## Task 4: Validate the slice

Run these commands:

- `pnpm typecheck`
- `pnpm build`
- `rustfmt --check --edition 2021 --config skip_children=true src-tauri/src/lib.rs src-tauri/src/media/mod.rs src-tauri/src/media/model.rs`
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets`
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `git diff --check`

Verify that the original Spotify ad tests, lyric tests, streamer tests, and new media wire tests all pass.

Implementation note: `smtc.rs` contains older formatting that belongs to the repository-wide rustfmt cleanup already listed in `BUGS.md`. Formatting that whole file created unrelated churn, so HUM-00A restored those mechanical changes and kept the SMTC diff limited to the extraction.

## Task 5: Close out

- Re-read every changed file.
- Confirm the diff contains no source-priority, event, timing, or UI change.
- Update HUM-00's slice map. Leave the phase In progress because later slices remain.
- Add a user-readable v0.13.54 changelog entry that describes the portability foundation and confirms no Windows workflow changed.
- Bump every manifest to v0.13.54.
- Review `BUGS.md` for anything discovered outside this slice.
- Commit locally without pushing.
- Update the Hum brain session, STATE, and solutions only if a non-obvious issue was solved.

## Stop conditions

Stop the slice if:

- Any serialized field or playback-state string changes.
- A current consumer needs a behavior rewrite instead of a type-path adjustment.
- Windows source priority or bridge blending changes.
- Event payloads stop being full track objects.
- Album art becomes part of timeline events.
- A non-Windows backend is needed to make the extraction compile.
