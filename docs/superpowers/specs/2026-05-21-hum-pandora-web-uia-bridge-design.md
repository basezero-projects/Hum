# Hum — Pandora web bridge via Windows UI Automation

- **Date:** 2026-05-21
- **Author:** Claude Opus 4.7 (1M context), with Wes
- **Status:** Approved for implementation
- **Project:** Hum (`D:\Work\App_Projects\All_Projects\lyric-overlay\`)
- **Slice:** v0.10.22 (Mica/acrylic spec deferred to a later slice)
- **Predecessor session:** `docs/summaries/2026-05-21_1149_hum-visual-polish-plus-lyric-resolver-rebuild.md` + v0.10.21 commit `4af5d2a` + clippy cleanup `137222d`

---

## Goal

Make lyric lookup work when the user is listening on **pandora.com** in Chrome, with zero additional setup steps for the user. Hum already works for any source that exposes proper Media Session metadata (Spotify Web, YouTube, YouTube Music, Apple Music Web, iTunes desktop). Pandora.com is the first known case where the source publishes audio to Windows SMTC but doesn't call `navigator.mediaSession.metadata`, so SMTC fills the title slot with the browser tab's HTML `<title>` ("Today's Hits Radio - Now Playing on Pandora") and the artist + album come through empty.

## Non-goals

- Browser extensions, userscripts, Tampermonkey, or any user-installed component. Hum is install-and-go; that bar is non-negotiable.
- Chrome launched with `--remote-debugging-port` or any non-default flag.
- macOS / Linux. Hum is Windows-only via SMTC.
- Pandora desktop / mobile / Microsoft Store app. Web player only — that's the case actually broken right now.
- Solving the broader "any web player without Media Session" problem in one shot. The infrastructure is reusable per-probe, but only the Pandora probe ships in this slice. SoundCloud / Bandcamp / NPR / etc. can land later as one-file probes against the same trait.
- The hover → "more info" expand panel with artist bio + tour dates + ticket affiliate links. That's a separate slice queued for after this one ships — see the out-of-scope follow-up list at the bottom of this spec.

## Why UI Automation specifically

Windows ships UI Automation (UIA), the same accessibility API that screen readers like NVDA and Narrator use. Chromium browsers expose their full DOM to UIA on demand. When an external process queries Chrome via UIA, Chromium enables its accessibility tree internally — no user prompt, no flag, no extension. Hum can walk this tree from a Tauri/Rust process and extract whatever Pandora's now-playing widget is rendering, with the same trust level as a screen reader running for a visually impaired user.

This is the only Windows API that gives Hum a structured read of arbitrary Chromium tab content with zero user setup.

## Architecture

### Layer summary

The existing track-resolution path is:

```
SMTC session → smtc.rs → CurrentTrack snapshot → lyrics.rs resolve_lyrics → overlay
```

The new path adds a sideband that can override the SMTC title/artist/album when a web-bridge probe matches:

```
SMTC session → smtc.rs → CurrentTrack snapshot ─┬─→ lyrics.rs resolve_lyrics → overlay
                                                │
                                                └─→ web_bridge.rs probe loop
                                                     │ (when SMTC source = Chrome
                                                     │  AND title matches a probe)
                                                     ↓
                                              UIA tree read of matching
                                              Chrome window → WebBridgeTrack
                                                     │
                                                     ↓
                                              shared cache (last_seen_unix_ms)
                                                     │
                                              consulted by resolve_lyrics
                                              BEFORE using the SMTC title
```

The probe loop is purely additive. Sources where SMTC's title/artist/album are correct (YouTube, Spotify, iTunes, anything that uses Media Session API) **never enter the probe path** because no probe's `detects()` matcher fires for them.

### Module layout

- **New: `src-tauri/src/web_bridge.rs`**
  - Trait `WebPlayerProbe` with three methods:
    - `name() -> &'static str` — short identifier, e.g. `"pandora-web"`.
    - `detects(smtc_title: &str, smtc_app_id: &str) -> bool` — fast string-match gate. Returns true only for the specific signature the probe knows how to read.
    - `read(automation: &UIAutomation) -> anyhow::Result<Option<WebBridgeTrack>>` — walks the UIA tree of the matching Chrome window(s), extracts a `WebBridgeTrack` or returns `Ok(None)` when no readable widget was found.
  - Struct `PandoraProbe;` (the first concrete impl).
  - Struct `WebBridgeTrack { title, artist, album, source, last_seen_unix_ms }`.
  - `SharedWebBridge = Arc<RwLock<Option<WebBridgeTrack>>>` — the live cache the resolver consults.
  - `start(app, snapshot, shared_bridge)` — spawns a background task that polls the active SMTC snapshot, picks a matching probe (if any), runs its `read()`, and updates the cache. Runs every 2s when a probe is active, otherwise idle (no UIA calls at all).
  - Internal helper `find_chrome_windows_matching(predicate)` that enumerates top-level Chrome windows by process name + window title.

- **New dependency in `src-tauri/Cargo.toml`:** `uiautomation = "0.x"`. Higher-level safe wrapper around `windows::Win32::UI::Accessibility::*` — has tree-walking helpers, caching, and pattern matchers that would otherwise be ~500 lines of raw COM code. Already on crates.io, mature, MIT-licensed.

- **Edit: `src-tauri/src/lyrics.rs`**
  - Top of `resolve_lyrics`, after reading the SMTC snapshot but before constructing the cache key + cleaning the title: consult the shared web-bridge cache. If a `WebBridgeTrack` exists AND its `last_seen_unix_ms` is within the last ~5 seconds AND its `title` is non-empty, replace `track.title` / `track.artist` / `track.album` with the bridge values before continuing. `track.duration_ms` is left as SMTC reported it — Pandora's audio duration is in SMTC; only the metadata text is wrong.
  - Cache key generation continues to work without changes — it'll just be keyed on the bridge-supplied title now, which is the canonical song title.

- **Edit: `src-tauri/src/lib.rs`**
  - Add `let shared_bridge = web_bridge::SharedWebBridge::default();` near the other shared state.
  - `app.manage(shared_bridge.clone());`
  - `web_bridge::start(app.handle().clone(), snapshot.clone(), shared_bridge);` after `smtc::start(...)`.

- **No changes** to `smtc.rs` (the SMTC reader keeps working as-is — its output is now consumed by both `lyrics.rs` and `web_bridge.rs`).

### Pandora probe details

`PandoraProbe::detects(title, app_id) -> bool`:

```
app_id.contains("Chrome")       // SourceAppUserModelId for Chrome contains "Chrome.exe"
  && title.ends_with("Now Playing on Pandora")
```

The trailing pattern is what Pandora puts in their `<title>` element. The check is intentionally precise — we don't want to false-positive on "Pandora's Box" by some artist or other titles containing the word "pandora."

`PandoraProbe::read(automation)`:

1. Enumerate top-level windows whose owning process is `chrome.exe`. Filter to those whose window title ends with `"Now Playing on Pandora"`. If zero windows match → return `Ok(None)`.
2. For each matching window, take its `IUIAutomationElement` root via `UIAutomation::element_from_handle(hwnd)`.
3. Search the subtree for the now-playing widget. The exact selector chain — combinations of `Name`, `LocalizedControlType`, `AutomationId`, and tree-walker direction — has to be determined during implementation by running `inspect.exe` (Microsoft's UIA inspector, ships in the Windows SDK) against a live Pandora session. The walker MUST use a stable property like `LocalizedControlType` ("text", "link") plus relative position rather than absolute paths or React-generated class-derived attributes.
4. Extract three text values:
   - Track title — the large bold text below the album art (in the screenshot: "Man I Need" / "So Easy (To Fall In Love)" / "Choosin' Texas")
   - Artist — the smaller text below the title, before a `-` separator
   - Album — the text after the `-` separator (in the screenshot: "The Art of Loving", "Dandelion")
5. Return `Ok(Some(WebBridgeTrack { ... }))` if all three (or at least title) read cleanly, else `Ok(None)` and log the probe-side failure for diagnostics.

The full element-path discovery is **an implementation activity, not a design activity**, because there's no substitute for running `inspect.exe` on the live page. The design commits to the trait shape and the polling lifecycle; the exact UIA selectors land during implementation. Risk if Pandora's React tree turns out to be hostile to UIA (e.g., labels delivered via aria-live regions only, or text smeared across many siblings): documented in the failure-mode section below.

### Polling lifecycle

The probe loop runs as a single async task spawned at startup:

```
loop {
  let snap = snapshot.read().await.clone();
  let active_probe = PROBES.iter().find(|p| p.detects(&snap.title, &snap.source_app_id));
  match active_probe {
    Some(p) => {
      let result = tokio::task::spawn_blocking(move || p.read(&automation)).await;
      match result {
        Ok(Ok(Some(track))) => *shared_bridge.write().await = Some(track),
        Ok(Ok(None)) => { /* probe ran but found nothing — keep last value until stale */ },
        Ok(Err(e)) | Err(e) => eprintln!("[web_bridge] probe '{}' read failed: {e:#}", p.name()),
      }
      tokio::time::sleep(Duration::from_secs(2)).await;
    }
    None => {
      // No probe matches → idle until SMTC changes. Wait on a notify or just sleep 5s.
      // Either way, do NOT touch UIAutomation at all — zero cost when not active.
      tokio::time::sleep(Duration::from_secs(5)).await;
    }
  }
}
```

Plus a fast-path: when `smtc.rs` emits `Msg::MediaChanged` for a Pandora session, immediately re-run the probe (don't wait for the 2s tick). Implementation: an `mpsc::Sender<()>` from `smtc.rs` into `web_bridge.rs` that the probe loop selects on alongside the sleep timer.

### Resolver integration point

`lyrics.rs::resolve_lyrics`, top of the function (around line 227 where `clean_title` is called):

```rust
let (effective_title, effective_artist, effective_album) = match shared_bridge.read().await.as_ref() {
    Some(b) if !b.title.trim().is_empty()
              && now_unix_ms() - b.last_seen_unix_ms < 5_000 => {
        (b.title.clone(), b.artist.clone(), b.album.clone())
    }
    _ => (track.title.clone(), track.artist.clone(), track.album.clone()),
};

let cleaned_title = clean_title(&effective_title);
let cleaned_artist = clean_artist(&effective_artist);
// ... rest of resolve_lyrics uses cleaned_title/cleaned_artist/effective_album ...
```

Effects on existing sources:

- **YouTube** — Chrome SMTC, title is the YouTube video title. No probe matches (title doesn't end with "Now Playing on Pandora"). Bridge cache is empty → fallback to SMTC values → same behavior as today.
- **Spotify Web** — Chrome SMTC, Media Session sets title to song name. Same — no match, no bridge.
- **iTunes / Apple Music desktop** — non-Chrome `source_app_id`. Pandora probe's first gate (`app_id.contains("Chrome")`) fails → no probe runs → unchanged.
- **Pandora.com in Chrome** — Probe matches. Bridge cache populated every 2s with real track info. Resolver sees the real title; v0.10.21's cleaner pipeline still runs on the bridge values for safety; `pick_best` searches LRCLib with the real song.

### Album art handling

The current `spawn_art_fetch` in `smtc.rs` reads the SMTC thumbnail, which for Pandora-in-Chrome is the Chrome browser favicon. The blurred-album-art treatment then makes it look like a rainbow mess (visible in the second Pandora screenshot earlier this session).

**Out of scope for v0.10.22 implementation but mentioned for visibility**: a follow-up could read the actual album art img URL from the UIA tree (or scrape `src` from the `<img>` Pandora renders) and feed that into the art pipeline. For this slice, the SMTC thumbnail stays as-is — the user sees the Chrome favicon as art for Pandora tracks. Lyrics work; art looks generic. Acceptable for v0.10.22; revisit if it stings.

## Failure modes

### Pandora's React tree is hostile to UIA

If `inspect.exe` shows that Pandora delivers track text via mechanisms UIA can't reliably reach (canvas-rendered text, aria-live-only updates that don't persist in the tree, deeply unstable React keys), then the probe can't read it. Mitigation hierarchy:

1. Try multiple selector strategies (by text pattern matching siblings of the album-art image; by tree-walking from a known anchor like the play/pause button; by `Name`-attribute fuzzy match on the visible labels).
2. If all selectors fail, fall back to the **Option-A graceful-failure path** for Pandora specifically: detect the SMTC pattern, suppress the misleading lookup, show `"Pandora web — track info unavailable"` in the overlay.

The graceful-failure path is shipped regardless (see below) so that the worst-case Pandora UX is honest rather than confused. If the UIA read works, great; if it doesn't, at least we're not lying to the user.

### Chrome accessibility doesn't enable

In rare cases Chrome's lazy accessibility enablement misfires. Workaround: send the Chrome window an `WM_GETOBJECT` message with `OBJID_CLIENT` before the first UIA query — this is the canonical way to force Chromium to populate the tree. The `uiautomation` crate's `element_from_handle()` already does this internally on most versions, but if needed we wrap it explicitly.

### Pandora DOM reshuffles between releases

The probe's UIA selectors are coupled to Pandora's specific tree shape. If Pandora ships a redesign that breaks the selectors, the probe stops returning data and falls into the graceful-failure path automatically. Mitigation: log the probe's `read()` failures verbose so we can see "probe ran but found no track" vs "probe matched and returned a track" in the dev console. Re-tuning the selectors after a Pandora redesign is a small re-implementation.

### Performance — UIA tree walks are not free

Walking a complex SPA's UIA tree takes 50-300ms typically. We're polling every 2s when active, on a `spawn_blocking` task — won't stall the main loop or the overlay UI. CPU cost is real but not noticeable on modern hardware. Budget: if a Pandora probe read consistently exceeds 500ms, that's a flag to optimize the selectors (use cached element refs, restrict the scope of the tree walker).

### Probe loop is idle when no probe matches

When the user is listening on YouTube / Spotify / iTunes / anything else, the probe loop checks the snapshot every 5 seconds and finds no matching probe. Zero UIA calls fire during this state — the loop is essentially a `tokio::time::sleep`. Validated by the explicit no-UIA branch in the polling pseudocode above.

### The "wrong window" risk

User has two Chrome windows, one playing Pandora and one with a different tab whose title happens to end in "Now Playing on Pandora" (extremely unlikely but theoretically possible — e.g., a search result page). The probe enumerates **all** matching windows and tries to read from each; whichever returns first wins. In the pathological multi-window case, the probe might briefly read from the wrong tab. Mitigation: prefer windows where the URL ends with `pandora.com/...` (UIA exposes Chrome's address bar contents). Implementation detail, not a design blocker.

## Graceful-failure fallback ships unconditionally

Independent of whether the UIA probe ultimately reads cleanly, the SMTC pattern detection ships as its own protection:

- In `resolve_lyrics`, when the bridge cache is empty/stale AND the SMTC title matches a known unreliable-source pattern (Pandora today, others later), skip the lyric lookup entirely and return a new `CachedLyrics::Unsupported` variant. The overlay displays `"Pandora web — track info unavailable"` or similar instead of `"♪ no lyrics for Today's Hits Radio - Now Playing on Pandora"`.

This is the existing Option-A from the earlier session conversation. Bundling it inside this same slice keeps Pandora's behavior coherent: when UIA works, real lyrics; when UIA fails, an honest status. The user is never told "no lyrics" because of a Pandora-side limitation that has nothing to do with the song.

The `CachedLyrics::Unsupported` variant is a new enum variant joining `Synced` / `Plain` / `Instrumental` / `NotFound`. Overlay rendering treats it like `NotFound` but uses a different copy string keyed by `source`. Like `NotFound` since v0.10.15, `Unsupported` is **never persisted to disk and never cached in memory** — so when a user upgrades Hum (probe selectors improved) or Pandora ships Media Session support, the next playback of the same track gets a fresh resolution attempt rather than a stale "unsupported" verdict.

## Testing & verification plan

Manual verification (Hum has no UI tests; UIA probes are integration-level by nature):

1. `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --lib` — clean.
2. With Pandora.com playing in Chrome: overlay shows real lyrics for the actual song within 2-4 seconds of track change.
3. With Pandora.com playing AND the UIA probe forced-failed (temporary log + early-return for testing): overlay shows `"Pandora web — track info unavailable"`, NOT `"♪ no lyrics for ..."`.
4. With YouTube in Chrome: overlay shows real lyrics (no regression — probe doesn't fire).
5. With Spotify Web in Chrome: same as YouTube.
6. With iTunes desktop: same — probe doesn't fire (non-Chrome app_id).
7. Switch from Pandora tab to YouTube tab in same Chrome window: overlay correctly switches to YouTube's song lyrics within one SMTC `MediaChanged` event.
8. Close the Pandora tab while playing: SMTC session changes, probe disengages, no UIA calls fire when idle.
9. Multiple Chrome windows, one Pandora, one something else: lyrics match the active Pandora session.
10. CPU monitor: with Pandora playing, Hum's CPU usage stays under 1% average over 5 minutes (probe polls 30 times in that window, each <300ms).

## Unit test surface

`web_bridge.rs` is mostly UIA glue that can't be unit-tested without a live Chrome window, but the pure functions ARE testable:

- `PandoraProbe::detects` — table-driven test: a slice of `(title, app_id, expected_bool)` rows. Catches future regressions if the title pattern changes.
- Future probe `detects` methods — same pattern.

The UIA `read` path stays manual-verification territory.

## Files this slice touches (expected)

- `src-tauri/src/web_bridge.rs` (new) — probe trait, Pandora impl, polling loop.
- `src-tauri/src/lyrics.rs` — top-of-`resolve_lyrics` bridge consultation. New `Unsupported` enum variant.
- `src-tauri/src/lib.rs` — startup wiring + `app.manage`.
- `src-tauri/Cargo.toml` — `uiautomation = "0.x"` dependency.
- `src/Overlay.tsx` — rendering for the `Unsupported` status (new copy keyed off `source`).
- `package.json` + `src-tauri/Cargo.toml` + `src-tauri/tauri.conf.json` — version bump to v0.10.22.
- `docs/CHANGELOG.md` — user-visible-outcome-first entry.

## Out-of-scope follow-up candidates

After this slice ships:

1. **Artist info + tour dates + ticket affiliate hover panel** — Wes's idea, see task #8. Bandsintown / Last.fm / MusicBrainz / Spotify Web API as data sources.
2. **SoundCloud probe** — `<title>` typically `"Track Name by Artist | SoundCloud"`. Cleaner than Pandora since the song info IS in the tab title; might not even need UIA, just regex on `track.title`.
3. **Bandcamp probe** — similar to SoundCloud, title has song info.
4. **Album art UIA scrape for Pandora** — read the `<img src>` URL out of the Pandora now-playing widget and feed Hum's art pipeline. Replaces the Chrome favicon with the actual album art.
5. **Mica/acrylic backdrop** — the original v0.10.21 design spec stays committed and waiting. Visual polish resumes once Pandora ships.
6. **Theme-aware luminance override** for Mica per the earlier spec's auto-contrast section.

Each of those is its own design pass when picked up.
