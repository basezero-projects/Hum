# Hum Pro Tier — Monetization Plan

**Status:** Strategy locked. Implementation deferred until Wes greenlights build.
**Decision date:** 2026-05-22
**Current version at plan time:** v0.13.0

---

## TL;DR

Hum stays **free + closed-source**, monetized via the SYVR PromoCard ad slot (already shipped in v0.12–v0.13). On top of that, ship **Hum Pro** as a one-time paid tier whose single core value prop is:

> **Replace the ad-break promo slot on your own machine with your own images.**

Pro is local-only. No ad network, no cross-user distribution, no cloud, no accounts. Just a license key that unlocks "use my own PNGs in the ad slot" plus a small set of quality-of-life features that justify the price.

Free users keep seeing SYVR/sponsor promos and feed the ad-revenue compound. Pro users pay once, see only their own content, and unlock B2B-shaped utility (streamers, cafés, gyms, retail, indie musicians, podcasters).

---

## Why this model and not the alternatives

| Considered | Rejected because |
|---|---|
| **Pure free + ad slots only** | Leaves money on the table from users who *want* their own promo inventory and would happily pay for it. The infrastructure to support them is 80% built. |
| **Pure paid one-time** | Friction kills the streamer/Reddit/OBS distribution flywheel. Charging for a lyric overlay loses against Spotify's built-in lyrics + free competitors like Spicetify. Sacrifices the SMTC-everywhere moat. |
| **Open source + paid auto-updater convenience** | Open source + a local license check is forked and bypassed within 48 hours. The auto-updater is a free GitHub Releases poll — nothing meaningful to charge for. Tauri builds are too painful for non-devs to use the "build it yourself" escape valve in practice, so it's "open source in name only" and gets roasted as such. |
| **Subscription** | Wrong shape for a single-purpose desktop utility. Users hate recurring fees on small tools. One-time + lifetime updates is what this market pays for. |

**Why "bring your own promos" wins as the Pro feature:**

- Tangible utility, not aesthetics — selling **inventory the user can monetize**, not "premium themes."
- Higher price tolerance — streamers and small businesses see a path to recouping the $15 within a week.
- Opens a B2B segment (physical-space buyers: cafés, gyms, retail, barbershops) that's invisible to other lyric overlays.
- Dual revenue stays intact: ~95% Free users feed ad-slot compound revenue, ~5% Pro users pay direct.
- Build cost is low — the PromoCard renderer is already shipped (v0.13). Pro is mostly UI + license gate on top.

---

## The Free / Pro split

| Capability | Free | Pro |
|---|---|---|
| Full lyrics + per-word karaoke sweep | ✅ | ✅ |
| All player sources (SMTC, iTunes, Pandora, YouTube, Spotify) | ✅ | ✅ |
| Auto-contrast text + album-art tinting | ✅ | ✅ |
| OBS streamer mode (axum HTTP) | ✅ | ✅ |
| Artist info panel (Wikipedia + TheAudioDB + Ticketmaster) | ✅ | ✅ |
| Global hotkeys + offset nudge | ✅ | ✅ |
| Auto-updater via GitHub Releases | ✅ | ✅ |
| **Ad-break slot content** | SYVR / sponsor PromoCards from `promos.json` | **User's own uploaded PNGs, local only** |
| Custom promo library (1–20 images, add/remove/reorder) | ❌ | ✅ |
| Promo rotation (random or sequential) | ❌ | ✅ |
| Per-promo display duration | ❌ | ✅ |
| Optional click-through URL per promo (opens in default browser) | ❌ | ✅ |
| In-app simple template builder (text + bg + logo → 1920×240 PNG) | ❌ | ✅ (v1.1 — see Roadmap) |
| Custom font upload | ❌ | ✅ |
| Extra theme presets | ❌ | ✅ |

**Critical design rule:** Pro users see **100% their own promos**, never SYVR promos. The Pro fee replaces what SYVR would have earned in ad inventory from that user. Clean ethics, clean marketing.

---

## Pricing

**Launch with one tier:**

- **Hum Pro: $14.99 one-time, lifetime updates**

**~6 months post-launch, evaluate adding:**

- **Hum Pro Business: $39.99 one-time** — multi-machine license (3–5 devices), promo scheduling (time-of-day rotation: café morning vs evening), basic local view counters per promo. Let real B2B inquiries dictate whether this tier is needed.

**Don't ship:** free trial. The model is "buy the right to put your own image in your own ad slot" — there's no trial-shaped version of that. Sell with a demo video and screenshots instead.

---

## What's already built vs what needs building

### Already shipped (v0.13.0)
- ✅ Ad-break detection across Spotify / Pandora / YouTube
- ✅ PromoCard rendering with image-driven 1920×240 hero format
- ✅ Hot-swappable `promos.json` with disk cache + bundled fallback
- ✅ Per-promo display duration + click-through URL
- ✅ Weighted-random rotation with last-shown cooldown
- ✅ Settings store via `tauri-plugin-store`
- ✅ Auto-updater via `tauri-plugin-updater` + GitHub Releases

### To build for Pro v1
- ⏳ License key system (purchase → activation → local validation)
- ⏳ License server endpoint (likely on existing Hetzner box — `simsweep-auth` pattern is the precedent)
- ⏳ `PromoSource` trait already architected per `ad-break-detection` plan; add `UserLocalSource` implementation
- ⏳ Settings UI: "Ad break replacements" panel with drag-drop library, rotation mode toggle, per-promo settings
- ⏳ Image validation with smart auto-fit + preview (do NOT reject non-1920×240 uploads — letterbox / center-crop with a live preview)
- ⏳ License gate that flips `PromoSource` from `SyvrRemoteSource` to `UserLocalSource` when active

### To build for Pro v1.1
- ⏳ In-app template builder (text + background color + logo upload → exports compliant 1920×240 PNG)
- ⏳ Custom font upload UI
- ⏳ Additional theme presets

### To build for Pro Business (deferred)
- ⏳ Multi-machine license activation (3–5 devices per key)
- ⏳ Time-of-day promo scheduling
- ⏳ Local view counters per promo

**Effort estimate (Pro v1):** ~1–2 focused weekends. License server is the biggest unknown — Hetzner pattern from `simsweep-auth` should keep it small.

---

## Architecture sketch

Reuses the `PromoSource` trait already designed in `2026-05-22-hum-ad-break-detection.md`:

```rust
// src-tauri/src/promos.rs
trait PromoSource {
    fn next_promo(&self) -> Option<Promo>;
}

struct SyvrRemoteSource { ... }      // existing — Free users
struct UserLocalSource { ... }       // NEW — Pro users, reads %APPDATA%\com.syvr.hum\promos\
```

The rotation engine selects which source is active based on `is_pro_licensed()`:

```rust
let source: Box<dyn PromoSource> = if license::is_pro_active() {
    Box::new(UserLocalSource::load_from_appdata()?)
} else {
    Box::new(SyvrRemoteSource::with_fallback_chain())
};
```

No conditional logic in the renderer — `PromoCard` just consumes whatever `next_promo()` returns.

**License storage:** local file at `%APPDATA%\com.syvr.hum\license.json` containing the activation token + machine fingerprint. License server validates the token on first activation and on a periodic (weekly) heartbeat; offline grace period of 30 days. Light DRM — this audience overwhelmingly pays.

**User promo storage:** `%APPDATA%\com.syvr.hum\promos\` directory. Settings UI is a thin wrapper over the filesystem. Drag-drop adds files; remove deletes them; rotation order is a JSON manifest in the same folder.

---

## Launch plan

### Hard sequencing rule
**Do not announce Pro until it's done.** Launching with "Pro coming soon" splits the marketing energy and dilutes the Free push. Launch Free first, build the user base, then drop Pro when it's polished.

### Phase 1 — Free launch (current focus)
- Polish v0.13.x — fix anything in `BUGS.md` that touches the launch story
- Ship Hum site page at `syvrstudios.com/hum` — demo GIF + GitHub Releases link + the pitch
- Pitch lines for the free launch:
  > "Free lyric overlay for Windows. Spotify, YouTube, Pandora, iTunes — anything. Even shows you something useful during ad breaks instead of silence."
- One thoughtful Reddit post per sub (NOT spam): r/spotify, r/Twitch, r/obs, r/pandora, r/youtubemusic
- Twitter/X demo video: per-word karaoke sweep + ad-break PromoCard fade
- Direct DM ~20 mid-tier streamers (300–3K viewers): free OBS browser source, here's the .exe
- Product Hunt launch at ~500 organic downloads

### Phase 2 — Pro launch (when ready)
- Build Pro UI + license server (1–2 weekends)
- Ship Pro v1 at $14.99 one-time
- Pitch line:
  > "Pro replaces the ads YOU see with the images YOU upload. Local only. No accounts, no cloud, just your folder of PNGs."
- Update Reddit/Twitter posts in the relevant niches: r/Twitch, r/obs, r/smallbusiness, r/musicproduction
- Add a "Buy Pro" page at `syvrstudios.com/hum/pro` with use-case-specific demo GIFs:
  - Streamer with Discord/Patreon promo
  - Café with daily-special menu board
  - Indie musician with merch + tour-date promo
- Email/DM the streamers who already adopted Free: "Pro is live, here's a comp license if you want to try it"

### Phase 3 — Pro Business (~6 months later)
- Only if real B2B inquiries materialize
- Don't pre-build — wait for someone to email "I run a café and want to buy 5 licenses"

---

## What "monetization" looks like in compound

After 12 months, assuming the model works:

| Revenue line | Compounds with | Mechanism |
|---|---|---|
| **SYVR ad-slot promos** (cross-funnel) | Free users | Drives signups to Stub, Wren, Trellis, Arcanum, SimSweep — measured in cross-app activations, not direct $ |
| **Sponsored promo slots** (when filled) | Free users | Sell `promos.json` slots to audio gear, VPN, indie tour promoters once Hum hits ~5K installs |
| **Ticketmaster + concert affiliate clicks** | Free users + Pro users (the artist panel runs for everyone) | Already shipped via impact.com; expand to Songkick, Bandsintown, Apple Music, Amazon Music |
| **Hum Pro one-time fees** | Pro users | Direct $14.99/sale, lifetime updates |
| **Hum Pro Business one-time fees** | B2B buyers (later) | Direct $39.99/sale × multi-device |

**The key dynamic:** Free user growth never stops paying off because it grows three revenue lines (ad slot, sponsored slots, affiliate clicks) AND drives Pro conversion. The ad slot is the engine; Pro is the high-margin add-on.

---

## Open decisions (need Wes's call before build starts)

1. **License server hosting** — confirm Hetzner box + which subdomain (e.g., `hum-license.syvr.dev` or fold into an existing service?)
2. **License storage format** — JWT? Plain JSON with signed token? Match the simplest pattern that already exists in `simsweep-auth`.
3. **Multi-machine policy for Pro v1** — single-device or 2-device? Recommendation: **2-device** (desktop + laptop is common). Business tier is where 3–5 lives.
4. **Refund policy** — recommend 30-day no-questions refund. Reduces sales friction; abuse rate on $15 desktop apps is negligible.
5. **Payment processor** — Stripe is the obvious default (already in the SYVR stack). Polar.sh also viable since it's already in play for Loomwerks. Pick whichever has the least friction for Wes's tax setup.
6. **Pricing currency localization** — start USD-only; Stripe handles foreign card conversion automatically. Localize prices later if conversion data warrants.
7. **Demo / preview mode** — do free users see ANY Pro UI hints in Settings (greyed-out "Upgrade to use your own promos" link)? Recommendation: **yes**, one subtle link in the Ad-break panel. Don't nag, don't pop modals, but make Pro discoverable.

---

## Anti-patterns to avoid

- ❌ Do **not** open source the core. You can't un-open-source it, and the fork-and-bypass risk is fatal to any local paywall.
- ❌ Do **not** ship a free trial. Sell with a demo video instead.
- ❌ Do **not** make Pro a subscription. One-time + lifetime updates is the right shape.
- ❌ Do **not** show SYVR promos to Pro users. They're paying for inventory; respect that.
- ❌ Do **not** build an ad network. Pro is local-only forever.
- ❌ Do **not** ship the Pro license check before the user-promo UI is ready. Pro launches as one polished product, not in pieces.
- ❌ Do **not** announce Pro publicly before it's built. Splits the marketing oxygen.

---

## References

- `docs/superpowers/specs/2026-05-22-hum-ad-break-detection-design.md` — PromoSource trait architecture
- `docs/superpowers/plans/2026-05-22-hum-ad-break-detection.md` — Phase 1 promo infrastructure
- `docs/CHANGELOG.md` — v0.12.0 onwards for PromoCard implementation history
- Hetzner stack reference: see `feedback`/`reference` memory entries for `simsweep-auth` license server pattern
