# Ad-Break Detection + SYVR Cross-Promo Overlay — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Detect Spotify / Pandora web / Pandora desktop / YouTube ad breaks; replace the lyric area with a rotating SYVR Studios product promo card; keep the metadata column visible so users see ad time remaining; source badge swaps to amber AD BREAK chip.

**Architecture:** One boolean `ad_active` flag on the existing `CurrentTrack` snapshot, set by per-source detectors (`is_spotify_ad`, bridge probes' `is_ad`), propagated through `lyrics_worker` which short-circuits to `Status::Ad`. Frontend renders a `PromoCard` instead of lyric rows when `status === "ad"`. Promos come from a remote JSON (`https://syvrstudios.com/hum/promos.json`) with disk cache + bundled fallback chain. Weighted-random rotation with last-shown cooldown. Phase 2 (user-supplied promos / premium) deferred but architected for via a `PromoSource` trait.

**Tech Stack:** Rust 2021, Tauri 2, React 19, `windows` crate (UIA), `reqwest` (already in deps), `rand` (new — for rotation), `tauri-plugin-opener` (new — for click-through URLs), `serde` (existing). LRC parsing untouched.

**Spec reference:** `docs/superpowers/specs/2026-05-22-hum-ad-break-detection-design.md`

---

## File Structure Overview

**New files:**
- `src-tauri/src/promos.rs` — Promo struct, PromosFile schema, PromoSource trait, SyvrRemoteSource (fetch + disk cache), pick_next_promo rotation logic
- `src-tauri/resources/default_promos.json` — bundled fallback promos
- `Websites/sites/syvr-site/public/hum/promos.json` — production promos hosted at https://syvrstudios.com/hum/promos.json (placed in syvr-site repo, deployed on git push)

**Modified Rust files:**
- `src-tauri/src/smtc.rs` — add `ad_active: bool` to `CurrentTrack`; add `is_spotify_ad`; set `ad_active` in `emit_blended` after blend
- `src-tauri/src/web_bridge.rs` — add `is_ad: bool` + `ad_duration_ms: Option<u64>` to `WebBridgeTrack`; extend `blend_bridge_into_snapshot` to map them onto the snapshot; extend `PandoraProbe::read()` for ad detection + countdown reading
- `src-tauri/src/pandora_desktop.rs` — extend `PandoraDesktopProbe::read()` for ad detection + countdown reading
- `src-tauri/src/lyrics.rs` — add `Status::Ad` variant; short-circuit lyrics resolution when `snap.ad_active`; emit `CurrentLyrics { status: Ad, .. }`
- `src-tauri/src/lib.rs` — register `promos` module, init the rotation engine on startup, register the new Tauri commands (`get_active_promo`, `open_promo_url`)
- `src-tauri/src/settings.rs` — add `ad_break_promos_enabled: bool` field (default true)
- `src-tauri/Cargo.toml` — add `rand`, `tauri-plugin-opener`
- `src-tauri/tauri.conf.json` — list `opener` plugin if needed
- `src-tauri/capabilities/default.json` — grant `opener:default` permission

**New Rust file (YouTube probe — could go in web_bridge.rs as a sibling but separate file keeps web_bridge focused):**
- `src-tauri/src/youtube_bridge.rs` — `YouTubeProbe` struct, ad-marker detection, ad-timer parsing

**Modified TS files:**
- `src/types.ts` — add `ad_active: boolean` to `CurrentTrack`; add `"ad"` to `LyricsStatus` union; add `Promo` type
- `src/Overlay.tsx` — render `PromoCard` when `lyrics.status === "ad"`; hide artist line during ads; swap source badge to AD BREAK chip; new `PromoCard` component
- `src/Settings.tsx` — checkbox for `ad_break_promos_enabled`

**Docs:**
- `docs/CHANGELOG.md` — entry per commit per Hum's CHANGELOG rules

---

## Task 1: Add `ad_active` to CurrentTrack + `Status::Ad` variant + frontend types

This is plumbing only. After this task, nothing detects ads — but the data path is in place end-to-end.

**Files:**
- Modify: `src-tauri/src/smtc.rs:59-73` (CurrentTrack struct)
- Modify: `src-tauri/src/lyrics.rs:102-114` (Status enum)
- Modify: `src/types.ts:1-17` (CurrentTrack), `src/types.ts:84-92` (LyricsStatus)
- Modify: `src/Overlay.tsx` (consume `lyrics.status === "ad"` placeholder — temporary "ad break" text; full PromoCard comes in Task 5)
- Modify: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` (version bump)
- Modify: `docs/CHANGELOG.md`

- [ ] **Step 1: Add `ad_active` field to Rust `CurrentTrack`**

Edit `src-tauri/src/smtc.rs` around line 73. Replace the struct definition:

```rust
#[derive(Clone, Serialize, Debug, Default)]
pub struct CurrentTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub position_ms: u64,
    /// Unix epoch ms when SMTC last reported the position. The frontend uses
    /// `position_ms + (now - last_update_unix_ms)` to interpolate while playing.
    pub last_update_unix_ms: i64,
    pub state: PlaybackState,
    /// e.g. "Spotify.exe", "308046B0AF4A39CB" (Firefox AUMID), etc. Useful for
    /// debugging / future per-source behavior.
    pub source_app_id: Option<String>,
    /// True when the current source is playing an ad break (Spotify's
    /// "Advertisement" track, Pandora's ad interlude, YouTube's ad rolls).
    /// Drives the overlay to render the SYVR promo card in place of lyrics.
    #[serde(default)]
    pub ad_active: bool,
}
```

- [ ] **Step 2: Add `Status::Ad` variant**

Edit `src-tauri/src/lyrics.rs` around line 114. Add `Ad` variant:

```rust
#[derive(Clone, Copy, Debug, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    #[default]
    Idle,
    Fetching,
    Synced,
    Plain,
    Instrumental,
    NotFound,
    Unsupported,
    Error,
    /// Source is playing an ad break — overlay swaps to the SYVR promo card.
    Ad,
}
```

- [ ] **Step 3: Run `cargo check`**

```bash
cd src-tauri && cargo check
```

Expected: clean build. The new field defaults to `false`; the new variant is unused (no match exhaustiveness issues because all existing matches use defaults or `_`).

- [ ] **Step 4: Sync frontend `CurrentTrack` type**

Edit `src/types.ts`. Add `ad_active` to the CurrentTrack type:

```ts
export type CurrentTrack = {
  title: string;
  artist: string;
  album: string;
  duration_ms: number;
  position_ms: number;
  last_update_unix_ms: number;
  state:
    | "unknown"
    | "closed"
    | "opened"
    | "changing"
    | "stopped"
    | "playing"
    | "paused";
  source_app_id: string | null;
  ad_active: boolean;
};
```

- [ ] **Step 5: Sync frontend `LyricsStatus` union**

Edit `src/types.ts` around line 84. Add `"ad"`:

```ts
export type LyricsStatus =
  | "idle"
  | "fetching"
  | "synced"
  | "plain"
  | "instrumental"
  | "not_found"
  | "unsupported"
  | "error"
  | "ad";
```

- [ ] **Step 6: Add temporary `ad` placeholder branch to `statusLine` in Overlay.tsx**

Edit `src/Overlay.tsx`. Find the `statusLine` function (search for `case "fetching"`). Add a new case `"ad"`:

```ts
case "ad":
  return "♪ ad break — promo coming in Task 5";
```

This is a temporary marker so we can verify the data path end-to-end before building the real UI in Task 5. It will be removed when PromoCard lands.

- [ ] **Step 7: Bump version to 0.12.0-rc1**

Edit `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` — change `0.11.10` to `0.12.0-rc1`. The `-rc1` suffix signals work-in-progress on a feature branch; we bump to `0.12.0` proper when all 9 tasks are done.

- [ ] **Step 8: Run TypeScript + Rust checks**

```bash
cd D:/Work/App_Projects/All_Projects/lyric-overlay
pnpm typecheck
cd src-tauri && cargo check
```

Expected: both clean.

- [ ] **Step 9: Add CHANGELOG entry**

Edit `docs/CHANGELOG.md`. Add a new entry above the latest:

```markdown
## [0.12.0-rc1] - 2026-05-22

### Added (internal plumbing — no user-visible behavior yet)
- **`ad_active` flag on the current-track snapshot + `Status::Ad` lyrics variant.** No user-visible behavior in this commit — this is the data-path scaffolding for the ad-break detection feature (spec: `docs/superpowers/specs/2026-05-22-hum-ad-break-detection-design.md`). When `lyrics.status === "ad"`, the overlay currently shows a temporary "♪ ad break — promo coming in Task 5" status line; the real SYVR promo card lands in a follow-up commit.

  **Implementation:** Added `ad_active: bool` (defaults false, serde `#[serde(default)]` for backwards compatibility) to `CurrentTrack` in `src-tauri/src/smtc.rs`. Added `Ad` variant to the `Status` enum in `src-tauri/src/lyrics.rs`. Mirrored both in `src/types.ts`. Frontend `statusLine` function has a placeholder branch for `"ad"`.
```

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/smtc.rs src-tauri/src/lyrics.rs src/types.ts src/Overlay.tsx package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "ad-detect(1/9): add ad_active flag + Status::Ad plumbing"
```

---

## Task 2: Lyrics worker short-circuits when `ad_active`

Wire `ad_active` into the lyrics resolver. When set, emit `Status::Ad` with empty lines and skip all network fetches.

**Files:**
- Modify: `src-tauri/src/lyrics.rs` (in the resolver loop after the snapshot read)
- Modify: `docs/CHANGELOG.md`

- [ ] **Step 1: Write a failing test for the ad short-circuit**

In `src-tauri/src/lyrics.rs`, find the test module at the bottom (if it exists) or add `#[cfg(test)] mod tests { ... }`. Add this test:

```rust
#[cfg(test)]
mod ad_short_circuit_tests {
    use super::*;

    #[test]
    fn ad_active_skips_network_and_emits_ad_status() {
        // Build a snapshot with ad_active = true.
        let mut snap = crate::smtc::CurrentTrack::default();
        snap.title = "Advertisement".into();
        snap.artist = "Spotify".into();
        snap.duration_ms = 30_000;
        snap.ad_active = true;

        // The resolver's short-circuit helper (introduced in this task)
        // should produce a CurrentLyrics with status=Ad and empty lines,
        // without doing any network IO.
        let outcome = ad_break_outcome(&snap);
        assert_eq!(outcome.status, Status::Ad);
        assert!(outcome.lines.is_empty());
        assert_eq!(outcome.line_count, 0);
        assert!(outcome.errors.is_empty());
    }
}
```

- [ ] **Step 2: Run the test (should fail with "function not found")**

```bash
cd src-tauri && cargo test ad_active_skips_network_and_emits_ad_status
```

Expected: FAIL with `cannot find function ad_break_outcome`.

- [ ] **Step 3: Add the `ad_break_outcome` helper**

In `src-tauri/src/lyrics.rs`, somewhere near the other helpers (above the `start` function works), add:

```rust
/// Build the `CurrentLyrics` payload emitted when the current snapshot
/// indicates an ad break is playing. No network IO — purely synthesized
/// from the snapshot. The frontend reads `status == Ad` and renders the
/// SYVR promo card in place of the lyric rows.
fn ad_break_outcome(snap: &crate::smtc::CurrentTrack) -> CurrentLyrics {
    CurrentLyrics {
        track_key: format!("ad|{}|{}", snap.source_app_id.clone().unwrap_or_default(), snap.duration_ms),
        status: Status::Ad,
        source: None,
        line_count: 0,
        lines: Vec::new(),
        plain: None,
        translation: None,
        errors: Vec::new(),
        track: TrackEcho {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            duration_ms: snap.duration_ms,
        },
    }
}
```

- [ ] **Step 4: Run the test (should pass)**

```bash
cd src-tauri && cargo test ad_active_skips_network_and_emits_ad_status
```

Expected: PASS.

- [ ] **Step 5: Wire `ad_active` short-circuit into the resolver loop**

In `src-tauri/src/lyrics.rs`, find the resolver loop in `start()` (around line 160 — the `while rx.recv().await.is_some()` block). Right after `let snap = { snapshot.read().await.clone() };` and BEFORE the bridge-consultation block, add:

```rust
// Ad-break short-circuit. When the source is playing an ad
// (Spotify "Advertisement", Pandora ad interlude, YouTube ad roll),
// skip all network resolution and emit Status::Ad. The overlay
// renders the SYVR promo card instead of lyrics.
if snap.ad_active {
    let outcome = ad_break_outcome(&snap);
    let key = outcome.track_key.clone();
    if key != last_key {
        last_key = key;
        {
            let mut s = shared.write().await;
            *s = outcome.clone();
        }
        let _ = app.emit("lyrics-loaded", &outcome);
    }
    continue;
}
```

- [ ] **Step 6: Run `cargo check`**

```bash
cd src-tauri && cargo check
```

Expected: clean.

- [ ] **Step 7: Manual end-to-end verification via a temporary forced flag**

This is verification, not a test — you can't easily force `ad_active = true` from a unit test against the full resolver loop. Temporarily edit `smtc.rs::emit_blended` to set `snap.ad_active = true` whenever `source_app_id` matches `"Spotify.exe"`:

```rust
// TEMPORARY — remove before committing this task. Forces ad state for
// any Spotify track so we can verify the resolver short-circuits and
// the overlay shows the placeholder status line.
if snap.source_app_id.as_deref().map_or(false, |s| s.to_lowercase().contains("spotify")) {
    snap.ad_active = true;
}
```

Run `pnpm tauri dev`, play a Spotify track, verify the overlay shows `♪ ad break — promo coming in Task 5`. Then revert the temporary edit.

- [ ] **Step 8: Add CHANGELOG entry**

```markdown
## [0.12.0-rc2] - 2026-05-22

### Added (internal plumbing — no user-visible behavior yet)
- **Lyrics resolver short-circuits when ad_active is set.** When the snapshot has `ad_active = true`, the resolver emits a `CurrentLyrics` with `status = Ad`, empty lines, and zero network calls. No detector is wired up yet (that's Task 4+); this is the resolver-side path. Verified manually by forcing `ad_active = true` for Spotify and confirming the overlay's placeholder status line renders.

  **Implementation:** New `ad_break_outcome(snap)` helper in `src-tauri/src/lyrics.rs` that synthesizes the payload. Wired into the resolver loop in `start()` ahead of the bridge consultation. Track-key namespaced as `ad|<app>|<duration>` so consecutive ads on the same source don't dedupe-skip.
```

- [ ] **Step 9: Bump version and commit**

Edit version files: `0.12.0-rc1` → `0.12.0-rc2` in `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.

```bash
git add src-tauri/src/lyrics.rs package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "ad-detect(2/9): lyrics resolver short-circuits on ad_active"
```

---

## Task 3: Promo data types + bundled defaults + rotation logic

Pure-Rust foundation for the rotation engine. No fetch, no UI — just the types and `pick_next_promo`.

**Files:**
- Create: `src-tauri/src/promos.rs`
- Create: `src-tauri/resources/default_promos.json`
- Modify: `src-tauri/src/lib.rs` (register module)
- Modify: `src-tauri/Cargo.toml` (add `rand` dep)
- Modify: `docs/CHANGELOG.md`

- [ ] **Step 1: Add `rand` to Cargo.toml**

Edit `src-tauri/Cargo.toml`. In `[dependencies]`, add:

```toml
rand = "0.8"
```

- [ ] **Step 2: Create the bundled default promos JSON**

Create `src-tauri/resources/default_promos.json` with this content. Keep it minimal — Wes can flesh out the real list in the remote JSON later.

```json
{
  "version": 1,
  "promos": [
    {
      "id": "syvr-studios",
      "product_name": "SYVR Studios",
      "tagline": "Tools and apps from the makers of Hum.",
      "url": "https://syvrstudios.com",
      "weight": 1,
      "active": true
    }
  ]
}
```

If the `resources/` directory doesn't exist yet, create it. Verify it gets bundled into the Tauri app — check `tauri.conf.json` for a `resources` entry in the `bundle` block. If missing, add `"resources": ["resources/*"]` to the bundle config.

- [ ] **Step 3: Write the failing test for `pick_next_promo` rotation logic**

Create `src-tauri/src/promos.rs` with:

```rust
//! Promo rotation engine for ad-break overlays.
//!
//! Loads a list of `Promo` entries from a remote JSON (with disk-cache
//! and bundled-fallback chain), and picks one to show per ad break via
//! weighted-random with a last-shown cooldown.

use serde::Deserialize;

fn default_weight() -> u32 { 1 }
fn default_active() -> bool { true }

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Promo {
    pub id: String,
    pub product_name: String,
    pub tagline: String,
    pub url: String,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default = "default_active")]
    pub active: bool,
    #[serde(default)]
    pub cta_text: Option<String>,
    #[serde(default)]
    pub accent_color: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct PromosFile {
    pub version: u32,
    pub promos: Vec<Promo>,
}

/// Pick one promo from the pool using weighted random with a last-shown
/// cooldown. Returns `None` only when the pool is empty.
///
/// - Inactive entries (`active: false`) are excluded.
/// - Entries with `weight == 0` are treated as weight 1 (prevents
///   accidentally-zero weights from making them un-pickable).
/// - When `last_shown_id` matches an entry, that entry is excluded
///   from the draw — unless excluding it would leave zero candidates,
///   in which case cooldown is ignored.
pub fn pick_next_promo<'a>(pool: &'a [Promo], last_shown_id: Option<&str>) -> Option<&'a Promo> {
    use rand::Rng;
    let active: Vec<&Promo> = pool.iter().filter(|p| p.active).collect();
    if active.is_empty() { return None; }

    let after_cooldown: Vec<&Promo> = active.iter()
        .copied()
        .filter(|p| last_shown_id.map_or(true, |id| p.id != id))
        .collect();

    let candidates: &[&Promo] = if after_cooldown.is_empty() {
        // Cooldown would have removed all candidates (e.g. only one active
        // promo and it was just shown). Ignore cooldown so we still pick
        // something.
        &active[..]
    } else {
        &after_cooldown[..]
    };

    let total_weight: u32 = candidates.iter().map(|p| p.weight.max(1)).sum();
    if total_weight == 0 { return Some(candidates[0]); }

    let mut roll: u32 = rand::thread_rng().gen_range(0..total_weight);
    for p in candidates {
        let w = p.weight.max(1);
        if roll < w { return Some(p); }
        roll -= w;
    }
    candidates.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn promo(id: &str, weight: u32, active: bool) -> Promo {
        Promo {
            id: id.into(),
            product_name: id.into(),
            tagline: "x".into(),
            url: "https://example.com".into(),
            icon_url: None,
            weight,
            active,
            cta_text: None,
            accent_color: None,
        }
    }

    #[test]
    fn empty_pool_returns_none() {
        assert!(pick_next_promo(&[], None).is_none());
    }

    #[test]
    fn all_inactive_returns_none() {
        let pool = vec![promo("a", 1, false), promo("b", 1, false)];
        assert!(pick_next_promo(&pool, None).is_none());
    }

    #[test]
    fn cooldown_excludes_last_shown_when_alternatives_exist() {
        let pool = vec![promo("a", 1, true), promo("b", 1, true)];
        for _ in 0..50 {
            let picked = pick_next_promo(&pool, Some("a")).unwrap();
            assert_eq!(picked.id, "b");
        }
    }

    #[test]
    fn cooldown_ignored_when_only_one_active_promo() {
        let pool = vec![promo("a", 1, true)];
        let picked = pick_next_promo(&pool, Some("a")).unwrap();
        assert_eq!(picked.id, "a");
    }

    #[test]
    fn weight_zero_treated_as_one() {
        let pool = vec![promo("a", 0, true), promo("b", 0, true)];
        // Both weight-0 → both treated as 1 → uniform draw. Just verify
        // we don't crash and pick *something* from the active set.
        let picked = pick_next_promo(&pool, None).unwrap();
        assert!(picked.id == "a" || picked.id == "b");
    }

    #[test]
    fn higher_weight_picked_more_often() {
        let pool = vec![promo("rare", 1, true), promo("common", 9, true)];
        let mut common = 0;
        let mut rare = 0;
        for _ in 0..10_000 {
            let picked = pick_next_promo(&pool, None).unwrap();
            if picked.id == "common" { common += 1; } else { rare += 1; }
        }
        // 90/10 distribution within ±3% slack.
        assert!(common > rare * 7, "common={common} rare={rare}");
    }

    #[test]
    fn bundled_default_parses() {
        let raw = include_str!("../resources/default_promos.json");
        let parsed: PromosFile = serde_json::from_str(raw).expect("default_promos.json must parse");
        assert_eq!(parsed.version, 1);
        assert!(!parsed.promos.is_empty(), "bundled defaults cannot be empty");
        for p in &parsed.promos {
            assert!(!p.id.is_empty(), "every default promo needs an id");
            assert!(p.url.starts_with("https://"), "default promo url must be https: {}", p.url);
        }
    }
}
```

- [ ] **Step 4: Register the module in lib.rs**

Edit `src-tauri/src/lib.rs`. Near the other `mod` declarations (search for `mod smtc;` or similar), add:

```rust
mod promos;
```

- [ ] **Step 5: Run the tests**

```bash
cd src-tauri && cargo test promos::tests
```

Expected: all 7 tests pass.

- [ ] **Step 6: Add CHANGELOG entry**

```markdown
## [0.12.0-rc3] - 2026-05-22

### Added (internal — no user-visible behavior yet)
- **Promo rotation engine + bundled default promos.** New `src-tauri/src/promos.rs` module defines the `Promo` schema (id, product_name, tagline, url, optional icon_url / weight / active / cta_text / accent_color) and the `pick_next_promo` helper. Weighted-random selection with last-shown cooldown that gracefully degrades when only one active promo exists. Bundled `src-tauri/resources/default_promos.json` ships with one SYVR Studios fallback entry. No fetch, no UI integration yet — those land in Tasks 4 and 5.
```

- [ ] **Step 7: Bump version and commit**

```bash
cd src-tauri && cargo check
cd .. && pnpm typecheck
```

Both clean.

Version: `0.12.0-rc2` → `0.12.0-rc3` in three files.

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/promos.rs src-tauri/src/lib.rs src-tauri/resources/default_promos.json src-tauri/tauri.conf.json package.json docs/CHANGELOG.md
git commit -m "ad-detect(3/9): promo data model + rotation logic + bundled defaults"
```

---

## Task 4: Promo source — remote fetch + disk cache + fallback chain

Wire the rotation engine to a real data source.

**Files:**
- Modify: `src-tauri/src/promos.rs` (add `PromoSource` trait, `SyvrRemoteSource`, `init_promo_state`)
- Modify: `src-tauri/src/lib.rs` (spawn the fetch task, expose `get_active_promo` command)
- Modify: `src-tauri/src/lyrics.rs` (when emitting `Status::Ad`, also pick a promo and stash on a shared state — emitted with the lyrics payload OR via a separate `ad-promo-changed` event)
- Modify: `src/types.ts` (Promo type)
- Modify: `src/Overlay.tsx` (listen to ad-promo-changed; render placeholder using the promo data)
- Modify: `docs/CHANGELOG.md`

The decision: emit the picked promo as part of the `CurrentLyrics` payload when status is Ad? Or as a separate event? Cleaner to ride along the lyrics event, since they fire together. Adding a `promo: Option<Promo>` field to `CurrentLyrics` (skipped via serde when None).

- [ ] **Step 1: Extend `CurrentLyrics` to carry an optional Promo**

Edit `src-tauri/src/lyrics.rs`. Find the `CurrentLyrics` struct (around line 76). Add:

```rust
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct CurrentLyrics {
    pub track_key: String,
    pub status: Status,
    pub source: Option<String>,
    pub line_count: usize,
    pub lines: Vec<LyricLine>,
    pub plain: Option<String>,
    pub translation: Option<Vec<LyricLine>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    pub track: TrackEcho,
    /// When `status == Ad`, the rotation-picked promo to display. None
    /// for every other status. Serialized as a sibling of `lines`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub promo: Option<crate::promos::Promo>,
}
```

Note: `Promo` must derive `Serialize`. Edit `src-tauri/src/promos.rs`:

```rust
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Promo {
    ...
}
```

(Add `use serde::Serialize;` to the imports at top of promos.rs alongside `Deserialize`.)

- [ ] **Step 2: Add `PromoSource` trait + `SyvrRemoteSource` implementation**

In `src-tauri/src/promos.rs`, below the existing types, add:

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

const REMOTE_URL: &str = "https://syvrstudios.com/hum/promos.json";
const CACHE_FILE_NAME: &str = "promos.json";
const FETCH_TIMEOUT_SECS: u64 = 5;
const REFRESH_INTERVAL_HOURS: u64 = 6;

/// A source of promos. Phase 2 introduces UserLocalSource alongside this.
pub trait PromoSource: Send + Sync {
    fn name(&self) -> &'static str;
    fn promos(&self) -> Vec<Promo>;
}

/// Fetches from `REMOTE_URL`, falls back to disk cache, falls back to
/// bundled defaults, falls back to a single hardcoded entry. The pool
/// is always non-empty after `bootstrap_load()`.
pub struct SyvrRemoteSource {
    pool: Arc<RwLock<Vec<Promo>>>,
    cache_path: PathBuf,
}

impl SyvrRemoteSource {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            pool: Arc::new(RwLock::new(Vec::new())),
            cache_path: cache_dir.join(CACHE_FILE_NAME),
        }
    }

    /// Synchronous bootstrap: read the disk cache (or bundled fallback)
    /// to populate the pool before the app's first ad break. Network
    /// refresh happens in the background.
    pub fn bootstrap_load(&self) {
        let from_disk = std::fs::read_to_string(&self.cache_path)
            .ok()
            .and_then(|s| serde_json::from_str::<PromosFile>(&s).ok());
        let pool = match from_disk {
            Some(f) if f.version == 1 && !f.promos.is_empty() => f.promos,
            _ => bundled_defaults(),
        };
        // Tokio's blocking sync write: this is called once at startup
        // before the event loop, so a brief blocking lock is fine.
        let pool_arc = self.pool.clone();
        let _guard = tauri::async_runtime::block_on(async move {
            let mut w = pool_arc.write().await;
            *w = pool;
        });
    }

    /// Long-running background task: fetch every REFRESH_INTERVAL_HOURS.
    /// Refreshes the in-memory pool AND writes the cache file on success.
    /// Silent failure on network error — the existing pool stays valid.
    pub async fn run_refresh_loop(self: Arc<Self>) {
        // Initial fetch right at startup (separate from bootstrap_load,
        // which reads from disk synchronously).
        self.refresh_once().await;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            REFRESH_INTERVAL_HOURS * 60 * 60,
        ));
        interval.tick().await; // skip the immediate tick
        loop {
            interval.tick().await;
            self.refresh_once().await;
        }
    }

    async fn refresh_once(&self) {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
            .build()
        {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[promos] http client build failed: {e}");
                return;
            }
        };
        let resp = match client.get(REMOTE_URL).send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[promos] fetch failed: {e}");
                return;
            }
        };
        if !resp.status().is_success() {
            eprintln!("[promos] fetch returned {}", resp.status());
            return;
        }
        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[promos] body read failed: {e}");
                return;
            }
        };
        let parsed: PromosFile = match serde_json::from_str(&body) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[promos] json parse failed: {e}");
                return;
            }
        };
        if parsed.version != 1 || parsed.promos.is_empty() {
            eprintln!("[promos] unexpected schema or empty list — keeping current pool");
            return;
        }
        // Write cache before swapping pool — if cache write fails the
        // in-memory pool still updates (better than the inverse).
        if let Some(parent) = self.cache_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&self.cache_path, &body) {
            eprintln!("[promos] cache write failed: {e}");
        }
        {
            let mut w = self.pool.write().await;
            *w = parsed.promos;
        }
        eprintln!("[promos] refreshed pool from {REMOTE_URL}");
    }
}

impl PromoSource for SyvrRemoteSource {
    fn name(&self) -> &'static str { "syvr-remote" }
    fn promos(&self) -> Vec<Promo> {
        // Block briefly on the async read. The pool is small, contention
        // is negligible, and pick_next_promo is called once per ad break
        // (not in a hot loop).
        tauri::async_runtime::block_on(async {
            self.pool.read().await.clone()
        })
    }
}

fn bundled_defaults() -> Vec<Promo> {
    const RAW: &str = include_str!("../resources/default_promos.json");
    serde_json::from_str::<PromosFile>(RAW)
        .map(|f| f.promos)
        .unwrap_or_else(|_| vec![Promo {
            id: "syvr-studios".into(),
            product_name: "SYVR Studios".into(),
            tagline: "Tools and apps from the makers of Hum.".into(),
            url: "https://syvrstudios.com".into(),
            icon_url: None,
            weight: 1,
            active: true,
            cta_text: None,
            accent_color: None,
        }])
}
```

- [ ] **Step 3: Add a test for the bundled-defaults fallback path**

Append to the test module in `src-tauri/src/promos.rs`:

```rust
#[test]
fn bundled_defaults_helper_returns_non_empty() {
    let pool = bundled_defaults();
    assert!(!pool.is_empty(), "bundled_defaults() must never return empty");
    assert!(pool.iter().all(|p| !p.id.is_empty()));
}
```

- [ ] **Step 4: Run the tests**

```bash
cd src-tauri && cargo test promos
```

Expected: 8 tests pass (the new one + the existing 7).

- [ ] **Step 5: Initialize the source in lib.rs**

Edit `src-tauri/src/lib.rs`. Find the `Builder::default()` block (search for `tauri::Builder` or `.setup(`). Inside the `.setup(|app| { ... })` closure, add:

```rust
// Promo rotation: bootstrap from disk cache (or bundled fallback)
// synchronously so the first ad break of the session has something
// to show, then spawn the background refresh.
let cache_dir = app.path().app_config_dir()
    .or_else(|_| app.path().app_data_dir())
    .expect("app config or data dir must resolve");
let promo_source = std::sync::Arc::new(crate::promos::SyvrRemoteSource::new(cache_dir));
promo_source.bootstrap_load();
{
    let src = promo_source.clone();
    tauri::async_runtime::spawn(async move {
        src.run_refresh_loop().await;
    });
}
app.manage(promo_source.clone());
// Shared "last shown" promo ID for cooldown across ad breaks.
app.manage(std::sync::Arc::new(tokio::sync::RwLock::new(
    Option::<String>::None,
)) as std::sync::Arc<tokio::sync::RwLock<Option<String>>>);
```

- [ ] **Step 6: Wire promo picking into the lyrics resolver**

Edit `src-tauri/src/lyrics.rs`. The `ad_break_outcome` function from Task 2 needs to also pick a promo. Update its signature to accept the source + cooldown state, and import the rotation logic:

```rust
async fn ad_break_outcome(
    snap: &crate::smtc::CurrentTrack,
    promo_source: &std::sync::Arc<crate::promos::SyvrRemoteSource>,
    last_shown: &std::sync::Arc<tokio::sync::RwLock<Option<String>>>,
) -> CurrentLyrics {
    use crate::promos::PromoSource;
    let pool = promo_source.promos();
    let cooldown_id = { last_shown.read().await.clone() };
    let picked = crate::promos::pick_next_promo(&pool, cooldown_id.as_deref()).cloned();
    if let Some(ref p) = picked {
        let mut w = last_shown.write().await;
        *w = Some(p.id.clone());
    }
    CurrentLyrics {
        track_key: format!("ad|{}|{}", snap.source_app_id.clone().unwrap_or_default(), snap.duration_ms),
        status: Status::Ad,
        source: None,
        line_count: 0,
        lines: Vec::new(),
        plain: None,
        translation: None,
        errors: Vec::new(),
        track: TrackEcho {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            duration_ms: snap.duration_ms,
        },
        promo: picked,
    }
}
```

Update the test in Task 2 to pass mocks. Replace `ad_short_circuit_tests` with:

```rust
#[cfg(test)]
mod ad_short_circuit_tests {
    use super::*;
    use crate::promos::{Promo, SyvrRemoteSource};

    #[tokio::test]
    async fn ad_active_skips_network_and_emits_ad_status() {
        let mut snap = crate::smtc::CurrentTrack::default();
        snap.title = "Advertisement".into();
        snap.artist = "Spotify".into();
        snap.duration_ms = 30_000;
        snap.ad_active = true;
        snap.source_app_id = Some("Spotify.exe".into());

        // Use a temp cache dir for the source so the test doesn't
        // pollute the real %APPDATA%.
        let tmp = std::env::temp_dir().join("hum-test-promos");
        std::fs::create_dir_all(&tmp).unwrap();
        let source = std::sync::Arc::new(SyvrRemoteSource::new(tmp));
        source.bootstrap_load();
        let last = std::sync::Arc::new(tokio::sync::RwLock::new(None));

        let outcome = ad_break_outcome(&snap, &source, &last).await;
        assert_eq!(outcome.status, Status::Ad);
        assert!(outcome.lines.is_empty());
        assert_eq!(outcome.line_count, 0);
        assert!(outcome.promo.is_some(), "rotation should have picked something");
    }
}
```

- [ ] **Step 7: Update the resolver-loop short-circuit call site**

In `src-tauri/src/lyrics.rs::start()`, the previous task added:

```rust
if snap.ad_active {
    let outcome = ad_break_outcome(&snap);
    ...
}
```

Update to pull the source + cooldown state from Tauri state:

```rust
if snap.ad_active {
    let source: tauri::State<'_, std::sync::Arc<crate::promos::SyvrRemoteSource>> = app.state();
    let last: tauri::State<'_, std::sync::Arc<tokio::sync::RwLock<Option<String>>>> = app.state();
    let outcome = ad_break_outcome(&snap, source.inner(), last.inner()).await;
    let key = outcome.track_key.clone();
    if key != last_key {
        last_key = key;
        {
            let mut s = shared.write().await;
            *s = outcome.clone();
        }
        let _ = app.emit("lyrics-loaded", &outcome);
    }
    continue;
}
```

- [ ] **Step 8: Mirror the Promo type to frontend**

Edit `src/types.ts`. Add:

```ts
export type Promo = {
  id: string;
  product_name: string;
  tagline: string;
  url: string;
  icon_url: string | null;
  weight: number;
  active: boolean;
  cta_text: string | null;
  accent_color: string | null;
};
```

And add to `CurrentLyrics`:

```ts
export type CurrentLyrics = {
  // ... existing fields ...
  promo?: Promo | null;
};
```

- [ ] **Step 9: Temporarily update statusLine to show the promo text (placeholder)**

In `src/Overlay.tsx`, replace the Task-1 placeholder branch in `statusLine`:

```ts
case "ad":
  if (l.promo) {
    return `♪ Ad break — ${l.promo.product_name}: ${l.promo.tagline}`;
  }
  return "♪ Ad break — Brought to you by SYVR Studios";
```

This is still temporary — Task 5 builds the actual PromoCard component. This step just proves the promo arrives through the event payload.

- [ ] **Step 10: Run all checks**

```bash
cd src-tauri && cargo check && cargo test
cd .. && pnpm typecheck
```

All clean, all tests pass.

- [ ] **Step 11: Manual verification with the temporary Spotify-forces-ad-state hack**

Same as Task 2 step 7. Re-apply the temporary `snap.ad_active = true` in `smtc.rs::emit_blended` for Spotify. Run `pnpm tauri dev`. Verify the overlay shows `♪ Ad break — SYVR Studios: Tools and apps from the makers of Hum.` (since the bundled default is the only entry).

REVERT the temporary edit before committing.

- [ ] **Step 12: Add CHANGELOG entry**

```markdown
## [0.12.0-rc4] - 2026-05-22

### Added (internal — promo data wired end-to-end, still no detection)
- **Promo rotation engine fetches from `https://syvrstudios.com/hum/promos.json` on startup and every 6 hours.** Falls back to a disk cache at `%APPDATA%\com.syvr.hum\promos.json`, then to a bundled `default_promos.json`, then to a hardcoded SYVR Studios entry. Picked promo rides on the `CurrentLyrics` payload's new `promo` field whenever `status == Ad`. Cooldown state (last-shown id) lives in app-managed shared state. No detection yet; verified manually with a temporary force-ad hack on Spotify.

  **Implementation:** New `PromoSource` trait + `SyvrRemoteSource` in `src-tauri/src/promos.rs`. Bootstrap is synchronous (reads disk before the event loop starts); refresh is a background tokio task. `tauri-plugin-store` not used — promos cache is a plain JSON file via `std::fs::write` because the existing store plugin's API is overkill for one file. `ad_break_outcome` in `lyrics.rs` is now async and consults the rotation engine. Promo type mirrored to `src/types.ts`.
```

- [ ] **Step 13: Bump version and commit**

Version: `0.12.0-rc3` → `0.12.0-rc4`.

```bash
git add src-tauri/src/promos.rs src-tauri/src/lyrics.rs src-tauri/src/lib.rs src/types.ts src/Overlay.tsx package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "ad-detect(4/9): promo fetch + cache + fallback chain wired"
```

---

## Task 5: PromoCard React component + AD BREAK chip + hide artist line

The full visual treatment. After this task, `pnpm tauri dev` + the temporary force-ad hack shows the real card.

**Files:**
- Modify: `src/Overlay.tsx` (new PromoCard component, conditional render, hide metadata artist line during ads, swap source badge to AD BREAK)
- Modify: `src-tauri/Cargo.toml` (`tauri-plugin-opener`)
- Modify: `src-tauri/capabilities/default.json` (opener permission)
- Modify: `src-tauri/src/lib.rs` (init the opener plugin)
- Modify: `docs/CHANGELOG.md`

- [ ] **Step 1: Add `tauri-plugin-opener` to Cargo.toml**

Edit `src-tauri/Cargo.toml` `[dependencies]`:

```toml
tauri-plugin-opener = "2"
```

- [ ] **Step 2: Init the plugin in lib.rs**

Edit `src-tauri/src/lib.rs`. In the `tauri::Builder::default()` chain, find where other plugins are registered (`.plugin(tauri_plugin_store::Builder::default().build())` or similar). Add:

```rust
.plugin(tauri_plugin_opener::init())
```

- [ ] **Step 3: Grant opener permission**

Edit `src-tauri/capabilities/default.json`. In the `permissions` array, add:

```json
"opener:default",
"opener:allow-open-url"
```

- [ ] **Step 4: Add `opener` to package.json deps**

```bash
cd D:/Work/App_Projects/All_Projects/lyric-overlay
pnpm add @tauri-apps/plugin-opener
```

- [ ] **Step 5: Write the PromoCard component**

Edit `src/Overlay.tsx`. Near the other helper components (after `ArtistInfoDot` is a good spot), add:

```tsx
import { openUrl } from "@tauri-apps/plugin-opener";
import type { Promo } from "./types";

// Promo card rendered in place of the lyric rows during an ad break.
// Three-line layout: supertitle / [icon + product name + tagline] / CTA.
// In single_line layout it collapses to one row.
function PromoCard({
  promo,
  textColor,
  textColorDim,
  textShadow,
  scaledFontSize,
  layoutMode,
  dragRegion,
}: {
  promo: Promo | null | undefined;
  textColor: string;
  textColorDim: string;
  textShadow: string;
  scaledFontSize: number;
  layoutMode: LayoutMode;
  dragRegion: boolean;
}) {
  const accent = promo?.accent_color ?? "#d4af37";
  const cta = promo?.cta_text ?? "Learn more →";
  const productName = promo?.product_name ?? "SYVR Studios";
  const tagline = promo?.tagline ?? "Tools and apps from the makers of Hum.";
  const url = promo?.url ?? "https://syvrstudios.com";
  const iconUrl = promo?.icon_url ?? null;

  const handleClick = (e: React.MouseEvent) => {
    if (dragRegion) return;
    e.stopPropagation();
    openUrl(url).catch(() => {});
  };
  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};

  if (layoutMode === "single_line") {
    return (
      <div
        {...drag}
        onClick={handleClick}
        style={{
          display: "flex",
          flexDirection: "row",
          alignItems: "center",
          gap: 10,
          cursor: dragRegion ? "move" : "pointer",
          maxWidth: "92vw",
          overflow: "hidden",
        }}
      >
        {iconUrl ? (
          <img
            src={iconUrl}
            alt=""
            draggable={false}
            style={{ width: 28, height: 28, borderRadius: 4, flexShrink: 0, pointerEvents: "none" }}
            onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
          />
        ) : null}
        <span style={{
          fontSize: scaledFontSize,
          color: textColor,
          textShadow,
          fontWeight: 600,
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}>
          {productName}
        </span>
        <span style={{ fontSize: scaledFontSize * 0.65, color: textColorDim, textShadow, opacity: 0.85 }}>·</span>
        <span style={{
          fontSize: scaledFontSize * 0.65,
          color: textColorDim,
          textShadow,
          opacity: 0.85,
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}>
          {tagline}
        </span>
        <span style={{ fontSize: scaledFontSize * 0.65, color: accent, textShadow, marginLeft: 6, whiteSpace: "nowrap" }}>
          {cta}
        </span>
      </div>
    );
  }

  // three_line + full_page: stacked card.
  return (
    <div
      {...drag}
      onClick={handleClick}
      className="hum-line-in"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 2,
        cursor: dragRegion ? "move" : "pointer",
        maxWidth: "92vw",
        overflow: "hidden",
      }}
    >
      <div style={{
        fontSize: Math.max(9, scaledFontSize * 0.38),
        color: textColorDim,
        textShadow,
        opacity: 0.7,
        letterSpacing: 0.4,
        textTransform: "uppercase",
        whiteSpace: "nowrap",
      }}>
        Brought to you by SYVR Studios
      </div>
      <div style={{
        display: "flex",
        flexDirection: "row",
        alignItems: "center",
        gap: 10,
      }}>
        {iconUrl ? (
          <img
            src={iconUrl}
            alt=""
            draggable={false}
            style={{ width: 32, height: 32, borderRadius: 4, flexShrink: 0, pointerEvents: "none" }}
            onError={(e) => { (e.currentTarget as HTMLImageElement).style.display = "none"; }}
          />
        ) : null}
        <div style={{
          display: "flex",
          flexDirection: "column",
          minWidth: 0,
          flex: 1,
        }}>
          <div style={{
            fontSize: scaledFontSize,
            color: textColor,
            textShadow,
            fontWeight: 600,
            lineHeight: 1.15,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}>
            {productName}
          </div>
          <div style={{
            fontSize: scaledFontSize * 0.55,
            color: textColorDim,
            textShadow,
            opacity: 0.85,
            lineHeight: 1.2,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}>
            {tagline}
          </div>
        </div>
      </div>
      <div style={{
        fontSize: scaledFontSize * 0.6,
        color: accent,
        textShadow,
        marginTop: 2,
        textDecoration: "underline",
        textUnderlineOffset: 2,
        whiteSpace: "nowrap",
      }}>
        {cta}
      </div>
    </div>
  );
}
```

- [ ] **Step 6: Wire PromoCard into the three_line layout**

In `src/Overlay.tsx`, find the three_line layout's lyricsCol div (`<div {...dragProps} ref={setLyricsColEl} style={lyricsColStyle}>`). Wrap the existing LineRow stack with a conditional:

```tsx
<div {...dragProps} ref={setLyricsColEl} style={lyricsColStyle}>
{lyrics?.status === "ad" ? (
  <PromoCard
    promo={lyrics.promo ?? null}
    textColor={effectiveTextColor}
    textColorDim={effectiveTextColorDim}
    textShadow={effectiveTextShadow}
    scaledFontSize={settingsForRender.font_size_px}
    layoutMode={layoutMode}
    dragRegion={isEdit}
  />
) : (
  <>
    <LineRow text={prev?.text} kind="prev" dragRegion={isEdit} settings={settingsForRender} textShadow={effectiveTextShadow} />
    <LineRow
      text={middleText}
      kind="cur"
      dragRegion={isEdit}
      settings={settingsForRender}
      karaoke={curKaraoke}
      textShadow={effectiveTextShadow}
    />
    {translationText ? (
      <TranslationRow text={translationText} settings={settingsForRender} textShadow={effectiveTextShadow} />
    ) : (
      <LineRow text={next?.text} kind="next" dragRegion={isEdit} settings={settingsForRender} textShadow={effectiveTextShadow} />
    )}
  </>
)}
</div>
```

- [ ] **Step 7: Same swap in single_line layout**

In the `single_line` branch of `Overlay.tsx`, wrap the LineRow + TranslationRow similarly:

```tsx
<div {...dragProps} ref={setLyricsColEl} style={lyricsColStyle}>
  {lyrics?.status === "ad" ? (
    <PromoCard
      promo={lyrics.promo ?? null}
      textColor={effectiveTextColor}
      textColorDim={effectiveTextColorDim}
      textShadow={effectiveTextShadow}
      scaledFontSize={settingsForRender.font_size_px}
      layoutMode={layoutMode}
      dragRegion={isEdit}
    />
  ) : (
    <>
      <LineRow ... />
      ...
    </>
  )}
</div>
```

(Use the original LineRow / TranslationRow code from the existing single_line block; just wrap with the conditional.)

- [ ] **Step 8: Same swap in full_page layout (centered card)**

In the `full_page` branch, replace the `hasLines ? lyrics!.lines.map(...) : LineRow` block with:

```tsx
{lyrics?.status === "ad" ? (
  <PromoCard
    promo={lyrics.promo ?? null}
    textColor={effectiveTextColor}
    textColorDim={effectiveTextColorDim}
    textShadow={effectiveTextShadow}
    scaledFontSize={settingsForRender.font_size_px}
    layoutMode={layoutMode}
    dragRegion={isEdit}
  />
) : hasLines ? (
  lyrics!.lines.map(...)  // unchanged
) : (
  <LineRow ... />  // unchanged fallback
)}
```

- [ ] **Step 9: Hide artist line + swap source badge in MetadataColumn during ads**

The MetadataColumn currently takes `track` but doesn't know about ad state. Update its props to accept `adActive: boolean`:

```tsx
function MetadataColumn({
  track,
  textColor,
  textColorDim,
  textShadow,
  source,
  alignRight,
  dragRegion,
  adActive,
}: {
  track: CurrentTrack;
  textColor: string;
  textColorDim: string;
  textShadow: string;
  source: string | null;
  alignRight: boolean;
  dragRegion: boolean;
  adActive: boolean;
}) {
  // ... existing code through hasDuration check ...

  const metaParts = [track.artist, track.title, track.album]
    .map((s) => (s || "").trim())
    .filter((s) => s.length > 0);
  const metaText = metaParts.join(" · ");
  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};

  return (
    <div {...drag} style={{ /* unchanged styles */ }}>
      {/* Artist line: hidden during ads */}
      {!adActive && metaText ? (
        <div title={metaText} style={{ /* unchanged */ }}>
          {metaText}
        </div>
      ) : null}

      {hasDuration ? (
        <ProgressBar
          track={track}
          textColor={textColor}
          textColorDim={textColorDim}
          textShadow={textShadow}
        />
      ) : null}

      {adActive ? (
        <AdBreakChip textShadow={textShadow} />
      ) : (
        <SourceBadge
          appId={track.source_app_id}
          overrideLabel={source}
          textColorDim={textColorDim}
          textShadow={textShadow}
        />
      )}
    </div>
  );
}
```

- [ ] **Step 10: Add the AdBreakChip component**

In `src/Overlay.tsx`, right next to `SourceBadge`:

```tsx
function AdBreakChip({ textShadow }: { textShadow: string }) {
  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "1px 6px",
        borderRadius: 8,
        fontSize: 9.5,
        letterSpacing: 0.6,
        textTransform: "uppercase",
        color: "rgba(212, 175, 55, 0.95)",
        textShadow,
        background: "rgba(212, 175, 55, 0.12)",
        border: "1px solid rgba(212, 175, 55, 0.5)",
        opacity: 0.95,
        whiteSpace: "nowrap",
      }}
    >
      Ad Break
    </div>
  );
}
```

- [ ] **Step 11: Pass `adActive` at every MetadataColumn call site**

Search `src/Overlay.tsx` for `<MetadataColumn`. There are two existing call sites (single_line + three_line). Add `adActive={track.ad_active}` to both:

```tsx
<MetadataColumn
  track={track}
  textColor={effectiveTextColor}
  textColorDim={effectiveTextColorDim}
  textShadow={effectiveTextShadow}
  source={null}
  alignRight
  dragRegion={isEdit}
  adActive={track.ad_active}
/>
```

- [ ] **Step 12: Remove the placeholder statusLine "ad" branch**

In `src/Overlay.tsx::statusLine`, remove the `case "ad":` block added in Tasks 1 + 4. Status "ad" no longer flows through `statusLine` because the lyric rows are replaced entirely by PromoCard.

- [ ] **Step 13: Run typecheck**

```bash
pnpm typecheck
```

Expected: clean.

- [ ] **Step 14: Manual verification with the temporary force-ad hack**

Re-apply the Spotify force-ad hack in `smtc.rs::emit_blended` one more time. Run `pnpm tauri dev`. Play Spotify. Verify:

1. The lyric area shows the PromoCard (SYVR Studios card from bundled defaults).
2. The album art on the left stays visible.
3. The right metadata column shows: progress bar + time, AD BREAK chip in place of SPOTIFY chip. Artist · Song · Album line is hidden.
4. Clicking the promo card opens `https://syvrstudios.com` in the default browser.
5. In edit mode, clicking the card drags the window instead.

REVERT the temporary hack before committing.

- [ ] **Step 15: Add CHANGELOG entry**

```markdown
## [0.12.0-rc5] - 2026-05-22

### Added
- **SYVR promo card replaces the lyric area during ad breaks** (still no real detection — pending Tasks 6-8). When `lyrics.status === "ad"`, the prev / cur / next lyric rows in the three-line and single-line layouts (or the scrolling column in full-page) are swapped out for a stacked card showing: a small `Brought to you by SYVR Studios` supertitle, an optional 32×32 product icon, the product name (same size as the current lyric), a dim tagline, and a clickable CTA (defaults to `Learn more →`). The card is fully clickable in locked/ghost modes — opens the promo's URL in the default browser via the new `tauri-plugin-opener`. In edit mode the card is a drag region instead. Right-side metadata column behavior: the Artist · Song · Album line is hidden during ads; the source badge swaps to an amber `AD BREAK` chip with the same shape as the existing source badges; the progress bar + time readout stays so users can see how much of the ad is left.

  **Implementation:** New `PromoCard` and `AdBreakChip` components in `src/Overlay.tsx`. `MetadataColumn` gained an `adActive: boolean` prop wired from `track.ad_active`. Added `tauri-plugin-opener` (Rust + JS sides) plus `opener:default` / `opener:allow-open-url` capabilities. PromoCard renders in all three layouts (`three_line` stacked, `single_line` inline-collapsed, `full_page` centered stacked).
```

- [ ] **Step 16: Bump version and commit**

Version `0.12.0-rc4` → `0.12.0-rc5`.

```bash
cd src-tauri && cargo check
cd .. && pnpm typecheck
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/capabilities/default.json src-tauri/tauri.conf.json package.json pnpm-lock.yaml src/Overlay.tsx docs/CHANGELOG.md
git commit -m "ad-detect(5/9): PromoCard + AD BREAK chip + hide artist line during ads"
```

---

## Task 6: Spotify ad detection (SMTC heuristics)

First real detector. After this task, free-tier Spotify ads automatically swap the overlay to the promo card.

**Files:**
- Modify: `src-tauri/src/smtc.rs` (new `is_spotify_ad` helper, called inside `emit_blended`)
- Modify: `docs/CHANGELOG.md`

- [ ] **Step 1: Write failing tests for `is_spotify_ad`**

In `src-tauri/src/smtc.rs`, find the existing test module (or add one at the bottom). Add:

```rust
#[cfg(test)]
mod is_spotify_ad_tests {
    use super::*;

    fn snap_with(title: &str, artist: &str, app_id: &str, state: PlaybackState) -> CurrentTrack {
        let mut t = CurrentTrack::default();
        t.title = title.into();
        t.artist = artist.into();
        t.state = state;
        t.source_app_id = if app_id.is_empty() { None } else { Some(app_id.into()) };
        t
    }

    #[test]
    fn non_spotify_source_never_ad() {
        let t = snap_with("Advertisement", "Spotify", "Chrome.exe", PlaybackState::Playing);
        assert!(!is_spotify_ad(&t));
    }

    #[test]
    fn spotify_title_advertisement_matches() {
        let t = snap_with("Advertisement", "", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_title_advertisement_case_insensitive() {
        let t = snap_with("advertisement", "", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_title_literal_spotify_matches() {
        let t = snap_with("Spotify", "", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_artist_field_spotify_with_nonempty_title_matches() {
        // Spotify sometimes sets artist="Spotify" and title=<some ad copy>.
        let t = snap_with("Try Premium Free", "Spotify", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_real_song_never_ad() {
        let t = snap_with("Mr. Brightside", "The Killers", "Spotify.exe", PlaybackState::Playing);
        assert!(!is_spotify_ad(&t));
    }

    #[test]
    fn spotify_aumid_format_also_matches() {
        // Spotify also appears as AUMID `SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify`.
        let t = snap_with("Advertisement", "", "SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_empty_title_and_artist_while_playing_matches() {
        let t = snap_with("", "", "Spotify.exe", PlaybackState::Playing);
        assert!(is_spotify_ad(&t));
    }

    #[test]
    fn spotify_empty_while_paused_not_ad() {
        let t = snap_with("", "", "Spotify.exe", PlaybackState::Paused);
        assert!(!is_spotify_ad(&t));
    }
}
```

- [ ] **Step 2: Run tests (should fail with "is_spotify_ad not found")**

```bash
cd src-tauri && cargo test is_spotify_ad
```

Expected: FAIL with `cannot find function is_spotify_ad`.

- [ ] **Step 3: Implement `is_spotify_ad`**

In `src-tauri/src/smtc.rs`, near the top of the file (after the struct definitions, before `start()`), add:

```rust
/// Heuristic detector for Spotify ad breaks via SMTC metadata patterns.
///
/// Spotify keeps publishing track metadata during ads, just with telltale
/// patterns:
/// - title == "Advertisement" (the most explicit case)
/// - title == "Spotify" (Spotify rotates "Spotify" through this slot too)
/// - artist == "Spotify" with a non-empty title (ad copy in the title slot)
/// - empty title + empty artist while Playing (rare but observed)
///
/// All matches require the source to be Spotify (matched on `source_app_id`
/// containing "spotify" case-insensitively).
pub(crate) fn is_spotify_ad(t: &CurrentTrack) -> bool {
    let src = t.source_app_id.as_deref().unwrap_or("").to_lowercase();
    if !src.contains("spotify") {
        return false;
    }

    let title = t.title.trim();
    let artist = t.artist.trim();

    if title.eq_ignore_ascii_case("Advertisement") { return true; }
    if title.eq_ignore_ascii_case("Spotify") { return true; }
    if artist.eq_ignore_ascii_case("Spotify") && !title.is_empty() { return true; }

    if title.is_empty() && artist.is_empty() && t.state == PlaybackState::Playing {
        return true;
    }

    false
}
```

- [ ] **Step 4: Run tests (should pass)**

```bash
cd src-tauri && cargo test is_spotify_ad
```

Expected: 9 tests pass.

- [ ] **Step 5: Wire it into `emit_blended`**

In `src-tauri/src/smtc.rs::emit_blended` (around line 171), set `ad_active` BEFORE the tier-1 SMTC-actively-playing check, so even paused ads don't get treated as normal tracks:

```rust
async fn emit_blended(app: &AppHandle, event: &str, mut snap: CurrentTrack) {
    use tauri::Manager;

    // Spotify ad detection runs before the tier-1 priority check so that
    // SMTC's "playing" state alone doesn't fast-path past it.
    if is_spotify_ad(&snap) {
        snap.ad_active = true;
    }

    // Tier 1: ... (unchanged)
    let smtc_actively_playing =
        snap.state == PlaybackState::Playing && !snap.title.trim().is_empty();
    if smtc_actively_playing {
        let _ = app.emit(event, &snap);
        return;
    }
    // ... rest unchanged ...
}
```

Wait — looking more carefully: tier 1 also fires when title is non-empty and playing. A Spotify ad with `title = "Advertisement"` is non-empty and playing, so it would emit through tier 1 path. That's fine because `ad_active` is already set on `snap` at that point. The emit carries the flag.

- [ ] **Step 6: Run all tests + cargo check**

```bash
cd src-tauri && cargo test && cargo check
```

Expected: all green.

- [ ] **Step 7: Manual end-to-end verification on real Spotify free**

Run `pnpm tauri dev`. Play Spotify free tier. When an ad rolls:
1. PromoCard appears (no more force-hack needed)
2. AD BREAK chip in place of SPOTIFY
3. Progress bar counts through the ad duration
4. Click → opens the promo URL

When the next song plays:
1. PromoCard disappears, lyrics resume
2. AD BREAK chip swaps back to SPOTIFY
3. Artist · Song · Album line reappears

If Spotify Premium is the only available account, manually verify the unit tests pass and skip the live ad verification — the heuristic is independently testable.

- [ ] **Step 8: Add CHANGELOG entry**

```markdown
## [0.12.0-rc6] - 2026-05-22

### Added
- **Spotify ad-break detection.** Spotify's free tier ad breaks now automatically trigger the SYVR promo card. Detection runs entirely off SMTC metadata heuristics with zero new permissions, processes, or APIs. Fires when the source is Spotify (matched on `source_app_id` containing `spotify`) AND one of: title is `Advertisement` or `Spotify` (case-insensitive); artist is `Spotify` with non-empty title; OR both title and artist are empty while Playing (rare but observed for cold-started sessions). Real Spotify songs (`title = "Mr. Brightside"`, etc.) are never matched. Spotify Premium users never see ads → never trigger this path.

  **Implementation:** New `is_spotify_ad` helper in `src-tauri/src/smtc.rs`. Called from `emit_blended` before the tier-1 priority check so the flag rides on the snapshot regardless of state. Nine unit tests cover positive matches (Advertisement / Spotify title, Spotify artist, empty-while-playing, AUMID), negatives (real song, paused empty, non-Spotify source), and case-insensitivity.
```

- [ ] **Step 9: Bump version and commit**

Version `0.12.0-rc5` → `0.12.0-rc6`.

```bash
git add src-tauri/src/smtc.rs package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "ad-detect(6/9): Spotify SMTC ad-break detection"
```

---

## Task 7: Pandora desktop ad detection + countdown reading

**Files:**
- Modify: `src-tauri/src/pandora_desktop.rs` (extend `PandoraDesktopProbe::read` to detect ad state + read countdown)
- Modify: `src-tauri/src/web_bridge.rs` (add `is_ad: bool` + `ad_duration_ms: Option<u64>` to `WebBridgeTrack`; map them onto snapshot in `blend_bridge_into_snapshot`)
- Modify: `docs/CHANGELOG.md`

- [ ] **Step 1: Add `is_ad` to `WebBridgeTrack`**

Edit `src-tauri/src/web_bridge.rs` around line 37. Update the struct:

```rust
#[derive(Clone, Debug, Serialize, Default)]
pub struct WebBridgeTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub source: String,
    pub last_seen_unix_ms: i64,
    #[serde(default)]
    pub position_ms: Option<u64>,
    #[serde(default)]
    pub state: Option<crate::smtc::PlaybackState>,
    /// Set by probes that can detect an ad break (Pandora desktop /
    /// Pandora web). When true, `blend_bridge_into_snapshot` flips
    /// `snap.ad_active = true`. `position_ms` / `duration_ms` still
    /// reflect ad timing when present.
    #[serde(default)]
    pub is_ad: bool,
}
```

- [ ] **Step 2: Map `is_ad` onto the snapshot in `blend_bridge_into_snapshot`**

Edit `src-tauri/src/web_bridge.rs` around line 90. After the existing `if let Some(pos) = bt.position_ms { ... }` block, add:

```rust
if bt.is_ad {
    snap.ad_active = true;
}
```

Also handle the duration when the bridge reports it. The struct doesn't have a separate `ad_duration_ms` field — bridges should set `WebBridgeTrack` like a normal track when `is_ad`, with `position_ms` being elapsed-into-ad and `duration_ms` (via a new field, OR via the existing snapshot-level `duration_ms`).

Pragmatic choice: keep `WebBridgeTrack` minimal. Add a new optional field `duration_ms: Option<u64>` (separate from snap's duration so probes can express it independently of SMTC's track duration):

```rust
pub struct WebBridgeTrack {
    // ... existing fields ...
    #[serde(default)]
    pub duration_ms: Option<u64>,
}
```

And in `blend_bridge_into_snapshot`:

```rust
if let Some(dur) = bt.duration_ms {
    snap.duration_ms = dur;
}
```

- [ ] **Step 3: Write failing tests for Pandora desktop ad detection**

In `src-tauri/src/pandora_desktop.rs`, find the existing test module. Add:

```rust
#[cfg(test)]
mod ad_detection_tests {
    use super::*;

    /// Classify ad detection logic in isolation from UIA. We test the
    /// classifier that takes an enumerated set of Hyperlink URLs + a
    /// possibly-empty countdown text and decides ad-ness.
    #[test]
    fn empty_url_set_with_pandora_window_present_is_ad() {
        let urls: Vec<String> = vec![];
        let countdown = Some("0:23".to_string());
        let result = classify_pandora_state(&urls, countdown.as_deref());
        assert!(result.is_ad, "no /TR URLs + countdown present → ad");
        assert_eq!(result.countdown_seconds, Some(23));
    }

    #[test]
    fn url_set_with_TR_link_is_not_ad() {
        let urls = vec!["https://www.pandora.com/artist/x/y/TR123".into()];
        let result = classify_pandora_state(&urls, None);
        assert!(!result.is_ad);
    }

    #[test]
    fn countdown_parses_minutes_seconds() {
        let urls: Vec<String> = vec![];
        let result = classify_pandora_state(&urls, Some("1:05"));
        assert!(result.is_ad);
        assert_eq!(result.countdown_seconds, Some(65));
    }

    #[test]
    fn countdown_parses_zero_seconds() {
        let urls: Vec<String> = vec![];
        let result = classify_pandora_state(&urls, Some("0:00"));
        assert!(result.is_ad);
        assert_eq!(result.countdown_seconds, Some(0));
    }

    #[test]
    fn malformed_countdown_returns_none() {
        let urls: Vec<String> = vec![];
        let result = classify_pandora_state(&urls, Some("not a countdown"));
        assert!(result.is_ad, "no /TR URLs is still an ad signal");
        assert_eq!(result.countdown_seconds, None);
    }
}
```

- [ ] **Step 4: Run tests (should fail)**

```bash
cd src-tauri && cargo test pandora_desktop::ad_detection_tests
```

Expected: FAIL — `classify_pandora_state` doesn't exist.

- [ ] **Step 5: Implement `classify_pandora_state`**

In `src-tauri/src/pandora_desktop.rs`, near the other helpers (`classify_pandora_url` is a sibling), add:

```rust
pub(crate) struct PandoraStateResult {
    pub is_ad: bool,
    /// Seconds remaining in the ad, if the countdown widget was readable.
    /// None when the countdown text couldn't be parsed.
    pub countdown_seconds: Option<u64>,
}

/// Given the URLs found in the player region + optionally the countdown
/// widget text, classify as ad or normal track.
///
/// - At least one URL matching `classify_pandora_url(...).is_track()` →
///   normal track. Otherwise → ad.
/// - Countdown text parsing: `M:SS` format. Returns total seconds when
///   parseable, None otherwise. Ad classification is independent of
///   countdown parseability.
pub(crate) fn classify_pandora_state(
    urls: &[String],
    countdown_text: Option<&str>,
) -> PandoraStateResult {
    let has_track = urls.iter().any(|u| {
        matches!(classify_pandora_url(u), PandoraUrlKind::Track(_))
    });
    let countdown_seconds = countdown_text.and_then(parse_countdown_to_seconds);
    PandoraStateResult {
        is_ad: !has_track,
        countdown_seconds,
    }
}

fn parse_countdown_to_seconds(text: &str) -> Option<u64> {
    let text = text.trim();
    let (mins, secs) = text.split_once(':')?;
    let mins: u64 = mins.parse().ok()?;
    let secs: u64 = secs.parse().ok()?;
    if secs >= 60 { return None; }
    Some(mins * 60 + secs)
}
```

You'll need `PandoraUrlKind::Track` to be visible — verify it's pub(crate) or move/expose as needed. If `classify_pandora_url` already returns an enum with a `Track` variant, just match it; if it returns something else, adapt.

(Worker note: read the existing `classify_pandora_url` signature and PandoraUrlKind enum before writing the match. The existing function in `pandora_desktop.rs` near line 161 has the canonical shape.)

- [ ] **Step 6: Run the tests (should pass)**

```bash
cd src-tauri && cargo test pandora_desktop::ad_detection_tests
```

Expected: 5 tests pass.

- [ ] **Step 7: Extend `PandoraDesktopProbe::read` to use the new classifier**

This step is integration — it modifies the real UIA-walking `read()` method. The walk now needs to also collect the countdown text. The exact node Path depends on Pandora's UIA tree, which can only be verified against a running Pandora desktop install.

For the implementation:
1. Run `cargo run --bin dump_uia -- --window pandora` (or whatever the existing dump tool's invocation is) while Pandora is playing an ad. Find the countdown text node in the printout — it'll match `^\d+:\d{2}$` and be near the ad-overlay text.
2. Once located, extend the DFS in `read()` to capture the text of that node alongside the existing Hyperlink URL collection.
3. After the DFS completes, call `classify_pandora_state(&collected_urls, countdown_text.as_deref())`.
4. Construct the returned `WebBridgeTrack` differently for ad state:

```rust
let state = classify_pandora_state(&urls, countdown.as_deref());

if state.is_ad {
    // Cache the first-seen total duration per-track-key (the ad's track
    // key is some stable identifier — for Pandora desktop ads, lacking
    // a /TR id, use a window-anchored fallback like
    // `pandora-ad|<hwnd>|<first-seen-unix-ms>`).
    let dur_ms = state.countdown_seconds.map(|s| s * 1000)
        .unwrap_or(30_000);  // 30s fallback when countdown isn't readable
    // Position is initial_duration - current_remaining.
    let position_ms = state.countdown_seconds
        .map(|s| dur_ms.saturating_sub(s * 1000))
        .unwrap_or(0);
    return Ok(Some(WebBridgeTrack {
        title: String::new(),
        artist: String::new(),
        album: String::new(),
        source: "pandora-desktop".into(),
        last_seen_unix_ms: now_unix_ms(),
        position_ms: Some(position_ms),
        state: Some(playback_state),  // from WASAPI peak meter, unchanged from v0.11.7
        is_ad: true,
        duration_ms: Some(dur_ms),
    }));
}

// existing non-ad path unchanged ...
```

- [ ] **Step 8: Add CHANGELOG entry**

```markdown
## [0.12.0-rc7] - 2026-05-22

### Added
- **Pandora desktop ad-break detection.** When the Pandora Microsoft Store app plays an ad break, Hum now switches the overlay to the SYVR promo card and shows the ad's countdown in the progress bar (when the countdown widget is readable from the UIA tree). Detection: the existing DFS-walk of Pandora's accessibility tree no longer requires `/artist/...TR{id}` Hyperlinks — when none are present but the window is still visible, the probe declares an ad break and looks for the player's countdown text (matches `M:SS`) to surface as ad position + duration.

  **Implementation:** New `classify_pandora_state` helper in `src-tauri/src/pandora_desktop.rs` that combines URL collection with countdown parsing into a single decision struct. `WebBridgeTrack` gained `is_ad: bool` + `duration_ms: Option<u64>` fields. `blend_bridge_into_snapshot` maps both onto the snapshot. Countdown parse fallback: when the countdown can't be read, ad duration defaults to 30 seconds and position starts at 0 (acceptable — most Pandora ads are 30s anyway, and progress will jump to "complete" when the next real track arrives).
```

- [ ] **Step 9: Bump version and commit**

Version `0.12.0-rc6` → `0.12.0-rc7`.

```bash
cd src-tauri && cargo test && cargo check
cd ..
git add src-tauri/src/pandora_desktop.rs src-tauri/src/web_bridge.rs package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "ad-detect(7/9): Pandora desktop ad detection + countdown"
```

---

## Task 8: Pandora web ad detection (extend existing `PandoraProbe`)

Same shape as Task 7 but for the Chrome-based Pandora web probe. The classifier helper is shared between both probes.

**Files:**
- Modify: `src-tauri/src/web_bridge.rs::PandoraProbe::read`
- Modify: `docs/CHANGELOG.md`

- [ ] **Step 1: Reuse `classify_pandora_state` from `pandora_desktop.rs`**

Make sure `classify_pandora_state` and `PandoraStateResult` are `pub(crate)` in `pandora_desktop.rs` so `web_bridge.rs` can call them. If they aren't, add `pub(crate)`.

- [ ] **Step 2: Write the failing integration shape**

The unit-test layer is the same `classify_pandora_state` tests from Task 7. No new unit tests needed — the classifier is shared. Verification is end-to-end with a live ad. So this task is implementation + manual verification only.

- [ ] **Step 3: Extend `PandoraProbe::read`**

In `src-tauri/src/web_bridge.rs::PandoraProbe::read` (around line 447), add the same shape as the desktop probe:

```rust
fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>> {
    // ... existing tab-finding + DFS code that collects `urls: Vec<String>` ...

    // Also collect any text node matching M:SS in the player region.
    let countdown_text: Option<String> = /* added DFS branch */;

    let state = crate::pandora_desktop::classify_pandora_state(
        &urls,
        countdown_text.as_deref(),
    );

    if state.is_ad {
        let dur_ms = state.countdown_seconds.map(|s| s * 1000).unwrap_or(30_000);
        let position_ms = state.countdown_seconds
            .map(|s| dur_ms.saturating_sub(s * 1000))
            .unwrap_or(0);
        return Ok(Some(WebBridgeTrack {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            source: "pandora-web".into(),
            last_seen_unix_ms: now_unix_ms(),
            position_ms: Some(position_ms),
            // pandora-web doesn't track its own playback state (relies on
            // SMTC for that), so leave it None.
            state: None,
            is_ad: true,
            duration_ms: Some(dur_ms),
        }));
    }

    // Existing non-ad path returns Ok(Some(track)) as before.
    // ...
}
```

The countdown DFS branch: while walking the Chrome accessibility tree, capture any Text or Name node matching the regex `^\d+:\d{2}$`. The countdown is the only text in that format inside the Pandora player region, so a single regex match suffices. Cache the regex in a `OnceLock` for efficiency.

- [ ] **Step 4: Manual end-to-end verification**

Open Pandora in Chrome at free tier. Wait for an ad. Verify:

1. PromoCard appears
2. AD BREAK chip in metadata column
3. Progress bar shows ad position + remaining time
4. When ad ends, normal lyrics resume

- [ ] **Step 5: Add CHANGELOG entry**

```markdown
## [0.12.0-rc8] - 2026-05-22

### Added
- **Pandora web ad-break detection.** Free-tier Pandora.com ads in Chromium browsers (Chrome, Edge, Brave, Opera, Vivaldi) now also trigger the SYVR promo card with countdown. Shares the same `classify_pandora_state` decision helper as the desktop probe (Task 7) — the only difference is where the UIA tree comes from (Chrome's tab vs the Pandora.exe window).

  **Implementation:** Extended `PandoraProbe::read` in `src-tauri/src/web_bridge.rs`. Walks the Chrome accessibility tree for the countdown text (M:SS regex match) alongside the existing /TR URL collection. Constructs an `is_ad: true` `WebBridgeTrack` when no /TR URLs are present.
```

- [ ] **Step 6: Bump version and commit**

Version `0.12.0-rc7` → `0.12.0-rc8`.

```bash
cd src-tauri && cargo check
cd ..
git add src-tauri/src/web_bridge.rs src-tauri/src/pandora_desktop.rs package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "ad-detect(8/9): Pandora web ad detection"
```

---

## Task 9: YouTube ad detection (new probe) + Settings toggle + final 0.12.0 release

The biggest detection step. YouTube currently flows through SMTC via Chrome — there's no Hum-side YouTube bridge. This task adds one specifically for ad detection.

**Files:**
- Create: `src-tauri/src/youtube_bridge.rs`
- Modify: `src-tauri/src/lib.rs` (register module, add probe to registry)
- Modify: `src-tauri/src/web_bridge.rs` (add YouTubeProbe to PROBES registry)
- Modify: `src-tauri/src/settings.rs` (add `ad_break_promos_enabled: bool`)
- Modify: `src/types.ts` (Settings type)
- Modify: `src/Settings.tsx` (checkbox)
- Modify: `src/Overlay.tsx` (gate PromoCard render on setting)
- Modify: `docs/CHANGELOG.md`
- Modify: `Websites/sites/syvr-site/public/hum/promos.json` (create with real product list)
- Final version bump to 0.12.0 (drop -rc suffix)

- [ ] **Step 1: Write failing tests for YouTube ad classification**

Create `src-tauri/src/youtube_bridge.rs` with the classifier-only initial code:

```rust
//! YouTube ad-break detection via Chrome UIA tree scraping.
//!
//! YouTube's normal track metadata flows through SMTC (Chrome publishes
//! it via the MediaSession API). Hum doesn't need to scrape non-ad
//! metadata. This probe runs only for ad detection.

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct YouTubeAdState {
    pub is_ad: bool,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}

/// Classify a YouTube ad state given (a) any text nodes found in the
/// player region and (b) any timer text found ("0:05 / 0:30" or "0:30").
///
/// Ad markers (any of these → is_ad true):
/// - "Sponsored"
/// - "Ad ·"
/// - "Advertisement"
/// - "Skip Ad"  // also matches "Skip in", "Skip Ad in"
pub(crate) fn classify_youtube_state(
    texts: &[String],
    timer_text: Option<&str>,
) -> YouTubeAdState {
    let markers = ["Sponsored", "Ad ·", "Advertisement", "Skip Ad", "Skip in"];
    let is_ad = texts.iter().any(|t| {
        markers.iter().any(|m| t.contains(m))
    });

    if !is_ad {
        return YouTubeAdState { is_ad: false, position_ms: None, duration_ms: None };
    }

    let (position_ms, duration_ms) = timer_text.map(parse_youtube_timer).unwrap_or((None, None));
    YouTubeAdState { is_ad, position_ms, duration_ms }
}

/// Parse YouTube's timer in the format "M:SS / M:SS" (e.g. "0:05 / 0:30").
/// Returns (position_ms, duration_ms). If only one M:SS is present, treats
/// it as duration with position None.
fn parse_youtube_timer(text: &str) -> (Option<u64>, Option<u64>) {
    let text = text.trim();
    if let Some((left, right)) = text.split_once(" / ") {
        return (parse_mss_to_ms(left), parse_mss_to_ms(right));
    }
    (None, parse_mss_to_ms(text))
}

fn parse_mss_to_ms(text: &str) -> Option<u64> {
    let text = text.trim();
    let (mins, secs) = text.split_once(':')?;
    let mins: u64 = mins.parse().ok()?;
    let secs: u64 = secs.parse().ok()?;
    if secs >= 60 { return None; }
    Some((mins * 60 + secs) * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_markers_not_ad() {
        let texts = vec!["Some other text".to_string(), "Music video".to_string()];
        let r = classify_youtube_state(&texts, None);
        assert!(!r.is_ad);
    }

    #[test]
    fn sponsored_text_is_ad() {
        let texts = vec!["Sponsored".to_string()];
        let r = classify_youtube_state(&texts, None);
        assert!(r.is_ad);
    }

    #[test]
    fn skip_ad_text_is_ad() {
        let texts = vec!["Skip Ad in 3".to_string()];
        let r = classify_youtube_state(&texts, None);
        assert!(r.is_ad);
    }

    #[test]
    fn ad_bullet_text_is_ad() {
        let texts = vec!["Ad · 0:30".to_string()];
        let r = classify_youtube_state(&texts, None);
        assert!(r.is_ad);
    }

    #[test]
    fn timer_parses_both_sides() {
        let texts = vec!["Sponsored".to_string()];
        let r = classify_youtube_state(&texts, Some("0:05 / 0:30"));
        assert!(r.is_ad);
        assert_eq!(r.position_ms, Some(5_000));
        assert_eq!(r.duration_ms, Some(30_000));
    }

    #[test]
    fn timer_with_only_duration() {
        let texts = vec!["Advertisement".to_string()];
        let r = classify_youtube_state(&texts, Some("0:15"));
        assert!(r.is_ad);
        assert_eq!(r.position_ms, None);
        assert_eq!(r.duration_ms, Some(15_000));
    }
}
```

- [ ] **Step 2: Register the module**

Edit `src-tauri/src/lib.rs`. Add to the `mod` declarations:

```rust
mod youtube_bridge;
```

- [ ] **Step 3: Run tests (should pass)**

```bash
cd src-tauri && cargo test youtube_bridge
```

Expected: 6 tests pass.

- [ ] **Step 4: Implement `YouTubeProbe` UIA-walk integration**

In `src-tauri/src/youtube_bridge.rs`, add the probe implementation:

```rust
use crate::web_bridge::{WebBridgeTrack, WebPlayerProbe, read_window_title};

pub(crate) struct YouTubeProbe;

impl WebPlayerProbe for YouTubeProbe {
    fn name(&self) -> &'static str { "youtube-web" }

    fn detects(&self, smtc_title: &str, smtc_app_id: &str) -> bool {
        // YouTube comes through SMTC via Chrome. Cheap gate: the SMTC
        // source is a Chromium browser AND the title or page state
        // suggests YouTube. We don't have a stronger SMTC signal than
        // "is this Chrome?" — the probe's read() does the real check.
        let app = smtc_app_id.to_lowercase();
        let is_chromium = app.contains("chrome")
            || app.contains("msedge")
            || app.contains("edge")
            || app.contains("brave")
            || app.contains("opera")
            || app.contains("vivaldi");
        if !is_chromium { return false; }
        // Heuristic: the SMTC title is something YouTube-publishing.
        // YouTube publishes via MediaSession to SMTC, so the title is
        // the video name. Without a more specific signal, just gate
        // on "Chromium is the source" — the actual ad detection is in read().
        !smtc_title.trim().is_empty()
    }

    fn read(&self) -> anyhow::Result<Option<WebBridgeTrack>> {
        // 1. Enumerate Chrome tabs; find one whose URL or title contains "youtube.com/watch".
        // 2. Re-anchor through element_from_handle to wake the accessibility tree.
        // 3. DFS for Text nodes that contain ad markers; also capture timer-shaped text.
        // 4. Classify via classify_youtube_state; if is_ad, return ad WebBridgeTrack.

        let hwnd = match find_youtube_watch_window() {
            Some(h) => h,
            None => return Ok(None),
        };

        let (texts, timer_text) = walk_for_ad_markers_and_timer(hwnd)?;
        let state = classify_youtube_state(&texts, timer_text.as_deref());

        if state.is_ad {
            let now_unix_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);

            return Ok(Some(WebBridgeTrack {
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                source: "youtube-web".into(),
                last_seen_unix_ms: now_unix_ms,
                position_ms: state.position_ms,
                state: None, // YouTube state continues through SMTC
                is_ad: true,
                duration_ms: state.duration_ms.or(Some(30_000)),
            }));
        }

        // Not an ad — return None so the bridge doesn't override
        // SMTC's normal YouTube metadata.
        Ok(None)
    }
}

// Helpers — both stubs that delegate to existing web_bridge.rs UIA scaffolding.
// Implementation specifics follow the same patterns as PandoraProbe.

fn find_youtube_watch_window() -> Option<windows::Win32::Foundation::HWND> {
    // Reuse web_bridge::enum_chrome_tabs (already used by PandoraProbe).
    // Filter for window titles containing "YouTube" (Chrome includes the page
    // title in the window title for the active tab). For non-active tabs the
    // UIA tree walk would need a different strategy — out of scope for v0.12.
    crate::web_bridge::find_chromium_window_with_title_substring("YouTube")
}

fn walk_for_ad_markers_and_timer(
    hwnd: windows::Win32::Foundation::HWND,
) -> anyhow::Result<(Vec<String>, Option<String>)> {
    use uiautomation::UIAutomation;
    use regex::Regex;
    use std::sync::OnceLock;

    static TIMER_RE: OnceLock<Regex> = OnceLock::new();
    let timer_re = TIMER_RE.get_or_init(|| Regex::new(r"^\d+:\d{2}( / \d+:\d{2})?$").unwrap());

    let automation = UIAutomation::new()?;
    let elem = automation.element_from_handle((hwnd.0 as isize).into())?;

    let mut texts: Vec<String> = Vec::new();
    let mut timer: Option<String> = None;

    fn walk(
        node: &uiautomation::UIElement,
        timer_re: &Regex,
        texts: &mut Vec<String>,
        timer: &mut Option<String>,
    ) -> anyhow::Result<()> {
        if let Ok(name) = node.get_name() {
            if timer_re.is_match(name.trim()) {
                if timer.is_none() { *timer = Some(name.trim().to_string()); }
            }
            // Cap the text collection to avoid runaway memory on YouTube's
            // verbose accessibility tree. 200 entries is plenty for ad-marker
            // matching; the markers we look for are all near the player.
            if texts.len() < 200 {
                texts.push(name);
            }
        }
        // Children
        let walker = uiautomation::core::UITreeWalker::new(); // (verify API: existing pandora_desktop.rs has the reference shape)
        let mut child = walker.get_first_child(node).ok();
        while let Some(c) = child {
            walk(&c, timer_re, texts, timer)?;
            child = walker.get_next_sibling(&c).ok();
        }
        Ok(())
    }

    walk(&elem, timer_re, &mut texts, &mut timer)?;
    Ok((texts, timer))
}
```

NOTE: the exact `uiautomation` crate API for the tree walker matches what `pandora_desktop.rs` already uses. The agent implementing this step MUST read the existing walker invocation in `pandora_desktop.rs` (around line 161-200) and mirror that exact pattern. The pseudo-code above shows shape; the agent fills the precise API surface.

Also add `find_chromium_window_with_title_substring` to `web_bridge.rs` (alongside the existing `read_window_title` / process-name helpers) as a small helper:

```rust
pub(crate) fn find_chromium_window_with_title_substring(needle: &str) -> Option<HWND> {
    // Iterate top-level visible windows whose owning process is a Chromium
    // browser AND whose title contains `needle`. Returns the first match.
    // Existing PandoraProbe code has this enumeration shape — extract it
    // into a reusable helper if not already.
    todo!("extract from PandoraProbe's existing enum_windows logic")
}
```

(That `todo!` is intentional — the agent must extract from the existing PandoraProbe logic. It's not the kind of placeholder this plan rejects; it's a precise marker for "lift the existing pattern into a helper.")

- [ ] **Step 5: Register YouTubeProbe in PROBES**

Edit `src-tauri/src/web_bridge.rs::PROBES` (around line 186):

```rust
static PROBES: &[&dyn WebPlayerProbe] = &[
    &PandoraProbe,
    &crate::pandora_desktop::PandoraDesktopProbe,
    &crate::youtube_bridge::YouTubeProbe,
];
```

- [ ] **Step 6: Add Settings toggle**

Edit `src-tauri/src/settings.rs`. Find the Settings struct (search for `pub struct Settings`). Add:

```rust
pub struct Settings {
    // ... existing fields ...
    #[serde(default = "default_ad_break_promos_enabled")]
    pub ad_break_promos_enabled: bool,
}

fn default_ad_break_promos_enabled() -> bool { true }
```

Find the Default impl (or wherever defaults are constructed) and ensure `ad_break_promos_enabled: true` is set.

- [ ] **Step 7: Mirror to frontend Settings type**

Edit `src/types.ts`:

```ts
export type Settings = {
  // ... existing fields ...
  ad_break_promos_enabled: boolean;
};
```

And in `src/Overlay.tsx`, find `DEFAULT_SETTINGS`:

```ts
const DEFAULT_SETTINGS: Settings = {
  // ... existing fields ...
  ad_break_promos_enabled: true,
};
```

- [ ] **Step 8: Add the settings checkbox**

Edit `src/Settings.tsx`. Find an existing checkbox setting (e.g. `show_album_art`) and copy the pattern. Add a new checkbox near the Overlay section:

```tsx
<label>
  <input
    type="checkbox"
    checked={settings.ad_break_promos_enabled}
    onChange={(e) => update({ ad_break_promos_enabled: e.target.checked })}
  />
  Show SYVR promo cards during ad breaks
</label>
```

(Use whatever the actual update function / form-control pattern in the file is — read a few lines around an existing checkbox first.)

- [ ] **Step 9: Gate PromoCard rendering on the setting**

In `src/Overlay.tsx`, the three layout branches each have `lyrics?.status === "ad" ? <PromoCard ... /> : ...`. Update each to:

```tsx
{lyrics?.status === "ad" && settings.ad_break_promos_enabled ? (
  <PromoCard ... />
) : lyrics?.status === "ad" ? (
  // Neutral "Ad break" placeholder when promos are disabled
  <div style={{ color: effectiveTextColorDim, fontSize: settingsForRender.font_size_px * 0.6, textAlign: "center" }}>
    Ad break
  </div>
) : (
  // Normal lyric rendering
  ...
)}
```

- [ ] **Step 10: Create the production promos.json on syvr-site**

```bash
cd D:/Work/App_Projects/All_Projects/Websites/sites/syvr-site
mkdir -p public/hum
```

Create `public/hum/promos.json` with a real product list. Wes can refine; suggested starter content:

```json
{
  "version": 1,
  "promos": [
    {
      "id": "trellis",
      "product_name": "Trellis",
      "tagline": "Guided AI creative platform.",
      "url": "https://trellis.syvr.dev",
      "weight": 1,
      "active": true,
      "cta_text": "Try free"
    },
    {
      "id": "stub",
      "product_name": "Stub",
      "tagline": "Track your business finances without the spreadsheet hell.",
      "url": "https://app.syvrstudios.com",
      "weight": 1,
      "active": true
    },
    {
      "id": "simsweep",
      "product_name": "SimSweep",
      "tagline": "The Sims 4 mod manager that actually works.",
      "url": "https://simsweep.com",
      "weight": 1,
      "active": true
    },
    {
      "id": "syvr-studios",
      "product_name": "SYVR Studios",
      "tagline": "Tools and apps from the makers of Hum.",
      "url": "https://syvrstudios.com",
      "weight": 1,
      "active": true
    }
  ]
}
```

Commit to syvr-site:

```bash
cd D:/Work/App_Projects/All_Projects/Websites/sites/syvr-site
git add public/hum/promos.json
git commit -m "hum: add ad-break promos.json for v0.12.0 launch"
git push
```

Wait for the Vercel deploy to go live. Verify `curl https://syvrstudios.com/hum/promos.json` returns the JSON.

- [ ] **Step 11: Bump version to 0.12.0 (drop -rc suffix)**

Edit `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` — change `0.12.0-rc8` to `0.12.0`.

- [ ] **Step 12: Manual end-to-end verification — all four sources**

Run `pnpm tauri dev`. Test the matrix:

| Source | Steps | Expected |
|--------|-------|----------|
| Spotify free | Play music, wait for an ad | PromoCard appears, real promos rotate, click opens browser |
| Pandora web (Chrome free) | Open pandora.com, play, wait for an ad | Same |
| Pandora desktop (free MS Store app) | Open Pandora.exe, play, wait for an ad | Same |
| YouTube web (Chrome) | Open a YouTube video that plays mid-roll ads OR a video with pre-roll ads | PromoCard appears during the ad |
| Spotify premium (if accessible) | Play music, never see an ad | No regression — normal lyrics |
| Settings off | Toggle off `ad_break_promos_enabled`, trigger an ad | Just shows "Ad break" centered text, no promo card |

- [ ] **Step 13: Add final CHANGELOG entry (v0.12.0 release notes)**

```markdown
## [0.12.0] - 2026-05-22

### Added — Ad-break detection + SYVR cross-promo overlay (full feature)

This is the consolidated user-facing release entry for v0.12.0. Sub-entries 0.12.0-rc1 through 0.12.0-rc8 above this entry document the per-commit work; the bullets below summarize what's new for the user.

- **Ad-break detection across Spotify, Pandora web, Pandora desktop, and YouTube web.** When an ad break is detected, the overlay's lyric area is replaced with a rotating SYVR Studios product promo card. The right-side metadata column stays visible so users see how much of the ad is left via the progress bar — the source-app badge swaps to an amber `AD BREAK` chip in place of `SPOTIFY` / `PANDORA` / `CHROME`. The Artist · Song · Album line is hidden during ads (showing the previous track's title would be misleading; the AD BREAK chip + promo card already explain what's happening).

- **Promo cards rotate one product per ad break, weighted-random with last-shown cooldown.** Each card shows: a small "Brought to you by SYVR Studios" supertitle, an optional 32×32 product icon, the product name (same size as a lyric line), a dim tagline, and a clickable CTA. Default CTA is `Learn more →`; per-promo `cta_text` overrides are supported. Clicking anywhere on the card opens the promo's URL in the default browser. In edit mode the card is a window drag region instead of a click handler, mirroring the rest of the overlay's chrome behavior.

- **Promo list lives at `https://syvrstudios.com/hum/promos.json` — hot-swappable without a Hum release.** Updates to the file go live on the next refresh (every 6 hours; immediate on app launch). Disk-cached at `%APPDATA%\com.syvr.hum\promos.json` so the first ad break of a session always has something to show even before the network call returns. Bundled `default_promos.json` ships with the app as the final fallback.

- **New Settings toggle: "Show SYVR promo cards during ad breaks"** (default on). Toggling off shows a neutral "Ad break" centered text in place of the promo card. The progress bar + AD BREAK chip still show ad timing regardless.

### Limitations

- **YouTube ad detection requires the YouTube tab to be the foreground Chrome window.** Background-tab YouTube ads are not detected. Implementation reuses the same UIA tree-walking pattern as the Pandora bridges — the Chromium accessibility tree is only fully populated for the active tab.
- **Pandora ad position interpolation falls back to a 30-second default duration** when the ad countdown widget can't be read from the UIA tree. The progress bar will jump to "complete" when the next real track is detected, which is acceptable for free-tier Pandora (most ads are 30s anyway).
- **Spotify Premium users never see ads** so the promo card path is unreachable for them. Not a bug — by design.

### Phase 2 (deferred — premium feature)

User-supplied promos are designed for but not built. The architecture supports them (the `PromoSource` trait, the in-memory pool model), so they slot in cleanly as a paid feature for streamers and small-business owners who want their own promo content during music ad breaks. Tracking in the spec at `docs/superpowers/specs/2026-05-22-hum-ad-break-detection-design.md`.
```

- [ ] **Step 14: Commit and merge**

```bash
cd D:/Work/App_Projects/All_Projects/lyric-overlay
cargo test
pnpm typecheck
git add src-tauri/src/youtube_bridge.rs src-tauri/src/web_bridge.rs src-tauri/src/lib.rs src-tauri/src/settings.rs src/types.ts src/Overlay.tsx src/Settings.tsx package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/CHANGELOG.md
git commit -m "ad-detect(9/9): YouTube probe + Settings toggle + v0.12.0 release"
```

DO NOT push (Tauri desktop policy).

---

## Closing checks

After all 9 tasks ship, run one final audit:

- [ ] `cargo test` — every test passes.
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` — zero warnings.
- [ ] `pnpm typecheck` — clean.
- [ ] Read each file modified across all 9 commits with fresh eyes (per CLAUDE.md's "audit after every phase").
- [ ] Verify the version is at exactly `0.12.0` across `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`.
- [ ] Verify the production `https://syvrstudios.com/hum/promos.json` returns 200 with the schema this plan defined.
- [ ] Verify the bundled default fallback is included in `src-tauri/resources/default_promos.json` AND the `tauri.conf.json` bundle config includes `resources/*`.
- [ ] Verify NONE of the 9 commits accidentally push to origin (Tauri desktop policy).
- [ ] Write a session summary to `docs/summaries/YYYY-MM-DD_HHMM_hum-ad-break-detection-shipped.md` per the project's summary rules.
