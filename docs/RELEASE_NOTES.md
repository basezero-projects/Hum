# Release Notes — Hum

User-facing notes for the current release. For the full per-commit history, see `docs/CHANGELOG.md`.

## v0.13.0 — 2026-05-22

**Promo cards can now use full hero images.** When an ad break fires on Spotify free, Pandora, or YouTube, Hum replaces the lyric area with a SYVR Studios promo card. Until this release the card was text-only (product name, tagline, CTA). Now a promo entry can also ship a designed hero image that fills the lyric area edge-to-edge, the same way a real Spotify ad looks.

If an image fails to load (offline, blocked, 404), the card falls back to the text layout for that promo. You never see a broken-image gap.

Existing installs pick up the change automatically on the next launch or within 6 hours, whichever comes first — promos are served from `https://syvrstudios.com/hum/promos.json` and refreshed in the background.

No setting changes needed. The "Show SYVR promo cards during ad breaks" toggle in Settings still works the same way (default on).
