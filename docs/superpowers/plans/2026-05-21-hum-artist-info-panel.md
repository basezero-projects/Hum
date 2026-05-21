# Hum Artist-Info / Tour-Dates / Ticket-Affiliate Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Pandora-inspired artist-info panel to Hum — click album art (or a "•••" fallback dot) opens a separate Tauri peer window showing artist bio, similar artists, and upcoming tour dates with Bandsintown-affiliate ticket links.

**Architecture:** New `src-tauri/src/artist_info.rs` owns the fetch chain (Last.fm bio + similar, Bandsintown events, TheAudioDB photo, MusicBrainz mbid fallback) plus a persistent disk cache and in-flight dedup. New `src-tauri/src/artist_window.rs` creates a peer Tauri window on demand via `WebviewWindowBuilder`. New `src/artist-panel/` Vite entry hosts the React panel UI. Single Hum-wide Bandsintown `app_id` embedded in the binary funnels affiliate revenue.

**Tech Stack:** Rust (Tauri 2, tokio, reqwest, serde) + React 19 / TypeScript 5.9 (Vite multi-page) + Bandsintown REST + Last.fm REST + TheAudioDB REST + MusicBrainz REST.

**Spec:** `docs/superpowers/specs/2026-05-21-hum-artist-info-panel-design.md`

**Pattern:** Subagent-driven execution with literal-code task blocks per the v0.10.23 backdrop slice precedent. Mechanical tasks bundled where appropriate; one final review at slice end against the full diff (vs per-task formal review).

---

## File Map

| File | Action | Responsibility |
|---|---|---|
| `src-tauri/src/artist_info.rs` | **Create** | Types, pure helpers, all fetchers, orchestrator, disk cache, in-flight dedup, Tauri commands. |
| `src-tauri/src/artist_window.rs` | **Create** | `open_artist_panel`, `close_artist_panel`, `open_ticket_url`, window position logic. |
| `src-tauri/src/lib.rs` | Modify | `mod` declarations, `.manage(ArtistInfoCache)`, new commands in `invoke_handler!`. |
| `src-tauri/Cargo.toml` | Modify | Add `urlencoding = "2"` if absent. Version bump to `0.11.0`. |
| `src-tauri/tauri.conf.json` | Modify | Version bump to `0.11.0`. Capability additions. |
| `src-tauri/capabilities/default.json` | Modify | Add `artist-info` to windows list; add webview/window/opener permissions. |
| `src/types.ts` | Modify | Add `show_artist_info_panel: boolean` to `Settings`; add `ArtistInfo`, `ArtistBio`, `TourDate`, `TicketStatus` types. |
| `src/Overlay.tsx` | Modify | `DEFAULT_SETTINGS` update; `AlbumArtSide`/`AlbumArtBadge` click handlers; `ArtistInfoDot` fallback. |
| `src/Settings.tsx` | Modify | New toggle row + cache-clear button. |
| `src/artist-panel/index.html` | **Create** | Second Vite entry point HTML. |
| `src/artist-panel/main.tsx` | **Create** | React mount for the panel. |
| `src/artist-panel/ArtistPanel.tsx` | **Create** | Full panel UI component. |
| `vite.config.ts` | Modify | Multi-page `build.rollupOptions.input`. |
| `package.json` | Modify | Version bump to `0.11.0`. |
| `docs/CHANGELOG.md` | Modify | Prepend `## [0.11.0]` entry. |

---

## Task 1 — Rust: `artist_info` module scaffold + pure-function helpers (TDD)

**Files:**
- Create `src-tauri/src/artist_info.rs`
- Modify `src-tauri/src/lib.rs`

- [ ] **Step 1: Create the module with types, pure helpers, and unit tests**

Create `src-tauri/src/artist_info.rs` with the type definitions, the pure-function helpers (`slug_for_artist`, `tour_dates_stale`, `strip_html`), and the test block. Only `fetch_artist_info` is a `todo!()` stub — Task 6 fills that in. Everything else is the real implementation; the tests pin the contract. Write the file with this exact content:

```rust
//! Artist-info fetch chain: Last.fm bio + similar, Bandsintown events,
//! TheAudioDB photo, MusicBrainz mbid fallback. Disk cache + in-flight dedup.

use anyhow::Result;
use serde::{Deserialize, Serialize};

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtistInfo {
    pub name: String,
    pub slug: String,
    pub bio: Option<ArtistBio>,
    pub photo_data_url: Option<String>,
    pub similar_artists: Vec<String>,
    pub tour_dates: Vec<TourDate>,
    pub mbid: Option<String>,
    pub fetched_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArtistBio {
    pub text: String,
    pub lastfm_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TourDate {
    pub date_unix_ms: i64,
    pub city: String,
    pub region: String,
    pub country: String,
    pub venue: String,
    pub ticket_url: Option<String>,
    pub status: TicketStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TicketStatus {
    Available,
    SoldOut,
}

// ── Pure helpers ───────────────────────────────────────────────────────────

/// Derive a URL-safe slug from an artist name.
/// Lowercase, collapse whitespace to single dash, strip everything else
/// except ASCII alphanumerics and dashes. Common Latin diacritics mapped
/// to ASCII before stripping.
pub(crate) fn slug_for_artist(name: &str) -> String {
    // Diacritic → ASCII mapping for common Latin Extended chars.
    let mapped: String = name.chars().map(|c| match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'a',
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => 'i',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => 'u',
        'ý' | 'ÿ' | 'Ý' | 'Ÿ' => 'y',
        'ñ' | 'Ñ' => 'n',
        'ç' | 'Ç' => 'c',
        'ß' => 's',
        'æ' | 'Æ' => 'a',
        'œ' | 'Œ' => 'o',
        'ð' | 'Ð' => 'd',
        'þ' | 'Þ' => 't',
        other => other,
    }).collect();

    let lower = mapped.to_lowercase();

    // Replace whitespace runs with a single dash, keep alphanumerics.
    let mut result = String::new();
    let mut last_was_dash = false;
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
            last_was_dash = false;
        } else if c.is_whitespace() || c == '-' {
            if !last_was_dash && !result.is_empty() {
                result.push('-');
                last_was_dash = true;
            }
        }
        // All other chars (punctuation, non-ASCII) are dropped.
    }
    // Trim trailing dash.
    result.trim_end_matches('-').to_string()
}

/// True if the tour-dates entry is stale (older than 12 hours).
pub(crate) fn tour_dates_stale(fetched_at_unix_ms: i64, now_unix_ms: i64) -> bool {
    const TWELVE_HOURS_MS: i64 = 12 * 3600 * 1000;
    (now_unix_ms - fetched_at_unix_ms) >= TWELVE_HOURS_MS
}

/// Strip HTML tags from a string, keeping tag content.
/// Handles `<a href="...">text</a>`, `<b>text</b>`, etc.
pub(crate) fn strip_html(s: &str) -> String {
    use regex::Regex;
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"<[^>]+>").expect("strip_html regex"));
    // Decode common HTML entities after stripping tags.
    let stripped = re.replace_all(s, "");
    stripped
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Stub — filled in by Task 6.
pub async fn fetch_artist_info(_artist: &str) -> Result<ArtistInfo> {
    todo!("implemented in Task 6")
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_simple() {
        assert_eq!(slug_for_artist("Shaggy"), "shaggy");
    }

    #[test]
    fn slug_diacritics() {
        assert_eq!(slug_for_artist("Mötley Crüe"), "motley-crue");
    }

    #[test]
    fn slug_punctuation_stripped() {
        assert_eq!(slug_for_artist("AC/DC"), "acdc");
    }

    #[test]
    fn slug_punctuation_pnk() {
        assert_eq!(slug_for_artist("  P!nk  "), "pnk");
    }

    #[test]
    fn slug_empty() {
        assert_eq!(slug_for_artist(""), "");
    }

    #[test]
    fn slug_multi_word() {
        assert_eq!(slug_for_artist("The Rolling Stones"), "the-rolling-stones");
    }

    #[test]
    fn slug_leading_trailing_dash() {
        // Leading/trailing non-alphanum should not produce leading/trailing dash.
        assert_eq!(slug_for_artist("---test---"), "test");
    }

    #[test]
    fn tour_dates_fresh() {
        // 0 hours ago — not stale.
        assert!(!tour_dates_stale(1_000_000, 1_000_000));
    }

    #[test]
    fn tour_dates_eleven_hours() {
        let now = 1_000_000_000i64;
        let fetched = now - (11 * 3600 * 1000);
        assert!(!tour_dates_stale(fetched, now));
    }

    #[test]
    fn tour_dates_thirteen_hours() {
        let now = 1_000_000_000i64;
        let fetched = now - (13 * 3600 * 1000);
        assert!(tour_dates_stale(fetched, now));
    }

    #[test]
    fn tour_dates_exactly_twelve_hours() {
        let now = 1_000_000_000i64;
        let fetched = now - (12 * 3600 * 1000);
        // Exactly at the boundary → stale (>=).
        assert!(tour_dates_stale(fetched, now));
    }

    #[test]
    fn strip_html_plain() {
        assert_eq!(strip_html("plain"), "plain");
    }

    #[test]
    fn strip_html_anchor() {
        assert_eq!(strip_html("<a href='x'>link</a>"), "link");
    }

    #[test]
    fn strip_html_bold() {
        assert_eq!(strip_html("text with <b>bold</b>"), "text with bold");
    }

    #[test]
    fn strip_html_entities() {
        assert_eq!(strip_html("rock &amp; roll"), "rock & roll");
    }

    #[test]
    fn strip_html_empty() {
        assert_eq!(strip_html(""), "");
    }
}
```

- [ ] **Step 2: Add `mod artist_info;` to `lib.rs`**

Find the block of `mod` declarations near the top of `src-tauri/src/lib.rs` (currently ending with `mod streamer;`). Add immediately after `mod streamer;`:

```rust
mod artist_info;
```

- [ ] **Step 3: Run tests — expect pass**

```
cd src-tauri && cargo test --lib artist_info::tests
```

Expected: PASS — all 15 tests pass (slug×7, tour_dates×4, strip_html×5). The `fetch_artist_info` stub is not exercised by any test, so its `todo!()` body doesn't trip anything at runtime.

- [ ] **Step 4: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS (clean — `todo!()` in dead `pub async fn` does not trigger dead_code warnings in this context; if it does, add `#[allow(dead_code)]` above `fetch_artist_info`).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/artist_info.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
artist_info: types, slug/TTL/strip-html helpers, unit tests (TDD scaffold)
EOF
)"
```

---

## Task 2 — Rust: Last.fm bio + similar artists fetchers

**Files:**
- Modify `src-tauri/src/artist_info.rs`

Add the Last.fm constants and two async fetcher functions. No new Cargo deps needed — `reqwest` and `serde_json` are already in `Cargo.toml`.

- [ ] **Step 1: Add Last.fm constants and fetchers**

Append to `src-tauri/src/artist_info.rs` after the `fetch_artist_info` stub and before the `#[cfg(test)]` block:

```rust
// ── Last.fm ────────────────────────────────────────────────────────────────

/// Register a free API account at https://www.last.fm/api before public release.
/// Embedding a static key is the documented intended use (rate-limit identifier,
/// not an auth secret).
const LASTFM_API_KEY: &str = "PLACEHOLDER_REPLACE_BEFORE_LAUNCH";
const LASTFM_BASE: &str = "https://ws.audioscrobbler.com/2.0/";

/// Fetch artist bio from Last.fm artist.getInfo.
/// Returns None on artist-not-found (error 6), network failure, or missing fields.
pub(crate) async fn fetch_lastfm_bio(
    client: &reqwest::Client,
    artist: &str,
) -> Option<ArtistBio> {
    let url = reqwest::Url::parse_with_params(
        LASTFM_BASE,
        &[
            ("method", "artist.getInfo"),
            ("artist", artist),
            ("api_key", LASTFM_API_KEY),
            ("format", "json"),
        ],
    )
    .ok()?;

    let resp = client.get(url).send().await.ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;

    // Error 6 = artist not found; error 26 = suspended key. Both → None.
    if body.get("error").is_some() {
        eprintln!(
            "[artist_info] lastfm getInfo error: {}",
            body.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
        );
        return None;
    }

    let artist_obj = body.get("artist")?;
    let raw_bio = artist_obj
        .get("bio")?
        .get("summary")?
        .as_str()
        .unwrap_or("")
        .to_string();
    let lastfm_url = artist_obj
        .get("url")?
        .as_str()
        .unwrap_or("")
        .to_string();

    if lastfm_url.is_empty() {
        return None;
    }

    let mut bio_text = strip_html(&raw_bio);

    // Truncate to last sentence boundary before 1500 chars.
    if bio_text.len() > 1500 {
        // Find last period, ?, or ! before position 1500.
        let cutoff = bio_text[..1500]
            .rfind(|c| matches!(c, '.' | '?' | '!'))
            .map(|i| i + 1)
            .unwrap_or(1500);
        bio_text.truncate(cutoff);
        bio_text = bio_text.trim_end().to_string();
    }

    if bio_text.is_empty() {
        return None;
    }

    Some(ArtistBio { text: bio_text, lastfm_url })
}

/// Fetch similar artists from Last.fm artist.getSimilar (top 8).
/// Returns an empty Vec on any failure.
pub(crate) async fn fetch_lastfm_similar(
    client: &reqwest::Client,
    artist: &str,
) -> Vec<String> {
    let url = match reqwest::Url::parse_with_params(
        LASTFM_BASE,
        &[
            ("method", "artist.getSimilar"),
            ("artist", artist),
            ("api_key", LASTFM_API_KEY),
            ("limit", "8"),
            ("format", "json"),
        ],
    ) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(_) => return vec![],
    };

    if body.get("error").is_some() {
        return vec![];
    }

    body.get("similarartists")
        .and_then(|sa| sa.get("artist"))
        .and_then(|arr| arr.as_array())
        .map(|artists| {
            artists
                .iter()
                .filter_map(|a| a.get("name")?.as_str().map(|s| s.to_string()))
                .take(8)
                .collect()
        })
        .unwrap_or_default()
}
```

- [ ] **Step 2: Verify build**

```
cd src-tauri && cargo check
```

Expected: PASS.

- [ ] **Step 3: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/artist_info.rs
git commit -m "$(cat <<'EOF'
artist_info: Last.fm bio + similar artists fetchers
EOF
)"
```

---

## Task 3 — Rust: Bandsintown events fetcher

**Files:**
- Modify `src-tauri/src/artist_info.rs`
- Possibly modify `src-tauri/Cargo.toml` (check for `urlencoding`)

- [ ] **Step 1: Check whether `urlencoding` is already in Cargo.toml**

```
grep urlencoding src-tauri/Cargo.toml
```

Expected output: no match (it is not currently present). If absent, add it:

Edit `src-tauri/Cargo.toml` `[dependencies]` section. After the `regex = "1"` line, add:

```toml
urlencoding = "2"
```

- [ ] **Step 2: Add Bandsintown constant and fetcher**

Append to `src-tauri/src/artist_info.rs` (before `#[cfg(test)]`):

```rust
// ── Bandsintown ────────────────────────────────────────────────────────────

/// Placeholder. Wes registers at https://bandsintown.com/partners before
/// public release and replaces this with the live partner app_id.
const BANDSINTOWN_APP_ID: &str = "hum-dev";

/// Fetch upcoming events from Bandsintown.
/// Returns events sorted by date ascending. Returns empty Vec on any failure.
pub(crate) async fn fetch_bandsintown_events(
    client: &reqwest::Client,
    artist: &str,
) -> Vec<TourDate> {
    // Artist name goes in the URL path, not as a query param.
    let encoded = urlencoding::encode(artist);
    let url_str = format!(
        "https://rest.bandsintown.com/artists/{}/events?app_id={}",
        encoded, BANDSINTOWN_APP_ID
    );
    let url = match reqwest::Url::parse(&url_str) {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[artist_info] bandsintown fetch failed: {e}");
            return vec![];
        }
    };

    let events: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[artist_info] bandsintown JSON parse failed: {e}");
            return vec![];
        }
    };

    let arr = match events.as_array() {
        Some(a) => a,
        None => return vec![],
    };

    let mut dates: Vec<TourDate> = arr
        .iter()
        .filter_map(|event| parse_bandsintown_event(event))
        .collect();

    // Sort ascending by date.
    dates.sort_by_key(|d| d.date_unix_ms);
    dates
}

fn parse_bandsintown_event(event: &serde_json::Value) -> Option<TourDate> {
    // datetime: ISO8601, e.g. "2026-03-05T20:00:00" (no timezone offset).
    let datetime_str = event.get("datetime")?.as_str()?;
    let date_unix_ms = parse_iso8601_to_unix_ms(datetime_str)?;

    let venue = event.get("venue")?;
    let city = venue
        .get("city")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let region = venue
        .get("region")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let country = venue
        .get("country")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let venue_name = venue
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Find the first "Tickets" offer.
    let mut ticket_url: Option<String> = None;
    let mut status = TicketStatus::Available;
    if let Some(offers) = event.get("offers").and_then(|o| o.as_array()) {
        for offer in offers {
            let offer_type = offer.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if offer_type.eq_ignore_ascii_case("Tickets") {
                if let Some(url) = offer.get("url").and_then(|u| u.as_str()) {
                    if !url.is_empty() {
                        ticket_url = Some(url.to_string());
                    }
                }
                let offer_status = offer
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("available");
                status = if offer_status.eq_ignore_ascii_case("available") {
                    TicketStatus::Available
                } else {
                    TicketStatus::SoldOut
                };
                break;
            }
        }
    }

    Some(TourDate {
        date_unix_ms,
        city,
        region,
        country,
        venue: venue_name,
        ticket_url,
        status,
    })
}

/// Parse Bandsintown ISO8601 datetime string to Unix milliseconds.
/// Input format: "2026-03-05T20:00:00" (no timezone; treat as UTC for sorting purposes).
fn parse_iso8601_to_unix_ms(s: &str) -> Option<i64> {
    // Split at 'T' and parse manually: "2026-03-05" and "20:00:00".
    let (date_part, time_part) = s.split_once('T')?;
    let mut date_parts = date_part.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time_part.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let min: i64 = time_parts.next()?.parse().ok()?;
    let sec_str = time_parts.next().unwrap_or("0");
    let sec: i64 = sec_str.split('.').next()?.parse().ok()?;

    // Days from epoch (1970-01-01). Use the proleptic Gregorian formula.
    // This is accurate for dates in the 2020s–2030s range we actually see.
    let days = days_from_epoch(year, month, day)?;
    let secs = days * 86400 + hour * 3600 + min * 60 + sec;
    Some(secs * 1000)
}

fn days_from_epoch(year: i64, month: i64, day: i64) -> Option<i64> {
    if month < 1 || month > 12 || day < 1 || day > 31 {
        return None;
    }
    // Days in each month (non-leap).
    let days_in_month = [0i64, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let is_leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let mut days: i64 = (year - 1970) * 365 + (year - 1969) / 4 - (year - 1901) / 100 + (year - 1601) / 400;
    for m in 1..month {
        days += days_in_month[m as usize];
        if m == 2 && is_leap { days += 1; }
    }
    days += day - 1;
    Some(days)
}
```

- [ ] **Step 3: Verify build**

```
cd src-tauri && cargo check
```

Expected: PASS. If `urlencoding` is not found, ensure Step 1's Cargo.toml edit was saved.

- [ ] **Step 4: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS. Address any `clippy::cast_possible_truncation` or integer-op warnings by adding explicit casts or restructuring.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/artist_info.rs src-tauri/Cargo.toml
git commit -m "$(cat <<'EOF'
artist_info: Bandsintown events fetcher + ISO8601 date parser
EOF
)"
```

---

## Task 4 — Rust: TheAudioDB photo fetcher

**Files:**
- Modify `src-tauri/src/artist_info.rs`

Mirrors `fetch_art_itunes_only` in `smtc.rs`: fetch JSON → extract image URL → download bytes → base64-encode.

- [ ] **Step 1: Add TheAudioDB constant and fetcher**

Append to `src-tauri/src/artist_info.rs` (before `#[cfg(test)]`):

```rust
// ── TheAudioDB ─────────────────────────────────────────────────────────────

/// TheAudioDB free tier uses the public test key "2".
/// Documented at https://www.theaudiodb.com/api_guide.php
const THEAUDIODB_BASE: &str = "https://www.theaudiodb.com/api/v1/json/2/search.php";

/// Fetch artist thumbnail from TheAudioDB and return as `data:image/jpeg;base64,...`.
/// Returns None on any failure.
pub(crate) async fn fetch_theaudiodb_photo(
    client: &reqwest::Client,
    artist: &str,
) -> Option<String> {
    use base64::Engine;

    let url = reqwest::Url::parse_with_params(THEAUDIODB_BASE, &[("s", artist)]).ok()?;

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[artist_info] theaudiodb search failed: {e}");
            return None;
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[artist_info] theaudiodb JSON parse failed: {e}");
            return None;
        }
    };

    let thumb_url = body
        .get("artists")?
        .as_array()?
        .first()?
        .get("strArtistThumb")?
        .as_str()?;

    if thumb_url.is_empty() {
        return None;
    }

    let bytes = match client.get(thumb_url).send().await {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[artist_info] theaudiodb image bytes failed: {e}");
                return None;
            }
        },
        Err(e) => {
            eprintln!("[artist_info] theaudiodb image fetch failed: {e}");
            return None;
        }
    };

    if bytes.is_empty() {
        return None;
    }

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:image/jpeg;base64,{b64}"))
}
```

- [ ] **Step 2: Verify build**

```
cd src-tauri && cargo check
```

Expected: PASS.

- [ ] **Step 3: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/artist_info.rs
git commit -m "$(cat <<'EOF'
artist_info: TheAudioDB photo fetcher (base64 inline)
EOF
)"
```

---

## Task 5 — Rust: MusicBrainz mbid resolver

**Files:**
- Modify `src-tauri/src/artist_info.rs`

- [ ] **Step 1: Add MusicBrainz resolver**

Append to `src-tauri/src/artist_info.rs` (before `#[cfg(test)]`):

```rust
// ── MusicBrainz ────────────────────────────────────────────────────────────

/// Resolve a MusicBrainz artist ID (mbid) by name.
/// Only invoked on the Last.fm fallback path (error 6 + Bandsintown hit).
/// MusicBrainz TOS requires a User-Agent with contact info; the HTTP client
/// built in Task 6 sets that header. Returns None on any failure.
pub(crate) async fn resolve_mbid_musicbrainz(
    client: &reqwest::Client,
    artist: &str,
) -> Option<String> {
    let query = format!("artist:{}", urlencoding::encode(artist));
    let url = reqwest::Url::parse_with_params(
        "https://musicbrainz.org/ws/2/artist",
        &[("query", query.as_str()), ("limit", "1"), ("fmt", "json")],
    )
    .ok()?;

    let resp = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[artist_info] musicbrainz fetch failed: {e}");
            return None;
        }
    };
    let body: serde_json::Value = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[artist_info] musicbrainz JSON parse failed: {e}");
            return None;
        }
    };

    body.get("artists")?
        .as_array()?
        .first()?
        .get("id")?
        .as_str()
        .map(|s| s.to_string())
}
```

- [ ] **Step 2: Verify build**

```
cd src-tauri && cargo check
```

Expected: PASS.

- [ ] **Step 3: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/artist_info.rs
git commit -m "$(cat <<'EOF'
artist_info: MusicBrainz mbid resolver
EOF
)"
```

---

## Task 6 — Rust: Orchestrator + disk cache + in-flight dedup

**Files:**
- Modify `src-tauri/src/artist_info.rs` (replace the `fetch_artist_info` stub and add `ArtistInfoCache`)
- Modify `src-tauri/Cargo.toml` (add `tokio/sync` for `Notify` if not present — it is already present)

The `tokio` dep in Cargo.toml already has `features = ["sync", ...]`, so `tokio::sync::Notify` is available.

- [ ] **Step 1: Add the HTTP client builder and managed-state cache struct**

Add the following to `src-tauri/src/artist_info.rs` after the existing fetchers and before `#[cfg(test)]`. Also add the needed imports at the top of the file (after the existing `use` statements):

At the top of the file (after `use serde::{Deserialize, Serialize};`), add:

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tokio::sync::{Mutex, Notify};
```

Then append before `#[cfg(test)]`:

```rust
// ── HTTP client ────────────────────────────────────────────────────────────

/// Build a reqwest::Client with the User-Agent required by MusicBrainz TOS
/// and a 10s timeout covering all artist-info requests.
pub(crate) fn build_artist_info_http_client() -> Result<reqwest::Client> {
    let client = reqwest::Client::builder()
        .user_agent("hum/0.11.0 (https://github.com/basezero-projects/Hum; itswesl3y@gmail.com)")
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    Ok(client)
}

// ── Disk cache ─────────────────────────────────────────────────────────────

/// On-disk structure per artist. Fields are individually timestamped so
/// tour-dates can be refreshed without blowing away the bio/photo.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct CachedArtistData {
    version: u32,
    name: Option<String>,
    slug: Option<String>,
    bio: Option<ArtistBio>,
    bio_fetched_at_unix_ms: Option<i64>,
    photo_data_url: Option<String>,
    photo_fetched_at_unix_ms: Option<i64>,
    similar_artists: Option<Vec<String>>,
    similar_fetched_at_unix_ms: Option<i64>,
    tour_dates: Option<Vec<TourDate>>,
    tour_dates_fetched_at_unix_ms: Option<i64>,
    mbid: Option<String>,
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn cache_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("cache").join("artist"))
}

fn cache_file_path(app: &AppHandle, slug: &str) -> Option<PathBuf> {
    cache_dir(app).map(|d| d.join(format!("{slug}.json")))
}

async fn read_cache_file(app: &AppHandle, slug: &str) -> Option<CachedArtistData> {
    let path = cache_file_path(app, slug)?;
    let bytes = tokio::fs::read(&path).await.ok()?;
    let data: CachedArtistData = match serde_json::from_slice(&bytes) {
        Ok(d) => d,
        Err(e) => {
            // Corrupted cache file. Per spec: delete the file, treat as a
            // miss, log so it shows up if it ever happens in real-world use.
            eprintln!(
                "[artist_info] cache file {:?} corrupted ({}); deleting",
                path, e
            );
            let _ = tokio::fs::remove_file(&path).await;
            return None;
        }
    };
    if data.version != 1 {
        // Version mismatch — delete and treat as miss so the new version's
        // shape lands on next fetch.
        let _ = tokio::fs::remove_file(&path).await;
        return None;
    }
    Some(data)
}

async fn write_cache_file(app: &AppHandle, data: &CachedArtistData) -> Result<()> {
    let slug = data.slug.as_deref().unwrap_or("unknown");
    let path = cache_file_path(app, slug)
        .ok_or_else(|| anyhow::anyhow!("could not resolve cache path"))?;
    // Ensure directory exists.
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_vec_pretty(data)?;
    tokio::fs::write(&path, &json).await?;
    Ok(())
}

// ── In-flight dedup + managed state ────────────────────────────────────────

/// Tauri managed state for the artist-info fetch chain.
/// `in_flight` maps artist slug → a `Notify` that fires when the pending
/// fetch for that slug completes. A second caller for the same slug waits
/// on the Notify instead of firing a duplicate request.
pub struct ArtistInfoCache {
    in_flight: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
    app: AppHandle,
}

impl ArtistInfoCache {
    pub fn new(app: AppHandle) -> Self {
        Self {
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            app,
        }
    }

    /// Fetch artist info. Returns cached data immediately when fresh.
    /// Re-fetches only tour dates when they are stale (≥12h).
    /// De-duplicates concurrent requests for the same slug via Notify.
    pub async fn fetch(&self, artist: &str) -> Result<ArtistInfo> {
        let slug = slug_for_artist(artist);
        if slug.is_empty() {
            return Err(anyhow::anyhow!("artist name produced empty slug"));
        }

        // Check if another task is already fetching this slug.
        let notify = {
            let mut map = self.in_flight.lock().await;
            if let Some(existing) = map.get(&slug) {
                let notify = existing.clone();
                drop(map);
                // Wait for the other fetch to complete, then read from cache.
                notify.notified().await;
                // Fall through to cache read below.
                None
            } else {
                let notify = Arc::new(Notify::new());
                map.insert(slug.clone(), notify.clone());
                Some(notify)
            }
        };

        // Always read cache after acquiring or waiting.
        let now = now_unix_ms();
        if let Some(cached) = read_cache_file(&self.app, &slug).await {
            let tour_stale = cached
                .tour_dates_fetched_at_unix_ms
                .map(|t| tour_dates_stale(t, now))
                .unwrap_or(true);

            if notify.is_none() {
                // We waited on another fetch; return the cache result.
                return build_artist_info_from_cache(&cached, artist, &slug);
            }

            if !tour_stale {
                // Fully fresh — release the in-flight slot and return.
                let notify = notify.unwrap();
                {
                    let mut map = self.in_flight.lock().await;
                    map.remove(&slug);
                }
                notify.notify_waiters();
                return build_artist_info_from_cache(&cached, artist, &slug);
            }

            // Tour dates stale — refetch only events; keep everything else.
            let client = build_artist_info_http_client()?;
            let new_events = fetch_bandsintown_events(&client, artist).await;
            let mut updated = cached.clone();
            updated.tour_dates = Some(new_events);
            updated.tour_dates_fetched_at_unix_ms = Some(now);
            let _ = write_cache_file(&self.app, &updated).await;
            let notify = notify.unwrap();
            {
                let mut map = self.in_flight.lock().await;
                map.remove(&slug);
            }
            notify.notify_waiters();
            return build_artist_info_from_cache(&updated, artist, &slug);
        }

        // Cache miss — full fetch.
        let notify = notify.unwrap(); // We definitely hold the notify here.
        let client = build_artist_info_http_client()?;

        // Parallel fetch: bio, similar, events, photo.
        let (bio_result, similar_result, events_result, photo_result) = tokio::join!(
            fetch_lastfm_bio(&client, artist),
            fetch_lastfm_similar(&client, artist),
            fetch_bandsintown_events(&client, artist),
            fetch_theaudiodb_photo(&client, artist),
        );

        // MusicBrainz fallback: only if Last.fm bio failed AND Bandsintown
        // returned something (so we know the artist actually exists).
        let (final_bio, mbid) = if bio_result.is_none() && !events_result.is_empty() {
            let mbid_opt = resolve_mbid_musicbrainz(&client, artist).await;
            if let Some(ref mbid_str) = mbid_opt {
                // Retry Last.fm with the mbid.
                let bio_retry = fetch_lastfm_bio_by_mbid(&client, mbid_str).await;
                (bio_retry, mbid_opt)
            } else {
                (None, None)
            }
        } else {
            (bio_result, None)
        };

        let data = CachedArtistData {
            version: 1,
            name: Some(artist.to_string()),
            slug: Some(slug.clone()),
            bio: final_bio,
            bio_fetched_at_unix_ms: Some(now),
            photo_data_url: photo_result,
            photo_fetched_at_unix_ms: Some(now),
            similar_artists: Some(similar_result),
            similar_fetched_at_unix_ms: Some(now),
            tour_dates: Some(events_result),
            tour_dates_fetched_at_unix_ms: Some(now),
            mbid,
        };

        let _ = write_cache_file(&self.app, &data).await;

        {
            let mut map = self.in_flight.lock().await;
            map.remove(&slug);
        }
        notify.notify_waiters();

        build_artist_info_from_cache(&data, artist, &slug)
    }

    /// Wipe the entire artist cache directory.
    pub async fn clear(&self) -> Result<()> {
        if let Some(dir) = cache_dir(&self.app) {
            if dir.exists() {
                tokio::fs::remove_dir_all(&dir).await?;
            }
        }
        Ok(())
    }
}

/// Retry Last.fm artist.getInfo using an mbid instead of artist name.
async fn fetch_lastfm_bio_by_mbid(
    client: &reqwest::Client,
    mbid: &str,
) -> Option<ArtistBio> {
    let url = reqwest::Url::parse_with_params(
        LASTFM_BASE,
        &[
            ("method", "artist.getInfo"),
            ("mbid", mbid),
            ("api_key", LASTFM_API_KEY),
            ("format", "json"),
        ],
    )
    .ok()?;
    let resp = client.get(url).send().await.ok()?;
    let body: serde_json::Value = resp.json().await.ok()?;
    if body.get("error").is_some() {
        return None;
    }
    let artist_obj = body.get("artist")?;
    let raw_bio = artist_obj
        .get("bio")?
        .get("summary")?
        .as_str()
        .unwrap_or("")
        .to_string();
    let lastfm_url = artist_obj
        .get("url")?
        .as_str()
        .unwrap_or("")
        .to_string();
    if lastfm_url.is_empty() { return None; }
    let mut bio_text = strip_html(&raw_bio);
    if bio_text.len() > 1500 {
        let cutoff = bio_text[..1500].rfind(|c| matches!(c, '.' | '?' | '!')).map(|i| i + 1).unwrap_or(1500);
        bio_text.truncate(cutoff);
        bio_text = bio_text.trim_end().to_string();
    }
    if bio_text.is_empty() { return None; }
    Some(ArtistBio { text: bio_text, lastfm_url })
}

fn build_artist_info_from_cache(
    data: &CachedArtistData,
    artist: &str,
    slug: &str,
) -> Result<ArtistInfo> {
    Ok(ArtistInfo {
        name: data.name.clone().unwrap_or_else(|| artist.to_string()),
        slug: data.slug.clone().unwrap_or_else(|| slug.to_string()),
        bio: data.bio.clone(),
        photo_data_url: data.photo_data_url.clone(),
        similar_artists: data.similar_artists.clone().unwrap_or_default(),
        tour_dates: data.tour_dates.clone().unwrap_or_default(),
        mbid: data.mbid.clone(),
        fetched_at_unix_ms: data
            .bio_fetched_at_unix_ms
            .or(data.tour_dates_fetched_at_unix_ms)
            .unwrap_or(0),
    })
}
```

- [ ] **Step 2: Replace the `fetch_artist_info` stub**

Find and remove the old stub:

```rust
/// Stub — filled in by Task 6.
pub async fn fetch_artist_info(_artist: &str) -> Result<ArtistInfo> {
    todo!("implemented in Task 6")
}
```

Replace with:

```rust
/// Top-level entry point for callers that don't hold an ArtistInfoCache.
/// Prefer ArtistInfoCache::fetch which adds caching + dedup.
pub async fn fetch_artist_info(artist: &str) -> Result<ArtistInfo> {
    let client = build_artist_info_http_client()?;
    let now = now_unix_ms();
    let (bio, similar, events, photo) = tokio::join!(
        fetch_lastfm_bio(&client, artist),
        fetch_lastfm_similar(&client, artist),
        fetch_bandsintown_events(&client, artist),
        fetch_theaudiodb_photo(&client, artist),
    );
    Ok(ArtistInfo {
        name: artist.to_string(),
        slug: slug_for_artist(artist),
        bio,
        photo_data_url: photo,
        similar_artists: similar,
        tour_dates: events,
        mbid: None,
        fetched_at_unix_ms: now,
    })
}
```

- [ ] **Step 3: Verify build**

```
cd src-tauri && cargo check
```

Expected: PASS. Address any `unused import` or lifetime errors reported by the compiler.

- [ ] **Step 4: Run all existing tests**

```
cd src-tauri && cargo test --lib
```

Expected: PASS — all Task 1 unit tests still pass.

- [ ] **Step 5: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/artist_info.rs
git commit -m "$(cat <<'EOF'
artist_info: orchestrator, disk cache, in-flight dedup (ArtistInfoCache)
EOF
)"
```

---

## Task 7 — Rust: Tauri commands + lib.rs wiring

**Files:**
- Modify `src-tauri/src/artist_info.rs` (add Tauri commands)
- Modify `src-tauri/src/lib.rs` (manage state, register commands)

- [ ] **Step 1: Add Tauri commands to artist_info.rs**

Append to `src-tauri/src/artist_info.rs` (before `#[cfg(test)]`):

```rust
// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_artist_info(
    artist: String,
    cache: tauri::State<'_, ArtistInfoCache>,
) -> Result<ArtistInfo, String> {
    cache.fetch(&artist).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_artist_info_cache(
    cache: tauri::State<'_, ArtistInfoCache>,
) -> Result<(), String> {
    cache.clear().await.map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Wire into lib.rs**

Open `src-tauri/src/lib.rs`. After the existing `mod artist_info;` line added in Task 1, no additional `mod` declaration is needed.

Find the line:

```rust
use settings::{
    get_settings, open_settings_window, reset_settings, update_settings, SharedSettings,
};
```

After that `use` block, add:

```rust
use artist_info::{ArtistInfoCache, clear_artist_info_cache, get_artist_info};
```

Find the `.manage(lyrics_state)` line inside `tauri::Builder::default()` (around line 181 in the current file). After `.manage(mode_state)`, add:

```rust
        .manage(ArtistInfoCache::new_placeholder())
```

But `ArtistInfoCache` needs `AppHandle` which is only available inside `.setup()`. Use a different pattern — initialize inside `.setup()`:

In the `.setup(move |app| {` block, after `app.manage::<SharedSettings>(...)`:

```rust
            app.manage(ArtistInfoCache::new(app.handle().clone()));
```

Because `.manage()` takes ownership and `ArtistInfoCache` is not `Clone`, this requires moving it — which requires `app.handle().clone()` first. Correct pattern:

```rust
            let artist_cache = ArtistInfoCache::new(app.handle().clone());
            app.manage(artist_cache);
```

Place these two lines immediately after:

```rust
            app.manage::<SharedSettings>(Arc::new(RwLock::new(loaded_settings)));
```

Find the `invoke_handler` block:

```rust
        .invoke_handler(tauri::generate_handler![
            get_current_track,
            get_current_lyrics,
            get_current_album_art,
            get_overlay_mode,
            set_overlay_mode,
            cycle_overlay_mode,
            toggle_overlay_visibility,
            get_settings,
            update_settings,
            reset_settings,
            open_settings_window,
            set_update_indicator,
            set_update_banner_visible,
        ])
```

Replace with:

```rust
        .invoke_handler(tauri::generate_handler![
            get_current_track,
            get_current_lyrics,
            get_current_album_art,
            get_overlay_mode,
            set_overlay_mode,
            cycle_overlay_mode,
            toggle_overlay_visibility,
            get_settings,
            update_settings,
            reset_settings,
            open_settings_window,
            set_update_indicator,
            set_update_banner_visible,
            get_artist_info,
            clear_artist_info_cache,
        ])
```

- [ ] **Step 3: Verify build**

```
cd src-tauri && cargo check
```

Expected: PASS.

- [ ] **Step 4: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/artist_info.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
artist_info: Tauri commands + lib.rs wiring (get_artist_info, clear_artist_info_cache)
EOF
)"
```

---

## Task 8 — Rust: `artist_window` module + open/close commands + auto-close listener

**Files:**
- Create `src-tauri/src/artist_window.rs`
- Modify `src-tauri/src/lib.rs`

- [ ] **Step 1: Create artist_window.rs**

Create `src-tauri/src/artist_window.rs` with full content:

```rust
//! Creates / closes the artist-info peer window on demand.
//! Window label: "artist-info"
//! URL: "artist-panel/index.html" (Vite multi-page entry added in Task 11)

use anyhow::{anyhow, Result};
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

#[cfg(windows)]
use windows::Win32::Foundation::HWND;

/// Open the artist-info panel window.
/// If a window with label "artist-info" already exists, focus it instead.
/// Position: anchored below the "overlay" window (center-aligned horizontally).
/// If less than 500px of screen below the overlay, anchors above instead.
pub async fn open_artist_panel(app: AppHandle) -> Result<()> {
    // If already open, just focus it.
    if let Some(existing) = app.get_webview_window("artist-info") {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(());
    }

    // Compute position relative to the overlay window.
    let (x, y) = compute_panel_position(&app)?;

    let window = WebviewWindowBuilder::new(
        &app,
        "artist-info",
        WebviewUrl::App("artist-panel/index.html".into()),
    )
    .title("Artist Info")
    .inner_size(360.0, 480.0)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .resizable(false)
    .skip_taskbar(true)
    .position(x as f64, y as f64)
    .build()?;

    // Mirror the overlay's window_backdrop setting onto this peer window so
    // the visual matches (Mica / Acrylic / Tabbed Mica / None). v0.10.23
    // backdrop machinery in `crate::backdrop` does the DWM call; we just
    // read the current kind from the persisted Settings state.
    #[cfg(windows)]
    {
        let settings_state = app.state::<crate::settings::SharedSettings>();
        let kind = settings_state.inner().blocking_read().window_backdrop;
        if let Ok(raw_hwnd) = window.hwnd() {
            let hwnd = HWND(raw_hwnd.0);
            if let Err(e) = crate::backdrop::apply_backdrop(hwnd, kind) {
                eprintln!("[artist_window] apply_backdrop failed: {e:#}");
            }
        }
    }

    let _ = window.show();

    // Listen for track-changed: auto-close when artist changes.
    // We capture the artist name at open time from get_current_track.
    let app_for_listener = app.clone();
    let open_artist = {
        let snap = app.state::<crate::smtc::SharedSnapshot>();
        let track = snap.read().await;
        crate::lyrics::clean_artist(&track.artist)
    };

    let _unlisten = app.listen("track-changed", move |event| {
        // Parse the new track's artist from the event payload.
        if let Ok(track) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            let new_artist = track
                .get("artist")
                .and_then(|a| a.as_str())
                .map(crate::lyrics::clean_artist)
                .unwrap_or_default();
            if !new_artist.eq_ignore_ascii_case(&open_artist) {
                if let Some(w) = app_for_listener.get_webview_window("artist-info") {
                    let _ = w.close();
                }
            }
        }
    });

    Ok(())
}

/// Close the artist-info panel window if it is open.
pub fn close_artist_panel(app: &AppHandle) -> Result<()> {
    if let Some(w) = app.get_webview_window("artist-info") {
        w.close()?;
    }
    Ok(())
}

/// Compute the (x, y) screen position for the artist-info window.
/// Centers horizontally on the overlay; anchors 8px below (or above if near screen bottom).
fn compute_panel_position(app: &AppHandle) -> Result<(i32, i32)> {
    let overlay = app
        .get_webview_window("overlay")
        .ok_or_else(|| anyhow!("overlay window not found"))?;

    let pos = overlay.outer_position()?;
    let size = overlay.outer_size()?;

    // Panel is 360px wide. Center it on the overlay.
    let center_x = pos.x + (size.width as i32) / 2 - 180;

    // 480px tall panel. Check if it fits below.
    let below_y = pos.y + size.height as i32 + 8;

    // Get monitor height to decide above/below.
    let monitor_height = overlay
        .current_monitor()?
        .map(|m| m.size().height as i32)
        .unwrap_or(1080);

    let y = if below_y + 480 <= monitor_height {
        below_y
    } else {
        pos.y - 488
    };

    // Clamp to screen top.
    let y = y.max(0);
    // Clamp x to screen left (rough guard — no right-side clamp needed for most setups).
    let x = center_x.max(0);

    Ok((x, y))
}

// ── Tauri commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn open_artist_panel_cmd(app: AppHandle) -> Result<(), String> {
    open_artist_panel(app).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn close_artist_panel_cmd(app: AppHandle) -> Result<(), String> {
    close_artist_panel(&app).map_err(|e| e.to_string())
}
```

**Note on `clean_artist`:** `lyrics::clean_artist` must be `pub` for the call above. Check whether it is already public in `src-tauri/src/lyrics.rs`. If it is not, the build step will show the error; in that case, open `src-tauri/src/lyrics.rs`, find the `fn clean_artist(` declaration, and change it to `pub fn clean_artist(`.

- [ ] **Step 2: Register module and commands in lib.rs**

After `mod artist_info;`, add:

```rust
mod artist_window;
```

After `use artist_info::{ArtistInfoCache, clear_artist_info_cache, get_artist_info};`, add:

```rust
use artist_window::{close_artist_panel_cmd, open_artist_panel_cmd};
```

Add both commands to the `invoke_handler!` macro:

```rust
            open_artist_panel_cmd,
            close_artist_panel_cmd,
```

- [ ] **Step 3: Verify build**

```
cd src-tauri && cargo check
```

Expected: PASS. If `clean_artist` is not pub, fix it in `lyrics.rs` and re-run.

- [ ] **Step 4: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/artist_window.rs src-tauri/src/lib.rs
git commit -m "$(cat <<'EOF'
artist_window: open/close panel commands, position logic, auto-close on artist change
EOF
)"
```

---

## Task 9 — Rust: `open_ticket_url` command + capability permissions

**Files:**
- Modify `src-tauri/src/artist_window.rs` (add command)
- Modify `src-tauri/src/lib.rs` (register command)
- Modify `src-tauri/capabilities/default.json` (permissions)
- Modify `src-tauri/tauri.conf.json` (add artist-info to windows list for capabilities scope)

**Which opener plugin:** `tauri-plugin-opener` is NOT in `Cargo.toml`. The existing Tauri 2 setup uses no explicit shell plugin either. Use `tauri::Emitter`-based approach — specifically the Tauri 2 API `tauri_plugin_shell` is also not present. The cleanest zero-new-dep approach for Tauri 2: use `opener::open(url)` from `opener = "2"` crate, OR use `std::process::Command::new("cmd").args(["/c", "start", url])` on Windows. Since we're Windows-only for this feature in practice, use the `opener` crate.

Check Cargo.toml: `opener` is not present. Add it.

- [ ] **Step 1: Add `opener` crate**

Edit `src-tauri/Cargo.toml`, after `urlencoding = "2"`, add:

```toml
opener = "0.7"
```

- [ ] **Step 2: Add `open_ticket_url` to artist_window.rs**

Append before the existing `#[tauri::command]` blocks in `artist_window.rs`:

```rust
/// Allowed URL hosts for ticket / artist links. Defends against cache-poisoning
/// with malformed or malicious URLs.
const TICKET_URL_WHITELIST: &[&str] = &[
    "bandsintown.com",
    "www.bandsintown.com",
    "ticketmaster.com",
    "www.ticketmaster.com",
    "seatgeek.com",
    "www.seatgeek.com",
    "axs.com",
    "www.axs.com",
    "livenation.com",
    "www.livenation.com",
    "last.fm",
    "www.last.fm",
    "theaudiodb.com",
    "www.theaudiodb.com",
    "musicbrainz.org",
    "www.musicbrainz.org",
];

#[tauri::command]
pub fn open_ticket_url(url: String) -> Result<(), String> {
    // Parse and whitelist-check the host.
    let parsed = reqwest::Url::parse(&url).map_err(|e| format!("invalid URL: {e}"))?;
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    if !TICKET_URL_WHITELIST.iter().any(|allowed| host == *allowed) {
        return Err(format!("URL host '{host}' is not on the ticket link whitelist"));
    }
    opener::open(&url).map_err(|e| format!("open_ticket_url failed: {e}"))
}
```

Also add `use reqwest;` at the top of `artist_window.rs` if not already present.

- [ ] **Step 3: Register command in lib.rs**

Add import:

```rust
use artist_window::{close_artist_panel_cmd, open_artist_panel_cmd, open_ticket_url};
```

Add to `invoke_handler!`:

```rust
            open_ticket_url,
```

- [ ] **Step 4: Update capabilities/default.json**

The `artist-info` window needs to be in scope for capabilities, and new permissions are required. Edit `src-tauri/capabilities/default.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default permissions for main + overlay + settings + artist-info windows.",
  "windows": ["main", "overlay", "settings", "artist-info"],
  "permissions": [
    "core:default",
    "core:event:default",
    "core:window:allow-start-dragging",
    "core:window:allow-show",
    "core:window:allow-hide",
    "core:window:allow-set-ignore-cursor-events",
    "core:window:allow-set-focus",
    "core:window:allow-unminimize",
    "core:window:allow-close",
    "core:window:allow-set-position",
    "core:window:allow-set-size",
    "core:webview:allow-create-webview-window",
    "core:tray:default",
    "store:default",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister",
    "global-shortcut:allow-is-registered",
    "window-state:default",
    "updater:default",
    "process:allow-restart"
  ]
}
```

- [ ] **Step 5: Verify build**

```
cd src-tauri && cargo check
```

Expected: PASS.

- [ ] **Step 6: Run clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/artist_window.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/capabilities/default.json
git commit -m "$(cat <<'EOF'
artist_window: open_ticket_url with whitelist; capability permissions for artist-info window
EOF
)"
```

---

## Task 10 — Frontend: types.ts updates + Overlay.tsx DEFAULT_SETTINGS

**Files:**
- Modify `src/types.ts`
- Modify `src/Overlay.tsx`

- [ ] **Step 1: Add new types to types.ts**

Open `src/types.ts`. After the `Settings` type definition (ending with `streamer_port: number;`), add `show_artist_info_panel: boolean;` to the Settings type. The full Settings type becomes:

```ts
export type Settings = {
  last_mode: OverlayMode;
  anticipate_ms: number;
  jitter_tolerance_ms: number;
  font_family: string;
  font_size_px: number;
  font_weight: number;
  text_color: string;
  text_color_dim: string;
  bg_color: string;
  bg_opacity: number;
  text_align: TextAlign;
  line_padding_px: number;
  layout_mode: LayoutMode;
  show_album_art: boolean;
  show_translation: boolean;
  tint_bg_from_album_art: boolean;
  blur_album_art_background: boolean;
  window_backdrop: "acrylic" | "mica" | "tabbed_mica" | "none";
  auto_contrast: boolean;
  streamer_enabled: boolean;
  streamer_port: number;
  show_artist_info_panel: boolean;
};
```

After the `Settings` type (before `WordSpan`), add:

```ts
export type TicketStatus = "available" | "sold_out";

export type TourDate = {
  date_unix_ms: number;
  city: string;
  region: string;
  country: string;
  venue: string;
  ticket_url: string | null;
  status: TicketStatus;
};

export type ArtistBio = {
  text: string;
  lastfm_url: string;
};

export type ArtistInfo = {
  name: string;
  slug: string;
  bio: ArtistBio | null;
  photo_data_url: string | null;
  similar_artists: string[];
  tour_dates: TourDate[];
  mbid: string | null;
  fetched_at_unix_ms: number;
};
```

- [ ] **Step 2: Update DEFAULT_SETTINGS in Overlay.tsx**

Find the `DEFAULT_SETTINGS` literal in `src/Overlay.tsx` (around line 18). After `streamer_port: 38247,`, add:

```ts
  show_artist_info_panel: true,
```

- [ ] **Step 3: Also update settings.rs**

Open `src-tauri/src/settings.rs`. In the `Settings` struct, after `pub streamer_port: u16,`, add:

```rust
    /// When true, clicking album art (or the "•••" fallback dot) opens the
    /// artist-info panel window.
    pub show_artist_info_panel: bool,
```

In `impl Default for Settings`, after `streamer_port: 38247,`, add:

```rust
            show_artist_info_panel: true,
```

- [ ] **Step 4: Typecheck**

```
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 5: Rust check**

```
cd src-tauri && cargo check
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/types.ts src/Overlay.tsx src-tauri/src/settings.rs
git commit -m "$(cat <<'EOF'
types: ArtistInfo/TourDate/ArtistBio/TicketStatus; Settings.show_artist_info_panel
EOF
)"
```

---

## Task 11 — Frontend: Vite multi-page entry + panel HTML/main.tsx

**Files:**
- Create `src/artist-panel/index.html`
- Create `src/artist-panel/main.tsx`
- Modify `vite.config.ts`

- [ ] **Step 1: Create src/artist-panel/index.html**

Read the root `index.html` first for the exact structure. Create `src/artist-panel/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Artist Info</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="./main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 2: Create src/artist-panel/main.tsx**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import ArtistPanel from "./ArtistPanel";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ArtistPanel />
  </React.StrictMode>
);
```

Create a stub `ArtistPanel.tsx` at this step (full implementation is Task 12):

```tsx
export default function ArtistPanel() {
  return <div style={{ color: "#fff", padding: 16 }}>Artist Panel — loading…</div>;
}
```

- [ ] **Step 3: Modify vite.config.ts**

The current `vite.config.ts` has no `build.rollupOptions.input`. Add it:

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "./src") },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        artistPanel: path.resolve(__dirname, "src/artist-panel/index.html"),
      },
    },
  },
});
```

- [ ] **Step 4: Typecheck**

```
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 5: Build (multi-page must succeed)**

```
pnpm build
```

Expected: PASS — two HTML entry points compiled into `dist/`. Output includes `dist/index.html` and `dist/src/artist-panel/index.html` (or similar under the Vite output dir).

- [ ] **Step 6: Commit**

```bash
git add src/artist-panel/index.html src/artist-panel/main.tsx src/artist-panel/ArtistPanel.tsx vite.config.ts
git commit -m "$(cat <<'EOF'
vite: multi-page build; artist-panel entry point scaffold
EOF
)"
```

---

## Task 12 — Frontend: ArtistPanel.tsx React component

**Files:**
- Modify `src/artist-panel/ArtistPanel.tsx` (replace stub with full implementation)

- [ ] **Step 1: Write the full ArtistPanel component**

Replace `src/artist-panel/ArtistPanel.tsx` entirely:

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ArtistInfo, CurrentTrack, TourDate } from "../types";

const GOLD = "#d4af37";
const DIM = "rgba(234,234,234,0.55)";
const BG = "rgba(18, 18, 18, 0.97)";
const BORDER = "rgba(255,255,255,0.07)";

export default function ArtistPanel() {
  const [artistName, setArtistName] = useState<string>("");
  const [info, setInfo] = useState<ArtistInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Brief 2s toast surfaced when `open_ticket_url` rejects a URL (host
  // not on the whitelist, or `opener::open` fails). The spec requires a
  // user-visible signal rather than silently dropping the click.
  const [toast, setToast] = useState<string | null>(null);

  // Auto-clear toast after 2s.
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 2000);
    return () => clearTimeout(t);
  }, [toast]);

  // On mount: get current track, then fetch artist info.
  useEffect(() => {
    async function init() {
      try {
        const track = await invoke<CurrentTrack>("get_current_track");
        const name = track.artist?.trim() ?? "";
        setArtistName(name);
        if (!name) {
          setLoading(false);
          return;
        }
        setLoading(true);
        const result = await invoke<ArtistInfo>("get_artist_info", { artist: name });
        setInfo(result);
        setLoading(false);
      } catch (e) {
        setError(String(e));
        setLoading(false);
      }
    }
    init();
  }, []);

  // Listen for track-changed: if the artist changes, close this window.
  useEffect(() => {
    const unlisten = listen<CurrentTrack>("track-changed", (event) => {
      const newArtist = event.payload.artist?.trim() ?? "";
      if (artistName && newArtist.toLowerCase() !== artistName.toLowerCase()) {
        invoke("close_artist_panel_cmd").catch(() => {});
      }
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, [artistName]);

  // ESC key closes the panel.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        invoke("close_artist_panel_cmd").catch(() => {});
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  function retry() {
    setError(null);
    setLoading(true);
    invoke<ArtistInfo>("get_artist_info", { artist: artistName })
      .then((result) => { setInfo(result); setLoading(false); })
      .catch((e) => { setError(String(e)); setLoading(false); });
  }

  function openUrl(url: string) {
    invoke("open_ticket_url", { url }).catch(() => {
      setToast("Couldn't open browser");
    });
  }

  function close() {
    invoke("close_artist_panel_cmd").catch(() => {});
  }

  const photo = info?.photo_data_url ?? null;
  const displayName = info?.name ?? artistName;

  return (
    <div
      style={{
        position: "relative",
        display: "flex",
        flexDirection: "column",
        minHeight: "100vh",
        background: BG,
        color: "rgba(234,234,234,0.9)",
        fontFamily: "'Inter', system-ui, sans-serif",
        fontSize: 13,
        overflow: "hidden",
      }}
    >
      {/* Header — drag region */}
      <div
        data-tauri-drag-region
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          padding: "10px 12px 10px 12px",
          borderBottom: `1px solid ${BORDER}`,
          flexShrink: 0,
          userSelect: "none",
        }}
      >
        {/* Artist photo */}
        {photo ? (
          <img
            src={photo}
            alt=""
            draggable={false}
            style={{
              width: 60,
              height: 60,
              borderRadius: "50%",
              objectFit: "cover",
              flexShrink: 0,
              pointerEvents: "none",
            }}
          />
        ) : (
          <div
            style={{
              width: 60,
              height: 60,
              borderRadius: "50%",
              background: "rgba(255,255,255,0.08)",
              flexShrink: 0,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: 24,
              color: DIM,
            }}
          >
            ♪
          </div>
        )}

        {/* Artist name */}
        <div
          style={{
            flex: 1,
            fontSize: 18,
            fontWeight: 600,
            letterSpacing: 0.1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
            pointerEvents: "none",
          }}
        >
          {displayName || "Unknown Artist"}
        </div>

        {/* Close button */}
        <button
          onClick={close}
          style={{
            background: "transparent",
            border: "none",
            color: DIM,
            cursor: "pointer",
            fontSize: 16,
            lineHeight: 1,
            padding: "2px 4px",
            borderRadius: 4,
            flexShrink: 0,
          }}
          onMouseEnter={(e) => (e.currentTarget.style.color = GOLD)}
          onMouseLeave={(e) => (e.currentTarget.style.color = DIM)}
          aria-label="Close"
        >
          ✕
        </button>
      </div>

      {/* Scrollable body */}
      <div style={{ flex: 1, overflowY: "auto", padding: "12px 14px" }}>
        {loading && !error && (
          <div>
            <LoadingDots />
            <div style={{ color: DIM, fontSize: 12, marginTop: 8 }}>Loading…</div>
          </div>
        )}

        {error && (
          <div style={{ textAlign: "center", padding: "24px 0" }}>
            <div style={{ color: "rgba(229,115,115,0.9)", marginBottom: 12 }}>
              Couldn't load artist info
            </div>
            <button onClick={retry} style={retryButtonStyle}>
              Retry
            </button>
          </div>
        )}

        {!loading && !error && info && (
          <>
            {/* Bio section */}
            {info.bio && (
              <section style={{ marginBottom: 16 }}>
                <SectionLabel>Bio</SectionLabel>
                <p
                  style={{
                    margin: 0,
                    lineHeight: 1.6,
                    color: "rgba(234,234,234,0.82)",
                    fontSize: 12.5,
                  }}
                >
                  {info.bio.text}
                </p>
                <div style={{ marginTop: 6 }}>
                  <ExternalLink url={info.bio.lastfm_url} onOpen={openUrl}>
                    Read more on Last.fm →
                  </ExternalLink>
                </div>
              </section>
            )}

            {/* Similar artists */}
            {info.similar_artists.length > 0 && (
              <section style={{ marginBottom: 16 }}>
                <SectionLabel>Similar to</SectionLabel>
                <p style={{ margin: 0, color: DIM, fontSize: 12.5, lineHeight: 1.6 }}>
                  {info.similar_artists.join(", ")}
                </p>
              </section>
            )}

            {/* Tour dates */}
            <section style={{ marginBottom: 16 }}>
              <SectionLabel>Upcoming shows</SectionLabel>
              <TourDatesList dates={info.tour_dates} onOpenUrl={openUrl} />
            </section>
          </>
        )}
      </div>

      {/* Footer attribution */}
      <div
        style={{
          flexShrink: 0,
          borderTop: `1px solid ${BORDER}`,
          padding: "6px 14px",
          fontSize: 10,
          color: "rgba(234,234,234,0.3)",
          textAlign: "center",
          display: "flex",
          gap: 6,
          justifyContent: "center",
          flexWrap: "wrap",
        }}
      >
        <span>Powered by</span>
        <FooterLink url="https://bandsintown.com" onOpen={openUrl}>Bandsintown</FooterLink>
        <span>·</span>
        <FooterLink url="https://last.fm" onOpen={openUrl}>Last.fm</FooterLink>
        <span>·</span>
        <FooterLink url="https://www.theaudiodb.com" onOpen={openUrl}>TheAudioDB</FooterLink>
      </div>

      {/* Toast overlay — shown briefly when open_ticket_url fails. */}
      {toast ? (
        <div
          style={{
            position: "absolute",
            bottom: 12,
            left: "50%",
            transform: "translateX(-50%)",
            background: "rgba(0,0,0,0.85)",
            color: "rgba(234,234,234,0.95)",
            fontSize: 11,
            padding: "6px 12px",
            borderRadius: 6,
            boxShadow: "0 4px 12px rgba(0,0,0,0.5)",
            pointerEvents: "none",
            zIndex: 100,
          }}
        >
          {toast}
        </div>
      ) : null}
    </div>
  );
}

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: 0.8,
        textTransform: "uppercase",
        color: GOLD,
        marginBottom: 6,
      }}
    >
      {children}
    </div>
  );
}

function TourDatesList({
  dates,
  onOpenUrl,
}: {
  dates: TourDate[];
  onOpenUrl: (url: string) => void;
}) {
  if (dates.length === 0) {
    return (
      <p style={{ margin: 0, color: DIM, fontStyle: "italic", fontSize: 12 }}>
        No upcoming tour dates.
      </p>
    );
  }

  const visible = dates.slice(0, 10);
  const hasMore = dates.length > 10;

  return (
    <div>
      {visible.map((event, i) => (
        <TourDateRow key={i} event={event} onOpenUrl={onOpenUrl} />
      ))}
      {hasMore && (
        <div style={{ marginTop: 8 }}>
          <ExternalLink
            url={`https://bandsintown.com`}
            onOpen={onOpenUrl}
          >
            View all on Bandsintown →
          </ExternalLink>
        </div>
      )}
    </div>
  );
}

function TourDateRow({
  event,
  onOpenUrl,
}: {
  event: TourDate;
  onOpenUrl: (url: string) => void;
}) {
  const dateStr = formatTourDate(event.date_unix_ms);
  const location =
    event.region
      ? `${event.city}, ${event.region}`
      : `${event.city}${event.country ? `, ${event.country}` : ""}`;

  const isSoldOut = event.status === "sold_out";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 10,
        padding: "6px 0",
        borderBottom: `1px solid ${BORDER}`,
      }}
    >
      {/* Date */}
      <div
        style={{
          width: 44,
          flexShrink: 0,
          fontSize: 11,
          fontVariantNumeric: "tabular-nums",
          fontWeight: 600,
          color: GOLD,
        }}
      >
        {dateStr}
      </div>

      {/* Location + venue */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        <div
          style={{
            fontSize: 12.5,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {location}
        </div>
        {event.venue && (
          <div
            style={{
              fontSize: 11,
              color: DIM,
              fontStyle: "italic",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {event.venue}
          </div>
        )}
      </div>

      {/* Ticket button */}
      {event.ticket_url && (
        <button
          disabled={isSoldOut}
          onClick={() => !isSoldOut && event.ticket_url && onOpenUrl(event.ticket_url)}
          style={{
            flexShrink: 0,
            fontSize: 11,
            fontWeight: 600,
            padding: "3px 8px",
            borderRadius: 4,
            border: "none",
            cursor: isSoldOut ? "not-allowed" : "pointer",
            background: isSoldOut ? "rgba(255,255,255,0.1)" : GOLD,
            color: isSoldOut ? DIM : "#111",
            opacity: isSoldOut ? 0.6 : 1,
            transition: "background 120ms ease",
          }}
          onMouseEnter={(e) => {
            if (!isSoldOut) (e.currentTarget.style.background = "#b8962d");
          }}
          onMouseLeave={(e) => {
            if (!isSoldOut) (e.currentTarget.style.background = GOLD);
          }}
        >
          {isSoldOut ? "Sold Out" : "Tickets"}
        </button>
      )}
    </div>
  );
}

function formatTourDate(unix_ms: number): string {
  const d = new Date(unix_ms);
  const now = new Date();
  const months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
  const month = months[d.getUTCMonth()];
  const day = d.getUTCDate();
  const year = d.getUTCFullYear();
  if (year === now.getFullYear()) {
    return `${month} ${day}`;
  }
  return `${month} ${day}, ${year}`;
}

function ExternalLink({
  url,
  onOpen,
  children,
}: {
  url: string;
  onOpen: (url: string) => void;
  children: React.ReactNode;
}) {
  return (
    <span
      role="link"
      tabIndex={0}
      onClick={() => onOpen(url)}
      onKeyDown={(e) => e.key === "Enter" && onOpen(url)}
      style={{
        color: GOLD,
        cursor: "pointer",
        fontSize: 12,
        textDecoration: "underline",
        textDecorationColor: "rgba(212,175,55,0.4)",
      }}
    >
      {children}
    </span>
  );
}

function FooterLink({
  url,
  onOpen,
  children,
}: {
  url: string;
  onOpen: (url: string) => void;
  children: React.ReactNode;
}) {
  return (
    <span
      role="link"
      tabIndex={0}
      onClick={() => onOpen(url)}
      onKeyDown={(e) => e.key === "Enter" && onOpen(url)}
      style={{ cursor: "pointer", color: "rgba(234,234,234,0.35)", fontSize: 10 }}
      onMouseEnter={(e) => (e.currentTarget.style.color = "rgba(234,234,234,0.65)")}
      onMouseLeave={(e) => (e.currentTarget.style.color = "rgba(234,234,234,0.35)")}
    >
      {children}
    </span>
  );
}

function LoadingDots() {
  return (
    <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
      {[0, 1, 2].map((i) => (
        <div
          key={i}
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: GOLD,
            opacity: 0.6,
            animation: `pulse 1s ease-in-out ${i * 0.2}s infinite`,
          }}
        />
      ))}
      <style>{`@keyframes pulse { 0%,100%{opacity:0.3;transform:scale(0.85)} 50%{opacity:1;transform:scale(1.1)} }`}</style>
    </div>
  );
}

const retryButtonStyle: React.CSSProperties = {
  background: "transparent",
  border: `1px solid ${GOLD}`,
  color: GOLD,
  cursor: "pointer",
  fontSize: 12,
  padding: "4px 14px",
  borderRadius: 4,
};
```

- [ ] **Step 2: Typecheck**

```
pnpm typecheck
```

Expected: PASS. Fix any import path issues (e.g. if `../types` path doesn't resolve, adjust to the correct relative path from `src/artist-panel/`).

- [ ] **Step 3: Commit**

```bash
git add src/artist-panel/ArtistPanel.tsx
git commit -m "$(cat <<'EOF'
artist-panel: full React panel UI (bio, similar, tour dates, tickets, footer)
EOF
)"
```

---

## Task 13 — Frontend: Overlay.tsx click handler + "•••" fallback dot

**Files:**
- Modify `src/Overlay.tsx`

- [ ] **Step 1: Update AlbumArtSide to accept an onClick prop**

Find the `AlbumArtSide` component definition (around line 1067). Change its props interface and implementation:

```tsx
function AlbumArtSide({
  dataUrl,
  size,
  dragRegion,
  onClick,
}: {
  dataUrl: string;
  size: number;
  dragRegion: boolean;
  onClick?: () => void;
}) {
  const [hover, setHover] = useState(false);
  const px = Math.max(40, size);
  const drag = dragRegion ? { "data-tauri-drag-region": true } : {};
  const isClickable = !!onClick;

  return (
    <div
      {...drag}
      onClick={(e) => {
        if (isClickable) {
          e.stopPropagation();
          onClick();
        }
      }}
      onMouseEnter={() => isClickable && setHover(true)}
      onMouseLeave={() => isClickable && setHover(false)}
      style={{
        width: px,
        height: px,
        flexShrink: 0,
        position: "relative",
        cursor: isClickable ? "pointer" : "default",
        outline: isClickable && hover ? "1.5px solid rgba(212,175,55,0.5)" : "none",
        outlineOffset: 2,
        borderRadius: 6,
      }}
    >
      <img
        src={dataUrl}
        alt=""
        draggable={false}
        style={{
          width: "100%",
          height: "100%",
          objectFit: "cover",
          borderRadius: 6,
          boxShadow: "0 2px 8px rgba(0,0,0,0.6)",
          display: "block",
          pointerEvents: "none",
        }}
      />
    </div>
  );
}
```

- [ ] **Step 2: Update AlbumArtBadge to accept an onClick prop**

Find `AlbumArtBadge` (around line 1033). Change:

```tsx
function AlbumArtBadge({ dataUrl, onClick }: { dataUrl: string; onClick?: () => void }) {
  const [hover, setHover] = useState(false);
  const isClickable = !!onClick;
  return (
    <img
      src={dataUrl}
      alt=""
      draggable={false}
      onClick={(e) => { if (isClickable) { e.stopPropagation(); onClick(); } }}
      onMouseEnter={() => isClickable && setHover(true)}
      onMouseLeave={() => isClickable && setHover(false)}
      style={{
        position: "absolute",
        top: 8,
        left: 8,
        width: 40,
        height: 40,
        borderRadius: 4,
        objectFit: "cover",
        boxShadow: "0 2px 8px rgba(0,0,0,0.6)",
        opacity: 0.9,
        pointerEvents: isClickable ? "auto" : "none",
        cursor: isClickable ? "pointer" : "default",
        outline: isClickable && hover ? "1.5px solid rgba(212,175,55,0.5)" : "none",
        outlineOffset: 2,
      }}
    />
  );
}
```

- [ ] **Step 3: Wire the click handler in the main render**

In the Overlay component's render, find where `AlbumArtSide` and `AlbumArtBadge` are used (lines ~717, ~749, ~792).

Define the handler near the top of the Overlay render function (after the `isEdit` / `mode` variables):

```tsx
  const openArtistPanel = settings.show_artist_info_panel && mode !== "ghost"
    ? () => invoke("open_artist_panel_cmd").catch(() => {})
    : undefined;
```

Thread `onClick={openArtistPanel}` into each `AlbumArtSide` and `AlbumArtBadge` call site:

- Line ~717: `<AlbumArtSide dataUrl={albumArt.data_url} size={artSize} dragRegion={isEdit} onClick={openArtistPanel} />`
- Line ~749: `{showArt && albumArt ? <AlbumArtBadge dataUrl={albumArt.data_url} onClick={openArtistPanel} /> : null}`
- Line ~792: `<AlbumArtSide dataUrl={albumArt.data_url} size={artSize} dragRegion={isEdit} onClick={openArtistPanel} />`

- [ ] **Step 4: Add ArtistInfoDot fallback component**

Add this new component after the `UpdateBanner` component (around line 937):

```tsx
// Fallback "•••" affordance shown top-right when album art is not displayed.
// Mirrors UpdateBanner geometry — 9×9 dot, hover expands label, same anchor.
function ArtistInfoDot({ onClick }: { onClick: () => void }) {
  const [hover, setHover] = useState(false);
  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      onClick={onClick}
      style={{
        alignSelf: "flex-start",
        display: "flex",
        alignItems: "center",
        gap: 6,
        cursor: "pointer",
        userSelect: "none",
        padding: "2px 4px",
        borderRadius: 4,
      }}
    >
      <span
        style={{
          display: "inline-block",
          width: 9,
          height: 9,
          borderRadius: "50%",
          background: "#d4af37",
          opacity: 0.7,
          boxShadow: "0 0 5px rgba(212,175,55,0.5)",
          flexShrink: 0,
        }}
      />
      <span
        style={{
          fontSize: 11,
          letterSpacing: 0.3,
          color: "rgba(234,234,234,0.85)",
          fontWeight: 500,
          overflow: "hidden",
          whiteSpace: "nowrap",
          transition: "opacity 180ms ease, max-width 220ms ease",
          opacity: hover ? 1 : 0,
          maxWidth: hover ? 120 : 0,
        }}
      >
        Artist info
      </span>
    </div>
  );
}
```

Then in the main render tree, find where `<UpdateBanner .../>` is rendered (it renders as a child of `outerStackStyle`). Immediately after `<UpdateBanner .../>`, add:

```tsx
          {openArtistPanel && (!settings.show_album_art || !albumArt) ? (
            <ArtistInfoDot onClick={openArtistPanel} />
          ) : null}
```

- [ ] **Step 5: Typecheck**

```
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/Overlay.tsx
git commit -m "$(cat <<'EOF'
Overlay: artist-info click handler on album art + dot fallback affordance
EOF
)"
```

---

## Task 14 — Frontend: Settings.tsx toggle + clear-cache button

**Files:**
- Modify `src/Settings.tsx`

- [ ] **Step 1: Add the toggle and cache-clear button**

Find the existing `<Section title="Extras">` block in `src/Settings.tsx` (around line 228). Add a new `<Section title="Artist info panel">` immediately before the `<Section title="OBS / Streamer">` block:

```tsx
      <Section title="Artist info panel">
        <Toggle
          label="Show artist info panel"
          checked={s.show_artist_info_panel}
          onChange={(v) => update("show_artist_info_panel", v)}
        />
        <Hint>
          Click album art (or the dot in the top corner when art is off) to view
          artist bio, similar artists, and upcoming tour dates with ticket links.
        </Hint>
        <Row label="Cache">
          <button
            onClick={() =>
              invoke("clear_artist_info_cache")
                .then(() => alert("Artist info cache cleared."))
                .catch((e: unknown) => alert(`Failed: ${e}`))
            }
            style={dangerButtonStyle}
          >
            Clear artist info cache
          </button>
        </Row>
      </Section>
```

`dangerButtonStyle` is already defined at the bottom of `Settings.tsx` — no new styles needed.

- [ ] **Step 2: Typecheck**

```
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/Settings.tsx
git commit -m "$(cat <<'EOF'
Settings: artist info panel toggle + clear-cache button
EOF
)"
```

---

## Task 15 — Version bump + CHANGELOG + final verify

**Files:**
- Modify `package.json`
- Modify `src-tauri/tauri.conf.json`
- Modify `src-tauri/Cargo.toml`
- Modify `docs/CHANGELOG.md`

- [ ] **Step 1: Bump version**

- `package.json` line 3: `"version": "0.10.25"` → `"version": "0.11.0"`
- `src-tauri/tauri.conf.json` line 4: `"version": "0.10.25"` → `"version": "0.11.0"`
- `src-tauri/Cargo.toml` line 3: `version = "0.10.25"` → `version = "0.11.0"`

Also update the `USER_AGENT` string in `src-tauri/src/artist_info.rs` (the `build_artist_info_http_client` function) from `hum/0.11.0` (which was already written correctly in Task 6) — no change needed if you wrote it correctly then.

Also update the `USER_AGENT` const in `src-tauri/src/lyrics.rs` from `hum/0.10.20` to `hum/0.11.0`.

- [ ] **Step 2: Prepend CHANGELOG entry**

At the top of `docs/CHANGELOG.md`, immediately after the file header (before the existing `## [0.10.25]` block), insert:

```markdown
## [0.11.0] - 2026-05-21

### Added
- **Artist-info panel — click album art to see bio, similar artists, tour dates, and buy tickets.** In edit and locked modes, clicking the album art square (visible in the 3-line and single-line layouts as the square image to the left of the lyrics; in the full-page layout as the small badge in the top-left corner) opens a new floating window showing information about the currently playing artist. When album art is hidden or unavailable, a small gold "•••" dot appears in the top-right corner of the overlay (same anchor as the update banner); hovering it expands an "Artist info" label, clicking it opens the panel. The panel is not available in ghost mode, consistent with ghost's "no chrome, click-through" design. The click affordance shows a 1.5px gold outline on hover so users know it is interactive; tooltip is omitted in the full-page layout where the badge is too small.

  The panel window (labeled `artist-info` internally) is 360×480px, transparent, always-on-top, no OS titlebar, and floats 8px below the overlay by default. If the overlay is within 500px of the screen's bottom edge, the panel anchors above the overlay instead. It can be dragged from its header to any screen position after opening. The panel closes via: the × button in the panel header, the ESC key, or automatically when the SMTC source switches to a different artist (same artist / new track keeps the panel open — the panel is artist-keyed, not track-keyed).

  **What the panel shows, top to bottom:**
  - **Header:** 60×60 round artist photo (from TheAudioDB) + artist name (18px, weight 600) + × close button (gold on hover). Header is the drag region.
  - **Bio section:** Last.fm artist bio prose, truncated at the last sentence before 1,500 characters. "Read more on Last.fm →" link. Section hidden entirely when bio is unavailable — no "no bio" placeholder.
  - **Similar artists section:** Up to 8 similar artists from Last.fm, comma-separated, prefixed with a gold "Similar to" section label. Section hidden when empty.
  - **Upcoming shows section:** Up to 10 upcoming tour dates from Bandsintown, sorted by date. Each row shows the date (gold, monospace, "Mar 5" format, year included only if not the current year), city+region (or city+country for international), venue in italic dim text, and a gold "[Tickets]" button right-aligned. Clicking Tickets opens the Bandsintown affiliate URL in the user's default browser — Bandsintown routes the click through Ticketmaster, SeatGeek, AXS, or Live Nation depending on the event. Sold-out events show a gray "[Sold Out]" non-clickable button instead. Empty state shows "No upcoming tour dates." in dim italic text. When more than 10 events exist, shows "View all on Bandsintown →" after the first 10.
  - **Footer:** "Powered by Bandsintown · Last.fm · TheAudioDB" in 10px dim centered text; each name is clickable and opens the respective service's website.

- **Affiliate ticket links via Bandsintown partner program.** Every ticket click from the panel routes through Hum's partner `app_id`. Affiliate revenue accrues to Wes on every user's click — no per-user setup required. The implementation ships with a `hum-dev` placeholder `app_id`; replace with the live partner ID from https://bandsintown.com/partners before public release.

- **Settings: "Show artist info panel" toggle** in Settings → Artist info panel section. Disabling it hides the click affordance on the album art and the fallback "•••" dot; any open panel closes. Below the toggle, a "Clear artist info cache" button wipes the on-disk artist cache (`%APPDATA%\com.syvr.hum\cache\artist\`).

### Architecture / files
- **New `src-tauri/src/artist_info.rs`** — All data types (`ArtistInfo`, `ArtistBio`, `TourDate`, `TicketStatus`); pure helpers `slug_for_artist` (diacritic-mapped, alphanumeric-only slug), `tour_dates_stale` (12-hour TTL check), `strip_html` (regex tag stripper + entity decode). Source fetchers: `fetch_lastfm_bio`, `fetch_lastfm_similar` (Last.fm REST), `fetch_bandsintown_events` (Bandsintown REST, ISO8601 date parser), `fetch_theaudiodb_photo` (TheAudioDB, base64-inline image), `resolve_mbid_musicbrainz` (MusicBrainz fallback). Orchestrator: `ArtistInfoCache` Tauri managed state — `fetch()` reads disk cache, returns immediately on fully-fresh data, refetches only tour dates when stale (≥12h), fires a full `tokio::join!` parallel fetch on cache miss with MusicBrainz fallback on Last.fm error 6. In-flight dedup via `Arc<Mutex<HashMap<String, Arc<Notify>>>>`. Disk cache at `%APPDATA%\com.syvr.hum\cache\artist\{slug}.json`, one JSON file per artist, version field for future schema evolution. Tauri commands: `get_artist_info`, `clear_artist_info_cache`.
- **New `src-tauri/src/artist_window.rs`** — `open_artist_panel` (creates `WebviewWindowBuilder` for label `artist-info`, computes anchor position from `overlay.outer_position` + `outer_size` + monitor height, auto-close listener via `app.listen("track-changed")`), `close_artist_panel`, `open_ticket_url` (URL host whitelist: bandsintown.com, ticketmaster.com, seatgeek.com, axs.com, livenation.com, last.fm, theaudiodb.com, musicbrainz.org). Uses `opener` crate for `shell.open` equivalent.
- **`src-tauri/src/lib.rs`** — added `mod artist_info; mod artist_window;`. `ArtistInfoCache::new(app.handle().clone())` managed in setup hook. Six new commands registered in `invoke_handler!`: `get_artist_info`, `clear_artist_info_cache`, `open_artist_panel_cmd`, `close_artist_panel_cmd`, `open_ticket_url`.
- **`src-tauri/src/settings.rs`** — new `show_artist_info_panel: bool` field, default `true`.
- **`src-tauri/Cargo.toml`** — added `urlencoding = "2"` and `opener = "0.7"`. Version bumped to `0.11.0`.
- **`src-tauri/capabilities/default.json`** — added `artist-info` to windows scope; added `core:window:allow-close`, `core:window:allow-set-position`, `core:window:allow-set-size`, `core:webview:allow-create-webview-window`.
- **`src/types.ts`** — `Settings` extended with `show_artist_info_panel: boolean`; new types `ArtistInfo`, `ArtistBio`, `TourDate`, `TicketStatus`.
- **`src/Overlay.tsx`** — `AlbumArtSide` and `AlbumArtBadge` accept optional `onClick?: () => void`; gold outline + pointer cursor on hover when onClick is provided. New `ArtistInfoDot` component (mirrors `UpdateBanner` geometry — 9×9 gold dot, hover-expand label). `openArtistPanel` handler gated on `settings.show_artist_info_panel && mode !== 'ghost'`. `DEFAULT_SETTINGS` updated with `show_artist_info_panel: true`.
- **`src/Settings.tsx`** — new `<Section title="Artist info panel">` with `<Toggle>` and cache-clear `<button>`.
- **New `src/artist-panel/index.html`**, **`src/artist-panel/main.tsx`**, **`src/artist-panel/ArtistPanel.tsx`** — second Vite entry point; React panel component with header/bio/similar/tour-dates/footer sections.
- **`vite.config.ts`** — `build.rollupOptions.input` added for multi-page build (`main` + `artistPanel`).
```

- [ ] **Step 3: Final verification — Rust tests**

```
cd src-tauri && cargo test --lib
```

Expected: PASS — all Task 1 unit tests pass (`slug_for_artist` ×7, `tour_dates_stale` ×4, `strip_html` ×5) plus all pre-existing tests (settings serde, backdrop tests).

- [ ] **Step 4: Final verification — Rust clippy**

```
cd src-tauri && cargo clippy --lib -- -D warnings
```

Expected: PASS, no warnings.

- [ ] **Step 5: Final verification — TypeScript**

```
pnpm typecheck
```

Expected: PASS, no errors.

- [ ] **Step 6: Final verification — Vite build**

```
pnpm build
```

Expected: PASS — multi-page build succeeds, `dist/` contains both entry points.

- [ ] **Step 7: Manual verification checklist (from spec)**

These are run in `pnpm tauri dev` (or a dev build) with Hum's overlay visible over a media source:

- [ ] Click album art with Shaggy playing → panel opens below overlay, populates within 2–3s, shows bio + similar artists + Bandsintown events.
- [ ] Click [Tickets] on an event → browser opens to a `bandsintown.com/event/...` URL (confirms affiliate routing).
- [ ] Switch track to a different artist → panel auto-closes.
- [ ] Switch track to same artist different song → panel stays open.
- [ ] Toggle Settings "Show artist info panel" off → panel closes; album art click no longer triggers anything.
- [ ] Cache hit on second open (same artist) → populates in <100ms (cache read).
- [ ] Offline (disable network) → panel shows "Couldn't load artist info" with Retry button.
- [ ] Artist with no upcoming tour dates → "No upcoming tour dates." empty state.
- [ ] Ghost mode → album art click does nothing; "•••" dot not rendered.
- [ ] Backdrop matches overlay setting (open panel with Acrylic on overlay → panel also shows Acrylic).
- [ ] Position: drag overlay near bottom of screen, open panel → opens above the overlay.
- [ ] Clear artist info cache from Settings → re-open same artist → fetches fresh from network (>100ms).

- [ ] **Step 8: Final commit**

```bash
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json docs/CHANGELOG.md src-tauri/src/lyrics.rs
git commit -m "$(cat <<'EOF'
v0.11.0: artist-info / tour-dates / ticket-affiliate panel
EOF
)"
```

---

*Plan written 2026-05-21. Target version: 0.11.0. 15 tasks. Spec: `docs/superpowers/specs/2026-05-21-hum-artist-info-panel-design.md`.*
