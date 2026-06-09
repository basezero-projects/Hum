# Smart Music-Video Sync — Design

_Date: 2026-06-09 · Author: Claude Opus 4.8 · Project: Hum (lyric overlay, Tauri 2)_

## Problem

Official music videos pad the song with a cold-open intro (dialogue, scene-setting, ambient
sound) before the track starts, and often an outro after. LRCLib's synced lyrics are timed to
the **studio recording**, which starts the song at 0:00. So when Hum plays a music video whose
song begins at, say, 0:25, the lyrics run ~25s **ahead** of the actual vocals for the whole song.

Nothing in the SMTC/LRCLib metadata tells us the intro length. The video duration includes
intro **and** outro, so a duration mismatch reveals *that* there is padding, not *where* the
song starts. Audio-onset detection is only reliable when the intro is quiet (dialogue/ambient)
and the song bursts in; loud or ambiguous intros can't be located confidently.

## Principle

**Per-video memory is the reliable backbone; conservative auto-onset is a best-effort bonus.**
Never make currently-good sync worse — when unsure, do nothing and let the user nudge.

## Existing building blocks (reused, not rebuilt)

- **Manual nudge** — `Ctrl+Alt+[` / `Ctrl+Alt+]` emit `lyric-offset-nudge` (±250ms). The frontend
  (`Overlay.tsx`) keeps `nudgeMsRef` and applies it as `effectivePos = pos + anticipate_ms - nudge`.
  Currently **resets to 0 on every track change** and is **not persisted**.
- **WASAPI peak meter** — `pandora_desktop.rs::is_audio_session_silent(pid)` reads a session's
  `IAudioMeterInformation` peak vs a silence floor. (Caveat: per-process; Chrome plays audio from
  a renderer/audio-service child process, not the window PID — see Auto-onset risks.)
- **LRCLib record duration** — available at lyric-resolution time (used today by `pick_best`).
- **`tauri-plugin-store`** — already hosts settings at `%APPDATA%\com.syvr.hum\`.

## Component 1 — Per-video offset memory (backbone, slice 1)

**Store.** A durable map `videoKey → offset_ms`, persisted via `tauri-plugin-store` in its own
file (`offsets.json`) next to settings. Capped LRU (~300 entries, evict oldest) so it never
grows unbounded.

**Key.** `rawArtist | rawTitle | durationSecs` using the **raw SMTC fields** (not the normalized
ones) plus duration. Rationale: the offset is specific to **one upload**, not the song — different
videos of the same song have different intros. Raw title/channel + duration identify a single
upload and avoid collapsing distinct videos. (`|`-join matches the existing `cache_key` shape;
a literal `|` inside a field is acceptable risk here — a wrong offset, never a crash — but we
trim and lowercase for stability.)

**Save.** When the user nudges the current track, debounce ~600ms, then persist the resulting
offset under the current track's key. Nudging is the save action — no separate "save" gesture.

**Load.** On track change, derive the key and look it up. If found, **pre-load** that offset into
`nudgeMsRef` instead of resetting to 0. Result: sync a video once → it stays synced on replays.

**Ownership.** This is frontend-driven (the nudge lives in `Overlay.tsx`), with the store accessed
through small Tauri commands (`get_offset(key) -> Option<i32>`, `set_offset(key, ms)`), keeping
the persistence + LRU logic in Rust where it's unit-testable.

## Component 2 — Conservative auto-onset (best-effort, slice 2)

**When it attempts.** Only when (a) there is **no remembered offset** for the video, AND (b) the
track looks padded: title matches `/(official\s+)?(music\s+)?video|\(\s*video\s*\)/i` **or** the
playing duration exceeds the matched LRCLib song duration by more than ~15s.

**Detection.** At track start (position ≈ 0, state Playing), sample the audio peak every ~200ms
for up to ~8s. Classify the series:
- **Sustained near-silence (below the silence floor) for ≥ ~1.5s, then a clear sustained rise**
  (above a music threshold for ≥ ~1s) at time `T` ⇒ set `offset = T`.
- **Loud from the start, or no clean silence→onset transition** ⇒ do nothing (leave sync as-is).

**Priority.** remembered (user-confirmed) > auto-detected > 0. A manual nudge always overrides and
re-saves under the video key (promoting an auto guess to a remembered value).

**Auto-onset risks (honest scope).**
- Chrome audio is on a child process, so per-PID metering is unreliable. The detector reads the
  **default render endpoint** peak (`IAudioMeterInformation` on the endpoint), which captures all
  system audio and can be fooled by other apps. The "do nothing unless a clean silence→onset" rule
  plus the conservative thresholds keep this safe (worst case: no auto-offset, user nudges).
- Fires only for quiet/dialogue intros. Loud intros get no auto-offset by design — the user nudges
  once and Component 1 remembers it.

## Priority resolution (single source of truth)

On track change the effective starting offset is chosen as:
`remembered_offset ?? auto_detected_offset ?? 0`. Manual nudges mutate the live value and, on
debounce, write back as the new remembered value.

## UX

- When a non-zero offset is active, the existing nudge banner shows it, tagged by origin:
  `music-video sync +0:23 (remembered)` / `(auto)` / `(manual)`. Reuses the current banner styling.
- One settings toggle: **Smart music-video sync** (default **on**) — a kill-switch that disables
  both auto-onset and pre-loading remembered offsets (manual nudge still works).

## Testing

- **Offset store (Rust, unit):** save/load round-trip, LRU eviction at the cap, key derivation
  (raw fields + duration, trim/lowercase), priority resolution helper.
- **Onset classifier (Rust, unit):** pure function over a synthetic peak series →
  silence-then-onset = `Some(T)`; loud-from-start = `None`; ambiguous/no-transition = `None`.
- **Manual:** a known cold-open music video (lyrics start offset), confirm auto or one-nudge-then-
  remembered brings vocals into line and persists across a replay + app restart.

## Out of scope (YAGNI)

- Outro handling (lyrics are already done by then — irrelevant).
- ASR / audio-fingerprint alignment (heavy, unnecessary for the value).
- Cloud-shared offset database across users.
- Per-video offset for non-music sources (Spotify/iTunes don't have video intros).

## Build order

1. **Slice 1 — Per-video offset memory.** Reliable, high value alone; makes the existing nudge
   permanent per video.
2. **Slice 2 — Conservative auto-onset.** Layered on top; the silence→onset detector + the
   "looks padded" gate, feeding the same priority resolver.
