# Hum — Ad-break detection + SYVR cross-promo overlay

- **Date:** 2026-05-22
- **Author:** Claude Opus 4.7 (1M context), with Wes
- **Status:** Awaiting Wes's review
- **Project:** Hum (`D:\Work\App_Projects\All_Projects\lyric-overlay\`)
- **Slice:** v0.12.0 — minor bump (this is a substantial new feature surface, not a patch)
- **Predecessor session:** `docs/summaries/2026-05-22_1555_hum-right-side-metadata-column.md` (right-side metadata column, v0.11.10 min-size fix)

---

## Goal

Detect ad breaks on Spotify, YouTube, and Pandora (web + desktop). When an ad break is detected, replace Hum's lyric area with a rotating SYVR Studios product promo card — turning the otherwise-dead-air ad break into a free distribution channel for SYVR's product ecosystem. The metadata column on the right keeps showing position / duration so the user can see how long the ad has left.

The promo content lives in a remote JSON hot-swappable without a Hum release, so Wes can rotate products / change copy / toggle entries without shipping new builds.

## Non-goals

- **Ad blocking.** Hum does not skip, mute, or modify ads. It only detects them and changes its own display.
- **Detecting ads from sources Hum doesn't already track.** This slice covers Spotify (SMTC), Pandora web (existing bridge), Pandora desktop (existing bridge), YouTube (new web-bridge probe). Tidal / Amazon Music / Deezer ad detection is a future slice.
- **Click telemetry / analytics.** No tracking pixel, no UTM injection by Hum. The `url` field in the JSON can carry UTM params if Wes wants them; Hum just opens the URL.
- **User-supplied promos (premium feature).** Architected for in Phase 2 (see bottom of spec). Not built in this slice.
- **A paywall / premium gating system in Hum.** Phase 2 will need this; not in this slice.
- **Per-ad-duration estimation for SMTC sources where the snapshot doesn't include a real duration.** Spotify's SMTC reports real ad durations. If a hypothetical future source doesn't, the bar shows whatever duration it has.
- **iOS / macOS / Linux.** Hum is Windows-only.

## Why this matters

Hum is free with no monetization mechanism today. Spotify's free tier alone is several billion users; each hears a 15-30s ad every few songs. While that ad plays, Hum currently displays a `♪ no lyrics on LRCLib` status line or stale lyrics — reads as broken. Turning that into a brand-consistent SYVR sponsor moment is essentially free distribution for the wider SYVR ecosystem (Trellis, Stub, Arcanum, SimSweep, Loomwerks, etc.).

Phase 2 (premium): streamers can replace SYVR's promos with their own — their Twitch handle, Patreon, merch store, etc. This turns Hum from "free overlay that occasionally shows lyrics" into "real-time stream lower-third that promotes your stuff during music breaks." That's the actual monetization hook for Hum.

## Architecture

### High-level flow

```
   ┌──────────────────────────────────────────┐
   │  SMTC + bridges (Spotify/Pandora/YT)     │
   │    ad heuristics per source              │
   └──────────────────┬───────────────────────┘
                      │ ad_active: bool on snapshot
                      │ position_ms + duration_ms (ad timing)
                      ▼
   ┌──────────────────────────────────────────┐
   │  CurrentTrack snapshot                    │
   │    new field: ad_active                   │
   └──────────────────┬───────────────────────┘
                      │
                      ▼
   ┌──────────────────────────────────────────┐
   │  lyrics_worker — short-circuits on        │
   │    ad_active=true; emits CurrentLyrics    │
   │    with status="ad"                       │
   └──────────────────┬───────────────────────┘
                      │
                      ▼
   ┌──────────────────────────────────────────┐
   │  Overlay (React)                          │
   │    when status="ad" → render PromoCard    │
   │    in place of lyric stack;                │
   │    metadata column stays;                  │
   │    source badge → "AD BREAK" chip          │
   └──────────────────────────────────────────┘

   ┌──────────────────────────────────────────┐
   │  Promo rotation engine (Rust)             │
   │    promos.json fetched on startup +       │
   │    every 6h; cached on disk;              │
   │    bundled fallback in app resources      │
   │                                           │
   │    pick_next_promo() — weighted random    │
   │    with last_shown_id cooldown            │
   └──────────────────────────────────────────┘
```

### New types

```rust
// src-tauri/src/types.rs (or wherever CurrentTrack lives)
pub struct CurrentTrack {
    // ... existing fields ...
    pub ad_active: bool,
}

// src-tauri/src/promos.rs (new module)
#[derive(Deserialize, Clone)]
pub struct Promo {
    pub id: String,
    pub product_name: String,
    pub tagline: String,
    pub url: String,
    pub icon_url: Option<String>,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default = "default_active")]
    pub active: bool,
    pub cta_text: Option<String>,        // default: "Learn more →"
    pub accent_color: Option<String>,    // default: "#d4af37"
}

#[derive(Deserialize)]
pub struct PromosFile {
    pub version: u32,    // 1 for this slice; bump for breaking schema changes
    pub promos: Vec<Promo>,
}
```

The `CurrentLyrics.status` union (in `src/types.ts`) gains an `"ad"` variant; the existing `"unsupported"` / `"not_found"` / `"error"` paths are unchanged.

### Per-source detection

#### Spotify (SMTC heuristics — no bridge changes)

A pure helper in `smtc.rs`:

```rust
fn is_spotify_ad(track: &SmtcSessionState) -> bool {
    let title = track.title.trim();
    let artist = track.artist.trim();
    let src = track.source_app_id.as_deref().unwrap_or("");

    let is_spotify_source =
        src.to_lowercase().contains("spotify");

    if !is_spotify_source { return false; }

    // Heuristic A: explicit "Advertisement" or "Spotify" in title.
    if title.eq_ignore_ascii_case("Advertisement") { return true; }
    if title.eq_ignore_ascii_case("Spotify") { return true; }
    if artist.eq_ignore_ascii_case("Spotify") && !title.is_empty() { return true; }

    // Heuristic B: empty title with playing state (rare but happens).
    if title.is_empty() && artist.is_empty() && track.state == Playing {
        return true;
    }

    false
}
```

Called in `emit_blended` before the snapshot ships. When `is_spotify_ad` returns true, set `snapshot.ad_active = true`. Position + duration come from SMTC as normal (Spotify's ads report real values).

#### Pandora web (`web_bridge::PandoraProbe` extension)

`PandoraProbe::read()` currently DFS-walks Chrome's UIA tree for `/artist/.../TR{id}` Hyperlinks. Extend to:

1. Continue the DFS. If at least one `/TR` URL was found → normal track, return `PandoraTrack { is_ad: false, ... }`.
2. If zero `/TR` URLs found but the Pandora tab was confirmed present (a `pandora.com` URL or distinctive page Name node was matched) → ad state.
3. In ad state, additionally walk the tree for the ad-countdown widget. Pandora's ad UI exposes a countdown like `"0:23"` near the player. Find the text node matching `/^\d+:\d{2}$/` inside the player region and parse it. Surface as `position_ms` (computed from total duration minus remaining countdown) + `duration_ms` (initial countdown at first detection, cached per ad-track-key).
4. Return `PandoraTrack { is_ad: true, position_ms, duration_ms, ... }`.

The 3-tier blend logic in `web_bridge::blend_bridge_into_snapshot` translates `is_ad` to `snapshot.ad_active = true`.

#### Pandora desktop (`pandora_desktop::PandoraDesktopProbe` extension)

Same shape as Pandora web. The desktop probe is already DFS-walking the control-view subtree via `element_from_handle` re-anchor. Extend the same way:

1. No `/TR` Hyperlink → ad state.
2. Walk for the countdown text node.
3. Surface `position_ms` + `duration_ms` from the countdown + cached initial.
4. Return `is_ad: true` on the returned track struct.

WASAPI peak-meter for play/pause (v0.11.7) continues to work in ad state — paused ad = paused bar, same as paused song.

#### YouTube (new `web_bridge::YouTubeProbe`)

YouTube currently flows through SMTC via Chrome. There's no Hum-side YouTube bridge today. New probe in `web_bridge.rs`:

1. Reuses the existing `enum_chrome_tabs` helper.
2. Matches tabs whose URL contains `youtube.com/watch`.
3. Re-anchors via `element_from_handle` on the matched tab's HWND (same wake-Chromium pattern).
4. DFS-walks for ad-marker UIA elements:
   - Text node containing `"Sponsored"`, `"Ad ·"`, `"Advertisement"`.
   - Or an Element whose Name starts with `"Skip Ad"` / `"Skip in"`.
   - Or a Text node matching `/^\d+:\d{2} \/ \d+:\d{2}/` paired with a sibling `"Ad"` Name node — that's YouTube's ad timer format.
5. If found → `ad_active = true` on the snapshot. Walk for the ad-timer text to parse `position_ms` + `duration_ms`. Fallback to `duration_ms = 30_000`, `position_ms` interpolated from first-detection wall time if the timer can't be read.
6. If not found → leave the snapshot untouched (YouTube non-ad metadata continues through SMTC as today; Hum does NOT need YouTube non-ad metadata from the bridge).

This is the most expensive part of the slice. The YouTube probe runs even when no Pandora tab is open, so its baseline cost when nothing is playing on YouTube needs to be light — gate it on `enum_chrome_tabs` already returning a `youtube.com/watch` URL before doing any UIA tree walk.

### State propagation: `ad_active` boolean on `CurrentTrack`

One field on the existing snapshot. No new event, no separate state channel.

- `smtc::emit_blended` is the single chokepoint. It calls `is_spotify_ad` then `blend_bridge_into_snapshot`. The bridge's `is_ad` field, if set, wins (3-tier priority from v0.11.6 still applies).
- Frontend's `applyTrack` already handles every `track-changed` / `timeline-changed` / `playback-state-changed` event. It now also reads `ad_active`.
- `lyrics_worker` on a snapshot with `ad_active = true`:
  - Skips the LRCLib fetch entirely (don't waste API calls fetching lyrics for "Advertisement").
  - Emits a `CurrentLyrics` with `status: "ad"`, `source: null`, `line_count: 0`, `lines: []`, `plain: null`, `translation: null`, and `track: { title: "", artist: "", album: "", duration_ms: 0 }` (the type requires `track` but the frontend reads `status` not `track` when deciding to render the promo card, so the values don't matter).
  - Caches nothing — the next snapshot (real track) re-resolves normally.
  - The progress bar in the metadata column still reads from `CurrentTrack.position_ms` / `duration_ms`, NOT from `CurrentLyrics.track`, so it shows ad timing correctly regardless.

### End-of-ad transition

No explicit "ad ended" signal needed. When the source's next read returns a real track:

- Spotify: SMTC reports a real title (the next song). `is_spotify_ad` returns false. `ad_active = false`. Lyrics worker re-runs and fetches the new track's lyrics.
- Pandora (both): bridge finds `/TR` URLs again. `is_ad = false` on `PandoraTrack`. Same cascade.
- YouTube: ad markers no longer present in UIA tree. `ad_active = false` on the snapshot (bridge skips setting it). Lyrics worker re-resolves from SMTC's normal YouTube metadata.

End-of-ad cadence = whatever cadence the source already emits at:
- SMTC: instant on Spotify's session state change.
- Bridges: every 2 s on the existing worker tick.

## Promo content source

### Hosting

`https://syvrstudios.com/hum/promos.json` — drop the file at `Websites/sites/syvr-site/public/hum/promos.json` (verify the static-asset directory matches the site's framework — most Next.js / Astro / SvelteKit sites use `public/`; the implementation step verifies before placing the file). `git push` to syvr-site → Vercel auto-deploys → live URL.

### Schema

```json
{
  "version": 1,
  "promos": [
    {
      "id": "trellis",
      "product_name": "Trellis",
      "tagline": "Guided AI creative platform.",
      "url": "https://trellis.syvr.dev",
      "icon_url": "https://syvrstudios.com/hum/icons/trellis.png",
      "weight": 1,
      "active": true,
      "cta_text": "Try free",
      "accent_color": "#d4af37"
    },
    {
      "id": "stub",
      "product_name": "Stub",
      "tagline": "Track your business finances without spreadsheet hell.",
      "url": "https://app.syvrstudios.com",
      "icon_url": "https://syvrstudios.com/hum/icons/stub.png",
      "weight": 1,
      "active": true
    }
  ]
}
```

Required: `id`, `product_name`, `tagline`, `url`. Optional with defaults:
- `icon_url` (null → generic SYVR mark from bundled resources)
- `weight` (default 1)
- `active` (default true)
- `cta_text` (default `"Learn more →"`)
- `accent_color` (default `#d4af37`)

### Fetch strategy

- App startup: spawn a tokio task that fetches `promos.json` via `reqwest`. 5s timeout. On success, write the parsed file to `%APPDATA%\com.syvr.hum\promos.json` and load into in-memory rotation pool.
- Background refresh: every 6 hours.
- Manual refresh: on first ad detection of a session, if last fetch was > 1 hour ago, trigger a refresh in the background (don't block).

### Fallback chain

1. **In-memory pool from last successful fetch** — primary.
2. **Disk cache** (`%APPDATA%\com.syvr.hum\promos.json`) — read on startup before any network call so the first ad break of a session always has something.
3. **Bundled default** (`src-tauri/resources/default_promos.json`) — included in the Tauri bundle, ships with each release. Used only when the disk cache is missing or corrupt.
4. **Hardcoded ultimate fallback** — if even the bundled file fails to parse (shouldn't happen), construct a single in-memory `Promo` with id=`syvr`, product_name=`SYVR Studios`, tagline=`Tools for creators and makers.`, url=`https://syvrstudios.com`. Never let the user see a "promo failed" state.

## Visual treatment

### What the overlay looks like during an ad break

Layout (3-line layout shown):

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ┌──────┐  Brought to you by SYVR Studios                                 │
│ │album │                                            (artist line hidden) │
│ │ art  │  ── Trellis ──                            2:14 / 0:30           │
│ │stays │  Guided AI creative platform.             ▓▓▓▓▓▓░░░░░░          │
│ └──────┘  Learn more →                             [ AD BREAK ]          │
└──────────────────────────────────────────────────────────────────────────┘
```

- **Album art (left):** stays from the most recent real track. Reads as "still listening to your music, just an ad break right now."
- **Promo card (replaces lyric stack):**
  - Supertitle row: `Brought to you by SYVR Studios` — 10px dim text, top of card.
  - Icon: 32×32 product icon if `icon_url` provided. Rounded corners. Fades in with the card.
  - Product name: same font size as the current lyric line (`settingsForRender.font_size_px`), full text color.
  - Tagline: ~60% of product-name size, dim text color.
  - CTA: ~70% of product-name size, `accent_color` text, underline on hover.
  - Whole card is clickable in locked/ghost modes. Drag-region in edit mode.
- **Right metadata column:**
  - Artist · Song · Album line: **hidden** during ads.
  - Progress bar + time: stays, shows ad position / duration.
  - Source badge: swapped to amber `[ AD BREAK ]` chip (same chip shape as the other source labels, gold accent: `rgba(212, 175, 55, 0.85)` border + lighter fill).

### Transitions

- Ad-active flip true → lyric rows fade out (200ms) → promo card fades in (220ms). Reuse the existing `hum-line-in` keyframe.
- Ad-active flip false → promo card fades out → lyric rows fade back in.

### Single-line layout

Card collapses to one row:

```
[icon]  Trellis · Guided AI creative platform. · Learn more →
```

Metadata column behavior unchanged (artist line still hidden, bar + AD chip showing).

### Full-page layout

Replaces the scrolling lyric column with a centered version of the 3-row promo card (icon, name, tagline, CTA). AlbumArtBadge in the corner stays. ArtistInfoDot stays.

## Rotation, cooldown, click behavior

### Picking a promo

```rust
fn pick_next_promo(pool: &[Promo], last_shown_id: Option<&str>) -> Option<&Promo> {
    let candidates: Vec<&Promo> = pool.iter()
        .filter(|p| p.active)
        .filter(|p| last_shown_id.map_or(true, |id| p.id != id))
        .collect();

    if candidates.is_empty() {
        // Fall back to ignoring the cooldown (e.g. only 1 active promo in pool).
        return pool.iter().find(|p| p.active);
    }

    let total_weight: u32 = candidates.iter().map(|p| p.weight.max(1)).sum();
    let mut roll = thread_rng().gen_range(0..total_weight);
    for p in &candidates {
        let w = p.weight.max(1);
        if roll < w { return Some(p); }
        roll -= w;
    }
    candidates.first().copied()
}
```

- Weighted-random with last-shown cooldown.
- Called once per ad break, anchored to the snapshot's `track_key`. While `ad_active` stays true on the same track key, the same promo stays on screen.

### Click behavior

- `tauri-plugin-opener` opens `promo.url` in the user's default browser. Add to `Cargo.toml` (sibling of `tauri-plugin-process`).
- No tracking / telemetry by Hum. UTM params live in the JSON if Wes wants them.
- Card has `data-tauri-drag-region` in edit mode (drag wins, click handler skipped); no drag region in locked/ghost mode (click reaches handler).

## Phase 2: User-supplied promos (premium)

**Not built in this slice.** Architecture today supports it tomorrow.

### Pluggable sources

Refactor the promo rotation pool to come from a `Vec<Box<dyn PromoSource>>` instead of a single `Vec<Promo>`:

```rust
pub trait PromoSource: Send + Sync {
    fn name(&self) -> &str;
    fn promos(&self) -> Vec<Promo>;
}
```

Today: one source, `SyvrRemoteSource` (the `promos.json` fetcher).
Tomorrow (Phase 2): add `UserLocalSource` (reads from Hum's settings store), and a mode toggle on the SyvrRemoteSource that disables it for premium users in "replace" mode.

### Premium gating

Phase 2 introduces a `premium: bool` flag on the user's account (mechanism TBD — license-key file, OAuth-backed entitlement check, etc.). When `premium == true`, a new Settings section appears: "Custom ad-break promos" with an add/edit/delete UI for `Promo` entries. The runtime logic doesn't gate; only the UI does.

### Modes (Phase 2)

- **Replace SYVR** — only the user's promos rotate. Streamer-friendly default.
- **Mix with SYVR** — combined pool, weighted random across both sources.

Choice exposed as a radio button in the Settings section.

## Settings

One new toggle in the existing Settings → Overlay section:

- `ad_break_promos_enabled: bool` (default `true`)
  - When off, ad breaks render a neutral "Ad break" indicator with no promo card and no clickable area. The metadata column still shows position / duration / AD chip.

That's it. No other user-facing config in Phase 1.

## Testing

### Rust unit tests

- `is_spotify_ad` — table-driven test across title / artist / source_app_id combinations.
- `PandoraProbe` ad-state detection — synthetic UIA-tree fixtures (already exist for the non-ad path).
- `PandoraDesktopProbe` ad-state detection — same.
- `YouTubeProbe` ad-marker matching — synthetic fixtures.
- `pick_next_promo` — verify weighted distribution, cooldown enforcement, single-active-promo fallback.
- `PromosFile` deserialization — schema-version forward compatibility (`version: 999` should not crash, just log + skip).

### Manual verification matrix

| Source | Verify |
|--------|--------|
| Spotify free tier | Play music; on ad, overlay swaps to promo card + AD chip; bar counts down through ad; on next song, lyrics resume |
| Pandora web (free) | Same |
| Pandora desktop (free) | Same |
| YouTube web | Same; verify YouTube non-ad metadata still flows through SMTC unchanged |
| Spotify premium | No ads — verify Hum never enters ad state, no regression in normal lyrics behavior |
| Network offline | First launch: bundled fallback promo shows. Subsequent launches: disk cache shows. |

## Out of scope (deferred follow-ups)

- **YouTube position estimation when the ad-timer DOM element can't be read.** Fallback to 30s estimated duration + wall-clock interpolation is acceptable for v0.12.
- **Tidal / Amazon Music / Deezer ad detection.** Future per-source slices.
- **Settings UI for user-supplied promos.** Phase 2.
- **Click telemetry / analytics.** Wes can bake UTM params into URLs in `promos.json` if he wants source attribution.
- **Promo impression frequency cap.** Not relevant for an MVP — the source ad cadence determines this.
- **Localized promo content (multi-language taglines).** Future if Hum gets a translation layer.
- **Per-region promos** (e.g. show region-specific Stub copy).
- **A/B testing of taglines** via the JSON. Could be added by having multiple `Promo` entries with the same product but different `tagline` + a stable `group_id` for analytics; not in this slice.

## Implementation order

A writing-plans skill invocation will break this into ordered steps. Rough mental shape:

1. **`ad_active` boolean** + lyrics-worker short-circuit + `"ad"` status variant. No detection yet — manually fake it for end-to-end testing.
2. **Promo rotation engine** + `default_promos.json` + remote fetch + disk cache. Hardcode `ad_active = true` temporarily to manually verify the visual.
3. **Promo card UI in Overlay.tsx** — replace lyric stack when status="ad", AD BREAK chip, hide artist line.
4. **Spotify detection** (`is_spotify_ad`) — wire into `emit_blended`.
5. **Pandora desktop detection** — extend `pandora_desktop.rs` for ad state + countdown reading.
6. **Pandora web detection** — extend `web_bridge::PandoraProbe`.
7. **YouTube detection** — new `web_bridge::YouTubeProbe`.
8. **Settings toggle** for `ad_break_promos_enabled`.
9. **Documentation** — CHANGELOG entry, README update for the new feature.

Each step ships as its own commit, version bump, CHANGELOG entry per Hum's existing rules.

## Sign-off

Spec written 2026-05-22 by Claude Opus 4.7 with Wes during the brainstorming session immediately following the v0.11.10 min-size fix.
