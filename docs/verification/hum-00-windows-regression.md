# HUM-00 Windows regression evidence

Last reviewed: 2026-08-19

This record separates automated contract coverage from checks that require a real Windows desktop, player, audio device, or OBS session. A green CI run does not complete the physical matrix.

## Evidence status

The v0.13.60 workflow exposed macOS compile failures and is not native build evidence. HUM-00 must use the checks attached to the final repair commit instead. That commit is not duplicated in this document because any source edit would start a new proof run. Open the final repair commit in GitHub and verify every required check below is green.

Local Windows development checks cover the same Rust and frontend commands, but they do not replace the final GitHub runner proof.

| Workflow job | Runner | What it proves | Current status |
|---|---|---|---|
| `frontend` | Ubuntu 24.04 | Frozen install, TypeScript, production frontend build, platform-info retry | Exact v0.13.61 repair check |
| `windows` | Windows Server 2022 | Formatting, all-target check, Clippy, all Rust tests, named shared-shell smoke | Exact v0.13.61 repair check |
| `portable-native` | Ubuntu 24.04 | Shared native check, library tests, named shared-shell smoke | Exact v0.13.61 repair check |
| `portable-native` | macOS 15 | Shared native check, library tests, named shared-shell smoke | Exact v0.13.61 repair check |

No job launches the GUI, packages an installer, uploads an artifact, or publishes a release.

## Automated contract coverage

All Rust tests below run in the Windows `cargo test --all-targets` step. The portable matrix runs library tests against the shared core.

### Source authority

- `media::backend::tests::playing_smtc_publishes_raw_even_with_authoritative_bridge`
- `media::backend::tests::inactive_smtc_with_authoritative_bridge_is_suppressed`
- `media::backend::tests::inactive_smtc_without_authoritative_bridge_is_blended`
- `media::backend::tests::paused_smtc_is_not_active_authority`
- `media::backend::tests::itunes_publishes_only_when_smtc_is_not_playing`
- `media::backend::tests::bridge_timeline_requires_position`
- `media::backend::tests::bridge_timeline_yields_to_non_empty_playing_smtc`
- `media::backend::tests::bridge_timeline_can_publish_against_non_authoritative_raw_states`

### Event order and payloads

- `media::publisher::tests::publisher_event_names_remain_exact`
- `media::publisher::tests::full_smtc_refresh_publication_order_remains_exact`
- `media::publisher::tests::artwork_is_cached_before_listener_observes_event`
- `media::model::tests::default_track_preserves_wire_contract`
- `media::model::tests::playback_states_preserve_lowercase_wire_values`
- `media::model::tests::album_art_payload_preserves_wire_contract`

### Shared shell startup

- `tests::shared_shell_state_smoke_preserves_neutral_defaults`
- `tests::raw_current_track_read_preserves_the_shared_snapshot`

This smoke uses the same `SharedShellState::new()` constructor as production. It locks the default track, absent artwork, default lyrics, Edit mode, false SMTC active flag, and empty audio-output state.

### Audio-output contracts and lifecycle

- `audio_output::model::tests::routes_and_device_preserve_exact_wire_contract`
- `audio_output::model::tests::opaque_endpoint_ids_round_trip_unchanged`
- `audio_output::model::tests::normalization_sorts_and_resolves_duplicate_ids_deterministically`
- `audio_output::model::tests::empty_inventory_has_no_active_output`
- `audio_output::backend::tests::cache_is_updated_before_both_events`
- `audio_output::backend::tests::event_names_preserve_exact_contract`
- `audio_output::backend::tests::identical_inventory_and_active_output_emit_nothing`
- `audio_output::backend::tests::inventory_only_and_active_only_changes_emit_matching_events`
- `audio_output::backend::tests::active_removal_emits_json_null_once`
- `audio_output::backend::tests::failed_sample_preserves_last_good_cache_and_emits_nothing`
- `audio_output::backend::tests::poll_loop_retries_failures_and_logs_once_per_failure_run`
- `audio_output::backend::tests::runtime_drop_wakes_and_joins_without_waiting_for_poll_interval`
- `audio_output::backend::tests::managed_shutdown_takes_and_drops_the_runtime_once`
- `audio_output::backend::tests::cached_commands_read_state_without_a_native_source`
- `platform::windows::audio_output_backend::tests::bluetooth_text_takes_precedence_over_form_factor`
- `platform::windows::audio_output_backend::tests::form_factor_classification_covers_hdmi_speakers_wired_and_unknown`
- `platform::windows::audio_output_backend::tests::only_an_unreadable_endpoint_id_drops_a_device`

### Timing and OBS projection

- `settings::tests::effective_offset_uses_selected_profile`
- `settings::tests::listening_profiles_default_when_missing`
- `settings::tests::listening_profile_delays_are_clamped`
- `streamer::tests::signed_offset_saturates_at_zero`
- `streamer::tests::obs_backdrop_projection_matches_portable_wire_values`
- `streamer::tests::sse_fingerprint_changes_when_word_timing_changes`
- `streamer::tests::sse_fingerprint_changes_when_same_count_line_text_changes`
- `streamer::tests::sse_fingerprint_changes_when_plain_lyrics_change`
- `streamer::tests::sse_fingerprint_changes_when_translations_change`
- `streamer::tests::sse_fingerprint_changes_when_track_duration_changes`
- `streamer::tests::sse_fingerprint_changes_when_source_app_id_changes`
- `streamer::tests::sse_fingerprint_changes_when_source_label_changes`
- `streamer::tests::sse_fingerprint_ignores_position_interpolation_ticks`
- `streamer::tests::sse_event_identity_changes_for_same_line_seek`
- `streamer::tests::sse_event_identity_ignores_equivalent_natural_position_anchor`
- `streamer::tests::sse_event_identity_accumulates_sub_tolerance_drift_from_published_anchor`
- `streamer::tests::sse_render_event_resets_published_position_anchor`
- `lyrics::tests::parses_yrc_with_source_durations`
- `lyrics::tests::yrc_preserves_token_spacing_and_punctuation`
- `lyrics::tests::valid_yrc_is_preferred_over_netease_line_lyrics`
- `lyrics::tests::netease_picker_requires_exact_normalized_metadata`

### Browser, Pandora, and ad rules

- `web_bridge::tests::pandora_detects_real_chrome_pandora_session`
- `web_bridge::tests::pandora_probe_detects_via_smtc_only`
- `web_bridge::tests::pandora_rejects_non_chrome_apps`
- `web_bridge::tests::pandora_rejects_non_pandora_titles_in_chrome`
- `web_bridge::tests::pandora_does_not_false_positive_on_word_pandora_elsewhere`
- `pandora_desktop::ad_detection_tests::empty_url_set_with_pandora_window_present_is_ad`
- `pandora_desktop::ad_detection_tests::url_set_with_TR_link_is_not_ad`
- `pandora_desktop::ad_detection_tests::countdown_parses_minutes_seconds`
- `youtube_bridge::tests::ad_bullet_text_is_ad`
- `youtube_bridge::tests::skip_ad_text_is_ad`
- `youtube_bridge::tests::sponsored_text_is_ad`
- `youtube_bridge::tests::youtube_does_not_short_circuit_lyrics`

The complete `web_bridge::tests`, `pandora_desktop::tests`, and `youtube_bridge::tests` namespaces also run in the Windows job.

### Native window behavior

- `artist_window::tests::artist_window_transparency_matches_safe_tauri_target_support`
- `window_effects::aspect::tests::current_ratio_is_derived_for_each_adjustment`
- `window_effects::aspect::tests::height_driven_edges_adjust_right`
- `window_effects::aspect::tests::top_corner_edges_adjust_top_from_width`
- `window_effects::aspect::tests::width_driven_bottom_edges_adjust_bottom`
- `window_effects::aspect::tests::zero_sized_current_rectangles_leave_request_unchanged`
- `window_effects::pointer::tests::banner_hit_test_obeys_visibility_and_half_open_boundaries`
- `window_effects::pointer::tests::failed_pointer_lookup_restores_click_through`
- `window_effects::pointer::tests::hidden_banner_restores_click_through_without_pointer_query`
- `window_effects::screen_sampler::tests::sample_placement_preserves_exact_dimensions_gap_and_centering`
- `window_effects::screen_sampler::tests::successful_below_sample_skips_above`
- `window_effects::screen_sampler::tests::failed_below_sample_tries_above_in_order`
- `window_effects::screen_sampler::tests::two_failed_samples_return_error`
- `window_effects::backdrop::tests::wire_values_and_default_remain_compatible`

These tests cover policy and native calculation seams. They do not show a real transparent window on a desktop.

### Platform capabilities and recovery

- `platform::info::tests::windows_payload_is_exact`
- `platform::info::tests::macos_payload_is_exact_without_media_or_updater`
- `platform::info::tests::linux_x11_and_wayland_differ_only_in_shortcut_support`
- `platform::info::tests::linux_does_not_claim_click_through_support`
- `platform::info::tests::audio_output_capabilities_are_windows_only`
- `platform::info::tests::settings_path_uses_the_canonical_store_filename`
- `platform::info::tests::path_resolution_failure_is_returned`
- Node test `platform information recovers after an initial rejected load`

The Node recovery test runs in the `frontend` job. It proves that a rejected `get_platform_info` load retries and can recover, which keeps capability-gated updater and tray behavior from becoming permanently disabled after one startup error. It does not install an update.

### TypeScript and production rendering

The `frontend` job runs:

```text
pnpm install --frozen-lockfile
pnpm typecheck
pnpm build
node --test src/platform-info-retry.test.mjs
```

`pnpm build` runs TypeScript before Vite produces the production bundles for the overlay, Settings, developer console, and artist panel. This is compilation evidence, not a screenshot or interaction test.

## Physical Windows matrix

Every item below is pending. Record the machine, Windows version, app build, player version, output device, OBS version when relevant, result, and evidence link before checking an item.

| Check | Status | Required observation |
|---|---|---|
| Spotify desktop | [ ] Pending | Track, artist, artwork, duration, and live position |
| Chromium player path | [ ] Pending | Supported Chrome and Edge media metadata and timing |
| iTunes desktop | [ ] Pending | iTunes publishes only when SMTC is not playing |
| Pandora web | [ ] Pending | Bridge metadata, timing, pause, and ad transitions |
| Pandora desktop | [ ] Pending | Supported UI Automation metadata, timing, and ad behavior |
| Pause | [ ] Pending | Desktop and OBS cursors freeze at the same lyric |
| Seek | [ ] Pending | Forward and backward seeks land on the correct line and word |
| Track change | [ ] Pending | Metadata, artwork, lyric cache key, and temporary nudge reset |
| Ad transition | [ ] Pending | Provider lookup stops during the ad and resumes for the next song |
| No active session | [ ] Pending | Hum returns to a stable neutral state and recovers on new playback |
| Ribbon layout | [ ] Pending | Active lyric remains readable at supported ribbon sizes |
| Square layout | [ ] Pending | Active lyric position and size remain stable through line changes |
| OBS browser source | [ ] Pending | State, artwork, pause, seek, word timing, and saved delay match desktop |
| Edit mode | [ ] Pending | Drag, resize, controls, and tray state agree |
| Locked mode | [ ] Pending | Overlay stays interactive without accidental movement |
| Ghost mode | [ ] Pending | Click-through works and remains recoverable through tray and shortcut |

The detailed release requirements remain in the [Hum 1.0 release checklist](1.0-release-checklist.md), especially Player and source compatibility, Lyric quality and timing, Overlay and displays, Creator Studio and OBS, and Build and artifact verification.

## Evidence template

Use one record per physical run:

```text
Date:
Commit:
Hum version:
Windows version:
Machine and display setup:
Player and version:
Audio output:
OBS version and canvas, if used:
Checks exercised:
Result:
Evidence link:
Notes:
```
