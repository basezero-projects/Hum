# Hum Web Demo — Build Plan

Interactive demo page for syvr.dev where visitors can play any song and see Hum's real-time synced lyrics overlay working live in their browser.

## Stack

Lives in `site-astro/` (Astro 6, static output, Vercel). One new `.astro` page, vanilla JS runtime. No new dependencies. YouTube IFrame API and LRCLib loaded at runtime. Spotify embed is a plain `<iframe>`.

**New file:** `site-astro/src/pages/hum-demo.astro` → `syvr.dev/hum-demo`

---

## Architecture

```
syvr.dev/hum-demo
┌─────────────────────────────────────────────────────┐
│  Header (shared from site)                          │
│  ─────────────────────────────────────────────────  │
│  Hero: "See Hum live."                              │
│    Search bar + source toggle (YouTube / Spotify)   │
│    Result list (max 5, from LRCLib search)          │
│  ─────────────────────────────────────────────────  │
│  Player section (hidden until track selected):      │
│    YouTube IFrame   OR   Spotify embed iframe       │
│  ─────────────────────────────────────────────────  │
│  Overlay preview:                                   │
│    The actual overlay DOM (from streamer_overlay)    │
│    Full-width dark panel, like a stream scene       │
│  ─────────────────────────────────────────────────  │
│  CTA: Download Hum                                  │
└─────────────────────────────────────────────────────┘
```

---

## Two Player Sources

### YouTube (default)
- YouTube IFrame Player API, free, no visitor auth
- Full song playback
- `player.getCurrentTime()` gives position in seconds at 60fps
- **No API key.** User pastes a YouTube link after selecting a song from LRCLib results. Extract video ID from the URL (`/watch?v=` or `youtu.be/`), load via IFrame API.
- The paste step is quick (2 seconds) and eliminates all API keys, quotas, and maintenance

### Spotify
- Spotify Embed (oEmbed), 30-second preview, no auth
- Embed posts `playback_update` messages via `postMessage` with `{ position, duration, isPaused }`
- Between messages (~1s interval), interpolate: `positionMs = Date.now() - lastWallMs`
- Track ID from LRCLib's `spotifyId` field (present on most popular tracks)
- Fallback if missing: paste-URL input

---

## Search Flow

1. User types in search box. Debounce 400ms.
2. Fetch `https://lrclib.net/api/search?q=<query>` (free, unlimited, CORS-friendly).
3. Filter to results where `syncedLyrics` is non-null.
4. Show up to 5 results: `{trackName} — {artistName} ({albumName}, {duration})`.
5. User clicks a result:
   - Parse `syncedLyrics` from the search result directly (no second fetch needed)
   - If YouTube: show a paste input ("Paste a YouTube link for this song"). Extract video ID from URL, load player.
   - If Spotify: use `spotifyId` from LRCLib result → load embed automatically. If `spotifyId` absent, show paste input.
   - Fetch album art from iTunes Search API

**Why LRCLib for search:** Free, unlimited, CORS, same API the desktop app uses. Guarantees every result has synced lyrics available. No API keys needed anywhere.

---

## Lyrics Engine

### LRC Parser
```js
function parseLrc(lrc) {
  const lines = [];
  const lineRe = /^\[(\d+):(\d+\.\d+)\]\s*(.*)$/;
  for (const rawLine of lrc.split('\n')) {
    const m = rawLine.match(lineRe);
    if (!m) continue;
    const timeMs = (parseInt(m[1]) * 60 + parseFloat(m[2])) * 1000;
    const text = m[3].trim();
    if (text) lines.push({ time_ms: Math.round(timeMs), text });
  }
  return lines.sort((a, b) => a.time_ms - b.time_ms);
}
```

### Cursor Computation
Same as desktop overlay and streamer_overlay.html:
```js
function computeCursor(lines, positionMs) {
  let found = -1;
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].time_ms <= positionMs) found = i;
    else break;
  }
  return found;
}
```

Line-level display only (no per-word karaoke sweep). LRCLib doesn't provide word timings. Matches the overlay's existing fallback path.

---

## Album Art

**Primary: iTunes Search API** (free, CORS-enabled, no auth)
```
https://itunes.apple.com/search?term=<artist+track>&entity=song&limit=3&media=music
```
Returns `results[0].artworkUrl100`. Replace `100x100` suffix with `600x600` for hi-res.

**Fallback for YouTube:** `https://img.youtube.com/vi/<videoId>/hqdefault.jpg`

---

## Overlay Rendering

Verbatim copy of DOM + CSS + render/tick functions from `streamer_overlay.html`. Changes:
1. Remove SSE/poll script (data comes from player state)
2. Remove `fetchAndApplySettings()` (no desktop settings server)
3. Remove `onlySource` filter
4. Keep all CSS variables, animations, karaoke markup

The overlay section wraps in a `.demo-stage` container:
```css
.demo-stage {
  background: #111;
  border-radius: 12px;
  overflow: hidden;
  aspect-ratio: 16 / 3;
  position: relative;
  box-shadow: 0 32px 80px -20px rgba(0,0,0,0.8), 0 0 0 1px rgba(255,255,255,0.06);
}
```

---

## Spotify Position Tracking

```js
window.addEventListener('message', (e) => {
  if (e.origin !== 'https://open.spotify.com') return;
  const d = e.data;
  if (d?.type === 'playback_update') {
    state.positionMs = d.payload?.position ?? state.positionMs;
    state.durationMs = d.payload?.duration ?? state.durationMs;
    state.isPlaying = !d.payload?.isPaused;
    if (state.isPlaying) state.lastWallMs = Date.now() - state.positionMs;
  }
});
```

Between messages, rAF tick interpolates: `positionMs = Date.now() - state.lastWallMs`.

---

## Data Flow

```
User types query
  → debounce 400ms
  → GET lrclib.net/api/search?q=<query>
  → filter to syncedLyrics only
  → render result list

User clicks result
  → parseLrc(selectedTrack.syncedLyrics) → state.lyricsLines
  → iTunes Search API → state.artUrl
  → YouTube: user pastes URL → extract videoId → loadYouTube(videoId)
  → Spotify: spotifyId from LRCLib → load embed iframe

rAF tick (60fps):
  → YouTube: positionMs = ytPlayer.getCurrentTime() * 1000
  → Spotify: positionMs = Date.now() - state.lastWallMs (interpolated)
  → cursor = computeCursor(state.lyricsLines, positionMs)
  → if cursor changed: update overlay lines
  → update progress bar, time display
```

---

## Edge Cases

| Case | Handling |
|------|----------|
| No synced lyrics in results | Filter out. Show "No synced lyrics found." |
| LRCLib returns 0 results | Show "Nothing found. Try artist + song title." |
| Invalid YouTube URL pasted | Show "Couldn't find a video ID in that URL. Try a youtube.com or youtu.be link." |
| YouTube video embedding disabled | Detect after 3s, show "Can't embed. Try another." |
| Spotify `spotifyId` missing | Show paste-URL input |
| Spotify preview not available | Embed shows its own error. Suggest YouTube. |
| Plain lyrics only (no sync) | Show text in `#cur`, no scrolling |
| User pauses | rAF stops advancing position. Overlay freezes. |
| Mobile viewport | Switch to taller aspect ratio (16:5), smaller font |

---

## Implementation Sequence

1. **Static skeleton** — `hum-demo.astro` with layout, search bar, source toggle, overlay DOM. Copy logo + SVG assets.
2. **LRCLib search** — Wire search input → debounce → fetch → render results.
3. **YouTube player** — Load IFrame API. Wire paste-URL input → extract video ID → load video. Verify position tracking.
4. **Overlay rendering** — Port render() and tick() from streamer_overlay.html. Connect to state.
5. **Album art** — Wire iTunes Search API → overlay art + blur bg.
6. **Spotify path** — Embed logic + postMessage listener + interpolated position.
7. **Edge cases + polish** — Error handling, mobile layout, CTA section, final styling.
8. **Deploy** — Push (auto-deploys). Verify at syvr.dev/hum-demo.

---

## Key Decisions

| Decision | Choice | Reason |
|----------|--------|--------|
| Framework | Astro (existing site-astro) | Zero new infra, static build, same deploy |
| Runtime JS | Vanilla | Overlay logic already vanilla, simpler to port |
| Search | LRCLib | Free, unlimited, CORS, guarantees lyrics exist |
| YouTube video | Paste-a-link, no API key | Zero quotas, zero maintenance, works forever |
| Spotify position | postMessage + wall-clock interpolation | Only option, embed has no JS API |
| Album art | iTunes Search API | Free, CORS, high quality, no auth |
| Karaoke | Line-level only | LRCLib has no word timings |
| Overlay DOM | Verbatim from streamer_overlay.html | Pixel-identical rendering |

---

## Critical Files for Implementation

- `Hum/src-tauri/src/streamer_overlay.html` — overlay CSS + DOM + render/tick JS to port
- `Websites/sites/syvr-site/site-astro/src/layouts/BaseLayout.astro` — shared header/footer
- `Websites/sites/syvr-site/site-astro/src/styles/global.css` — existing CSS vars
- `Websites/sites/syvr-site/site-astro/astro.config.mjs` — static output config
