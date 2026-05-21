# Hum artist-info / tour-dates / ticket-affiliate panel — design

- **Date:** 2026-05-21
- **Author:** Claude Opus 4.7 (1M context)
- **Project:** Hum (`D:\Work\App_Projects\All_Projects\lyric-overlay\`)
- **Status:** Approved
- **Predecessor specs:** `2026-05-21-hum-mica-acrylic-backdrop-design.md` (v0.10.23, shipped)

## Goal

Add a Pandora-inspired artist-info panel to Hum: click the album art (or a small "•••" affordance when art is off) → a separate Tauri window opens showing the current artist's bio, photo, similar artists, and upcoming tour dates with affiliate-tagged ticket links. Revenue feature — the ticket-link clicks generate affiliate revenue via Bandsintown's partner program. Single Hum-wide `app_id` embedded in the binary; no per-user setup, no per-artist curation.

## Motivation

- **Revenue:** Bandsintown's partner program pays affiliate splits on ticket clicks routed through Ticketmaster / SeatGeek / AXS / etc. One signup, every Hum user's clicks accrue to Wes.
- **User value:** The overlay shows lyrics; users sometimes want to know who they're listening to, see related artists, or buy tickets. Pandora's "tap the album cover" surface is the canonical pattern.
- **Low-friction extension:** Reuses existing patterns (art-fetch chain in `smtc.rs::fetch_art_via_itunes`, click-through hole pattern from `UpdateBanner`). Doesn't touch the lyrics-rendering hot path.

## Non-goals

- No per-user OAuth flows. Authentication on data sources is limited to static app-wide API keys/IDs embedded in the binary.
- No per-event ticket-vendor integrations (Ticketmaster, StubHub direct). Bandsintown's aggregator handles all routing.
- No social features (sharing, follows, "going" markers).
- No location-aware filtering. Tour dates are shown worldwide. (Future-optional ZIP code filter explicitly deferred — see "Out of scope" below.)
- No Spotify Web API integration. Last.fm + TheAudioDB cover the same surface without OAuth plumbing.
- No artist-discography listings, no album tracklists. Bio + tour dates + similar artists only.
- No mode switch — the affordance is hidden in ghost mode entirely; ghost remains a click-through, minimal-chrome experience.

## Interaction model

### Trigger affordance

- **Primary trigger:** click on the album-art square (`AlbumArtSide` in `three_line` / `single_line` layouts; `AlbumArtBadge` in `full_page` layout). This is the Pandora-inspired affordance Wes referenced; users intuitively understand "tap the cover for more."
- **Discoverability cue:** on hover over the album art (cursor enters the art square), apply a subtle `cursor: pointer` + a 1px gold (`#d4af37`) outline at 0.5 alpha. Tooltip on hover: "Artist info, tour dates, tickets" after a 600ms delay. Tooltip suppressed in `full_page` layout (the badge is tiny, the tooltip would dominate).
- **Fallback affordance when art is hidden/missing:** when `!settings.show_album_art || !albumArt`, render a small 9×9 "•••" dot top-right of the overlay (same wrapper geometry as `UpdateBanner`'s gold dot — alpha 0.7, gold color `#d4af37`). Hover expands "Artist info" label inline; click opens the panel.
- **Click is the trigger, not hover.** Hover-opens-a-window is intrusive because the cursor sweeps the overlay constantly during normal use.

### Mode rules

- **Edit mode:** affordance visible, clickable. Click opens panel.
- **Locked mode:** affordance visible, clickable. Click opens panel.
- **Ghost mode:** affordance hidden. Album art click does nothing. The "•••" fallback dot is not rendered. Rationale: ghost is "minimal chrome, click-through, just lyrics" — adding a clickable surface contradicts the philosophy.

Because ghost mode hides the affordance entirely, no extension of the cursor-poll worker's click-through hole logic is needed.

### Auto-dismiss

The panel auto-closes when the underlying SMTC artist changes to a different artist. Same artist with new track does NOT close the panel (panel content is artist-keyed, not track-keyed). Rationale: stale data + the user moved on.

User-initiated close: X button, ESC key, click-outside (defocus event from the overlay re-gaining focus does NOT count — only an explicit click on the overlay).

## Panel location & window

### Decision

**Separate Tauri peer window**, not inline expansion or in-overlay popover.

Considered:
- **Inline expansion** (overlay grows tall to host the panel) — rejected: fights the auto-resize-to-content model in `Overlay.tsx:424-455`; would either dominate the lyric strip or require disabling auto-resize.
- **In-overlay popover** (absolute-positioned div inside the overlay window) — rejected: Tauri/Win32 windows can't paint outside their own chrome; the popover would be clipped to the overlay's small bounds.
- **Separate peer window** — chosen: full real estate, clean isolation, doesn't touch overlay sizing model. Tauri 2 multi-window is well-supported.

### Window specification

- **Label:** `artist-info` (Tauri window label, distinct from `main`).
- **Size:** 360px × 480px initial. Auto-grows up to 640px tall when content overflows; beyond that, scroll inside the window.
- **Backdrop:** mirror the overlay's `window_backdrop` setting — same Mica/Acrylic/Tabbed Mica/None choice. Visual continuity.
- **Chrome:**
  - `decorations: false` (custom titlebar, matches overlay's lack of OS chrome)
  - `transparent: true` (so the backdrop shows through)
  - `alwaysOnTop: true` (matches overlay)
  - `resizable: false` (fixed-ish; auto-grow only, user can't drag-resize)
  - `skipTaskbar: true` (peer window, not a top-level app)
- **Drag:** the panel header acts as a drag region (`data-tauri-drag-region`).
- **Position:** anchored below the overlay (overlay bottom edge + 8px gap). If overlay sits within 500px of screen bottom, anchor above (overlay top edge - 488px). Centered horizontally on the overlay. Position computed by Rust at open time using `Window::outer_position` + `Window::outer_size` + `Monitor::size`. No auto-reposition when the overlay moves; user can drag the panel independently after opening.
- **Close triggers:**
  - X button (top-right of panel header)
  - ESC keypress when panel focused
  - Click on overlay's main surface (panel listens for `track-changed` to an artist where `clean_artist !== panel.artist` → auto-close)
  - Settings toggle off (closes any open panel)

## Data sources

Final chain — 3 active sources, 1 utility.

| Source | Role | Auth | Endpoint |
|---|---|---|---|
| **Bandsintown** | Tour dates + affiliate ticket links | `app_id` query param (one Hum-wide ID, registered once) | `GET https://rest.bandsintown.com/artists/{name}/events?app_id={ID}` |
| **Last.fm** | Bio prose + similar artists | One free API key embedded in binary | `GET http://ws.audioscrobbler.com/2.0/?method=artist.getInfo&artist={name}&api_key={KEY}&format=json` and `&method=artist.getSimilar` |
| **TheAudioDB** | Artist photo | Free, no auth (use public test key `2` for free tier) | `GET https://www.theaudiodb.com/api/v1/json/2/search.php?s={name}` |
| **MusicBrainz** | `mbid` resolution on ambiguous names | None, polite User-Agent required | `GET https://musicbrainz.org/ws/2/artist?query=artist:{name}&fmt=json` |

### Source priority + fallback flow

1. **First**: parallel fetch of (Bandsintown events) + (Last.fm artist.getInfo) + (Last.fm artist.getSimilar) + (TheAudioDB photo).
2. Each result is independently `Option<T>` — partial success is fine.
3. **MusicBrainz is conditional**: only invoked if Last.fm `getInfo` returned `error: 6` (artist not found) AND Bandsintown returned an artist match. Use MusicBrainz to resolve `mbid`, then retry Last.fm with `&mbid={mbid}`. Skip this branch on the first lookup to keep latency tight.
4. Cache hit short-circuits the whole chain.

### Skipped: Spotify Web API

Explicitly rejected. Even with client-credentials OAuth (no user login), the token-refresh dance adds:
- A token cache + expiry-aware refresh in Rust
- A client_id + client_secret pair to embed (vs. Last.fm's single API key)
- HTTP retry-on-401 logic

For the marginal gain over Last.fm's bio (Spotify's bio is often shorter), it's not worth the surface area. If Last.fm coverage gaps become a real-world problem, revisit then.

### API key & app_id management

- **Bandsintown `app_id`:** placeholder during implementation (`hum-dev`). Live signup at <https://bandsintown.com/partners> before public release. Embedded in `src-tauri/src/artist_info.rs` as a `const`.
- **Last.fm API key:** Wes creates a free Last.fm API account once; key embedded as `const` in `artist_info.rs`. No per-user signup.
- **TheAudioDB:** uses public key `2` (Free tier; documented as the "test/free" key in their API docs). No signup.
- **MusicBrainz:** no key; requires a `User-Agent: hum/{version} ({contact})` header per their TOS. Use `itswesl3y@gmail.com` as contact OR the SYVR Studios support email if Wes prefers.

None of these keys are secrets in any meaningful sense — Last.fm and Bandsintown keys are rate-limit identifiers, not auth tokens. Embedding in the binary is the documented intended use. Do not log them; do not echo them in user-facing strings; but committing them to git is fine.

## Affiliate model

**Bandsintown partner program, single `app_id` embedded in the Hum binary.**

- One-time partner signup at <https://bandsintown.com/partners>. Wes gets a partner `app_id`.
- Every Hum user's ticket clicks carry that ID — affiliate revenue routes to Wes regardless of which user clicked.
- No per-user setup, no per-artist curation, no Ticketmaster/StubHub direct integration.
- Affiliate split is whatever Bandsintown publishes (varies by downstream vendor; typically 5-10%). Revenue scales with active users × tour-clicking behavior.
- Implementation can ship with a placeholder ID `hum-dev` meanwhile; pre-release commit swaps in the live ID once Wes registers.

### Ticket link handling

- Bandsintown's `events` endpoint returns each event's `offers` array. Each offer has a `url` (already affiliate-tagged when the request includes our `app_id`), a `type` ("Tickets" / "Presale"), and a `status` ("available" / "sold out").
- The panel renders a "[Tickets]" button per upcoming event. Click → invoke `open_ticket_url(url)` → Rust calls Tauri's `shell.open(url)` → URL opens in user's default browser.
- Sold-out events: button shows "[Sold Out]", disabled (no click handler), gray styling.

## Caching & rate limits

### Cache strategy

| Field | TTL | Key |
|---|---|---|
| Bio (Last.fm) | Forever (manual invalidate) | `clean_artist` slug |
| Photo (TheAudioDB or Bandsintown fallback) | Forever (manual invalidate) | `clean_artist` slug |
| Similar artists (Last.fm) | Forever (manual invalidate) | `clean_artist` slug |
| Tour dates (Bandsintown) | 12 hours | `clean_artist` slug |
| Resolved `mbid` (MusicBrainz, when used) | Forever | `clean_artist` slug |

### Cache storage

- **Location:** `%APPDATA%\com.syvr.hum\cache\artist\{slug}.json` per artist.
- **Format:** one JSON file per artist, structure:
  ```json
  {
    "version": 1,
    "name": "Shaggy",
    "slug": "shaggy",
    "bio": { "text": "...", "fetched_at_unix_ms": 1716300000000 },
    "photo": { "data_url": "data:image/jpeg;base64,...", "fetched_at_unix_ms": ... },
    "similar_artists": { "list": ["Sean Paul", "..."], "fetched_at_unix_ms": ... },
    "tour_dates": { "events": [...], "fetched_at_unix_ms": ... },
    "mbid": "abc-123-..."
  }
  ```
- **Slug derivation:** `clean_artist` (from `lyrics.rs::clean_artist`) → lowercase → strip non-alphanumeric → collapse whitespace to `-`. Examples: `"Shaggy"` → `"shaggy"`, `"Mötley Crüe"` → `"motley-crue"`, `"AC/DC"` → `"acdc"`.

### Inflight dedup

- `Arc<Mutex<HashMap<String, Arc<Notify>>>>` of in-flight artist keys. Second concurrent request for the same artist waits on the existing fetch's `Notify` rather than firing a duplicate.

### Rate limits

- **Bandsintown:** free tier rate limits are not publicly documented; user reports suggest ~10 req/sec per IP. On-demand per-artist usage stays well below.
- **Last.fm:** 5 req/sec per IP (documented). On-demand per-artist usage stays well below.
- **TheAudioDB:** free key has 2 req/sec limit (documented). On-demand per-artist usage stays well below.
- **MusicBrainz:** 1 req/sec per IP (strict; documented). Only invoked on artist-not-found fallback path; rate limit is not a concern at expected usage.

### Cache invalidation

- No automatic invalidation beyond the 12h tour-dates TTL.
- Manual invalidation only via Settings: a "Clear artist info cache" button under the artist-info panel section. Deletes the entire `cache/artist/` directory tree.
- Cache size is bounded by user listening history × ~10KB per entry. For 1000 unique artists, ~10MB on disk. Acceptable.

## Privacy & location

**Tour dates show all upcoming events worldwide, no geo-filter, no location ask.**

- Bandsintown's `events` endpoint returns the full upcoming list per artist (typically 5-30 events, often fewer).
- User scans the list mentally; dataset is small enough that no filter is needed.
- No IP geolocation. No ZIP code prompt. No fingerprinting. Zero privacy footprint for the user.

Future-optional "filter near my ZIP" toggle in Settings is explicitly deferred — see "Out of scope" below. If real-world usage shows users want it, revisit then.

## Panel content

Top to bottom in the panel window, scroll-overflow inside the window:

### 1. Header

- Artist photo (60×60 round), photo source priority: TheAudioDB → Bandsintown `image_url` → existing album art → silhouette icon.
- Artist name (left-aligned, large — 18px, weight 600).
- X close button (top-right, 12×12, hover-highlights to gold).
- Header is the drag region.

### 2. Bio section

- Last.fm `artist.getInfo` summary text. Strip HTML tags.
- Max ~200 words; truncate at last sentence boundary before the limit.
- "Read more on Last.fm →" link at end (opens artist's Last.fm page via `shell.open`).
- If bio is missing: collapse the section entirely (no "No bio available" placeholder — the section just isn't rendered).

### 3. Similar artists

- Last.fm `artist.getSimilar` top 5-8 names.
- Plain text, comma-separated, prefixed with "**Similar to:** ".
- No click-through in v1. (Future: click a name → switch panel to that artist's info.)
- Collapse section if list is empty.

### 4. Upcoming tour dates

- Up to 10 events, sorted by date ascending.
- Each row:
  - Date: `Mar 5` (month abbrev + day, 2026 year omitted if current year, shown as `Jan 5, 2027` for next year)
  - Location: `Denver, CO` (city + region for US/Canada; city + country for international)
  - Venue: italic, dim text (`Mission Ballroom`)
  - Action: `[Tickets]` button (right-aligned), gold background `#d4af37`, hover-darkens. Disabled "[Sold Out]" variant in gray.
- Empty state when Bandsintown returns zero events: dim italic centered text "No upcoming tour dates."
- More than 10 events: show first 10 + "View all on Bandsintown →" link at bottom of section.

### 5. Footer

Required attribution per TOS of all three data sources:

```
Powered by Bandsintown · Last.fm · TheAudioDB
```

Small (10px), dim, centered. Each name links to the respective service's website via `shell.open`.

### Loading state

While the first fetch is in flight (cache miss):
- Show artist name in header immediately (we already have it from `clean_artist`).
- Show silhouette icon for the photo.
- Show a 3-dot loading indicator in the bio section.
- Tour dates section shows "Loading…".
- Each section independently swaps to its real content as data arrives.

## Failure modes

| Failure | UX |
|---|---|
| All three sources fail (no network) | Panel shows "Couldn't load artist info" + Retry button. Retry re-fires the full fetch chain. |
| Bandsintown 200, empty events array | Bio + similar still render; tour dates section shows "No upcoming tour dates." |
| Last.fm `error: 6` (artist not found) | Trigger MusicBrainz fallback for `mbid` resolution; retry Last.fm with mbid. If retry still misses, collapse bio + similar sections. |
| Last.fm `error: 26` (suspended key) | Log + collapse bio section. Don't surface the key-suspension reason to the user. |
| TheAudioDB photo missing | Fall back to Bandsintown's `image_url` → fall back to album art → silhouette icon. |
| Cache file corrupted (parse error) | Delete the file, treat as cache miss, refetch. Log the corruption. |
| Affiliate URL malformed | Skip rendering the [Tickets] button for that event. |
| `shell.open` fails | Toast "Couldn't open browser" inside the panel for ~2s. |

## Settings additions

| Field | Type | Default | UI |
|---|---|---|---|
| `show_artist_info_panel` | bool | `true` | Settings.tsx: toggle row "Show artist info panel" with hint "Click album art to view artist bio + tour dates" |

No separate "show similar artists" / "show tour dates" sub-toggles — keep the surface minimal; users who want to disable the feature can disable the whole panel.

## Implementation surface

### Rust modules

#### `src-tauri/src/artist_info.rs` (NEW)

```rust
pub struct ArtistInfo {
    pub name: String,
    pub slug: String,
    pub bio: Option<ArtistBio>,
    pub photo_data_url: Option<String>,
    pub similar_artists: Vec<String>,
    pub tour_dates: Vec<TourDate>,
    pub mbid: Option<String>,
}

pub struct ArtistBio {
    pub text: String,
    pub lastfm_url: String,
}

pub struct TourDate {
    pub date_unix_ms: i64,
    pub city: String,
    pub region: String,
    pub country: String,
    pub venue: String,
    pub ticket_url: Option<String>,
    pub status: TicketStatus,
}

pub enum TicketStatus { Available, SoldOut }

pub async fn fetch_artist_info(artist: &str) -> Result<ArtistInfo>;
fn slug_for_artist(artist: &str) -> String;
fn cache_path_for(slug: &str) -> PathBuf;
async fn read_cache(slug: &str) -> Option<CachedArtistInfo>;
async fn write_cache(info: &ArtistInfo) -> Result<()>;

// Source-specific fetchers, all returning Option<T>:
async fn fetch_lastfm_bio(client: &Client, artist: &str) -> Option<ArtistBio>;
async fn fetch_lastfm_similar(client: &Client, artist: &str) -> Vec<String>;
async fn fetch_bandsintown_events(client: &Client, artist: &str) -> Vec<TourDate>;
async fn fetch_theaudiodb_photo(client: &Client, artist: &str) -> Option<String>;
async fn resolve_mbid_musicbrainz(client: &Client, artist: &str) -> Option<String>;
```

Mirrors `smtc.rs::fetch_art_via_itunes` structure: shared `reqwest::Client` builder, fall-through chain, returns `data:image/...;base64,...` for the photo by inlining bytes (so the panel HTML doesn't need to re-fetch). Disk cache + in-memory dedup live here.

#### `src-tauri/src/artist_window.rs` (NEW)

```rust
pub async fn open_artist_panel(app: &AppHandle, artist: &str) -> Result<()>;
pub async fn close_artist_panel(app: &AppHandle) -> Result<()>;
fn compute_position(overlay_window: &Window) -> Result<(i32, i32)>;
```

Creates a `WebviewWindowBuilder` for the `artist-info` label pointing at the panel's HTML entry point. Handles position computation (anchor below or above the overlay based on screen-edge proximity). Listens for `track-changed` events and auto-closes if `clean_artist` changes from the panel's current artist.

#### New Tauri commands (registered in `lib.rs`)

- `get_artist_info(artist: String) -> Result<ArtistInfo, String>`
- `open_artist_panel() -> Result<(), String>` — reads current artist from `SharedSnapshot`, calls `artist_window::open_artist_panel`
- `close_artist_panel() -> Result<(), String>`
- `open_ticket_url(url: String) -> Result<(), String>` — wraps `shell.open(url)` with a URL-host whitelist (`bandsintown.com`, `ticketmaster.com`, `seatgeek.com`, `axs.com`, `livenation.com`, `last.fm`, `theaudiodb.com`) for safety
- `clear_artist_info_cache() -> Result<(), String>` — wipes `cache/artist/` for the Settings "Clear cache" button

### Frontend

#### New entry point: `src/artist-panel/`

- `index.html` — second Tauri webview entry point
- `main.tsx` — React entry mounting `<ArtistPanel />`
- `ArtistPanel.tsx` — fetches via `invoke("get_artist_info", { artist })` on mount, renders header / bio / similar / tour dates / footer
- Styled with inline styles matching the overlay's aesthetic (dark, gold accents, dim text). No new CSS framework.

#### `tauri.conf.json` changes

- The `artist-info` window is created dynamically via `WebviewWindowBuilder` in `artist_window::open_artist_panel` on demand — not declared statically in `tauri.conf.json`. Rationale: the panel doesn't exist until the user clicks the affordance, so a static window declaration would mean a hidden ghost-window in the process tree from app launch.
- Permissions: add `core:webview:allow-create-webview-window`, `core:window:allow-set-position`, `core:window:allow-set-size`, `shell:allow-open`.

#### `Overlay.tsx` changes

- Add `onClick` handler to `AlbumArtSide` + `AlbumArtBadge` that invokes `open_artist_panel` (gated by `settings.show_artist_info_panel && mode !== "ghost"`).
- Add hover state for the discoverability cue (1px gold outline + cursor pointer).
- Add the "•••" dot affordance top-right when art is hidden (mirrors `UpdateBanner` geometry).

#### `src/types.ts` changes

- Add `show_artist_info_panel: boolean` to `Settings`.
- Add `ArtistInfo`, `ArtistBio`, `TourDate`, `TicketStatus` types matching the Rust shape.

#### `src/Settings.tsx` changes

- New row "Show artist info panel" with toggle + hint.
- New button "Clear artist info cache" under the toggle.

#### `Overlay.tsx::DEFAULT_SETTINGS` update

- `show_artist_info_panel: true`.

### Build / version

- Version bump per existing pattern (package.json + Cargo.toml + Cargo.lock + tauri.conf.json).
- CHANGELOG entry leading with the user-visible feature, then technical details.

## Test surface

Mirroring the v0.10.23 + v0.10.24 testing pattern:

### Rust unit tests in `artist_info.rs`

- `slug_for_artist` — covers Mötley Crüe (diacritics), AC/DC (punctuation), single-word, multi-word, leading/trailing whitespace, empty string.
- Cache serialize/deserialize round-trip with all fields populated and with optional fields empty.
- TTL check helper — given `fetched_at` + `now`, returns whether tour dates entry is stale.
- Bio HTML strip — Last.fm bios include `<a>` tags; verify the strip leaves only plain text.

### Rust integration tests

- Skipped for v1. The fetchers hit live APIs; mocking them in tests adds machinery without proportional value. Wes verifies manually with real artists.

### Manual verification checklist

- Click album art with Shaggy playing → panel opens below overlay, populates within 2-3s, shows bio + similar + Bandsintown events.
- Click [Tickets] on an event → browser opens to a `bandsintown.com/event/{id}` URL with `affiliate=hum-dev` (or live ID) in the query string.
- Switch track to a different artist → panel auto-closes.
- Switch track to same artist different song → panel stays open.
- Toggle Settings "Show artist info panel" off → panel closes; album art click no longer triggers anything.
- Cache hit on second open (same artist) — should populate in < 100ms.
- Cache invalidation: change system date to +13h, open same artist → tour dates refetch (other fields stay cached).
- Offline (disable network) → panel shows "Couldn't load artist info" with Retry button.
- Artist with no upcoming tour dates → "No upcoming tour dates" empty state.
- Ghost mode → album art click does nothing; "•••" dot not rendered.
- Backdrop matches overlay setting (Acrylic / Mica / Tabbed / None).
- Position: drag overlay near bottom of screen, open panel → opens above the overlay.

## Open items

- **Bandsintown live `app_id`:** Wes registers at <https://bandsintown.com/partners> before public release. Implementation ships with `hum-dev` placeholder. Tracking this in the spec, not blocking implementation start.
- **MusicBrainz contact email:** confirm whether the `User-Agent` should embed `itswesl3y@gmail.com` or a SYVR Studios support email. Default to `itswesl3y@gmail.com` unless Wes redirects.
- **Window backdrop on the artist-info window:** initially mirror the overlay's `window_backdrop` setting. If the visual is ugly when the overlay uses `None` (no backdrop, transparent), revisit and force at least Acrylic on the panel for readability.

## Out of scope (future enhancements)

- **ZIP-code-based tour-date filtering** — a Settings toggle "Filter near my ZIP" + a ZIP input → filter Bandsintown events to within 100mi. Defer until real-world usage shows demand.
- **Click similar artist names to switch panel content** — natural extension once the panel ships and Wes/users want it.
- **Artist photo zoom-in modal** — click photo → full-size view. Low-priority polish.
- **Sharing** — "Share artist info" button. Out of scope; no social features.
- **Per-event reminder notifications** — "Notify me when tickets go on sale" requires backend infrastructure. Out of scope.
- **User-supplied Last.fm API keys** — if rate limits become an issue, expose a Settings field for users to provide their own key. Not in v1.
- **Local-only cache export/import** — moving artist cache between machines. Not in v1.

## Decisions log

1. **Click, not hover, opens the panel.** Hover-opens-a-window is intrusive given how often the cursor sweeps the overlay.
2. **Album art is the primary trigger, "•••" dot is the fallback.** Reuses an already-visible UI element when present; minimizes new chrome.
3. **Separate Tauri window, not inline.** Overlay's auto-resize-to-content model can't host the panel inline without major restructuring.
4. **Ghost mode hides the affordance entirely.** Aligns with ghost's "minimal chrome, click-through" philosophy. Avoids extending the cursor-poll worker.
5. **Bandsintown is the only affiliate source.** Their aggregator handles Ticketmaster/SeatGeek/AXS routing; per-vendor integrations add curation burden with no revenue upside.
6. **Spotify Web API skipped.** OAuth plumbing isn't worth it when Last.fm covers the same surface with a single static key.
7. **No location-based filtering in v1.** Privacy-perfect + dataset is small enough that mental filtering works.
8. **One Hum-wide affiliate `app_id`.** Zero per-user setup; revenue accrues to Wes regardless of which user clicked.
9. **Bio source is Last.fm only.** Wikipedia-derived, license-friendly, attribution-clean.
10. **Photo source is TheAudioDB primary, multiple fallbacks.** Last.fm deprecated artist images; TheAudioDB still serves them.
11. **Auto-close on artist change, NOT on track change.** Panel is artist-keyed; same-artist track changes shouldn't dismiss it.
12. **Manual cache invalidation only.** TTL only on tour dates (12h); everything else is forever-cache until the user explicitly clears.
13. **Affiliate URL whitelist in the `open_ticket_url` command.** Defends against malformed/malicious URLs in cache poisoning scenarios. Whitelist: `bandsintown.com`, `ticketmaster.com`, `seatgeek.com`, `axs.com`, `livenation.com`, `last.fm`, `theaudiodb.com`.

## Sign-off

Approved by Wes 2026-05-21 via brainstorming session. Spec written by Claude Opus 4.7 (1M context), same day.
