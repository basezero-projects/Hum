# HUM-10G paid launch plan

Date: 2026-08-19
Status: In progress
Related phase: [HUM-10](../../roadmap/1.0/HUM-10-purchase-trust-first-run.md)
Progress ledger: [HUM-10 ledger](../../roadmap/1.0/HUM-10-progress-ledger.md)
Website: `D:/Work/App_Projects/All_Projects/Websites/sites/hum-site`

## Outcome

Hum behaves like a paid product by default. Promotional cards start off, the desktop release knows where to send customers for purchase and recovery, and the public website hands a buyer to a validated Polar checkout with final pricing, refund, device, delivery, privacy, and support terms.

## Locked purchase and delivery flow

- Hum costs $19 once and includes Hum 1.x updates.
- One license covers three Windows devices.
- Full refunds are available for 30 days.
- Polar is the Merchant of Record and hosted checkout provider.
- The Polar product is a one-time fixed-price product with a license-key benefit limited to three activations.
- The signed Windows installer is a Polar file-download benefit.
- Polar's purchase email links to the customer portal. The portal provides the receipt, license key, signed installer, and activation management.
- Hum does not promise that the installer is attached to the purchase email.
- The checkout Success URL points to a static, noindex purchase-complete page and includes `checkout_id={CHECKOUT_ID}`. The Return URL points to Pricing.
- No webhook, custom mailer, checkout bridge, account system, or Hum-hosted download service is part of this slice.

This contract is supported by Polar's current product, license-key, file-download, checkout-link, customer-portal, refund, and sandbox documentation. Research was completed through the required local Firecrawl stack on 2026-08-19.

## Desktop promo policy

- Fresh installs and Reset Settings start with promotional cards off.
- Existing settings receive a one-time migration to off, even when their stored value is on. The old schema cannot distinguish an implicit default from a deliberate opt-in, so the paid-safe choice wins once.
- The migration stores a promo-policy version. A customer who opts in after migration keeps that choice on later launches.
- Overlay hydration fails closed with promos off.
- Settings describes promotional cards as optional Hum offers during ad breaks and states that they are off by default.
- When promos are off, every layout keeps the existing neutral Ad break presentation. Lyrics timing, OBS, licensing, and remote promo fetching do not change.

## Public configuration contract

The signed desktop workflow receives these public GitHub repository variables:

- `HUM_POLAR_ORGANIZATION_ID`
- `HUM_POLAR_CHECKOUT_URL`
- `HUM_POLAR_CUSTOMER_PORTAL_URL`

The website receives:

- `PUBLIC_CHECKOUT_URL`
- `PUBLIC_SITE_URL`

Provider administration tokens remain in Arcanum and provider dashboards. They never enter source, GitHub variables, Vercel public variables, desktop binaries, or support diagnostics.

## Desktop production boundary

- Modify `src-tauri/src/settings.rs`.
- Modify `src/Overlay.tsx`.
- Modify `src/Settings.tsx`.
- Add `src/promo-policy.ts`.
- Add `src/promo-policy.test.mts`.
- Modify `.github/workflows/release.yml`.
- Modify `scripts/prepare-release.test.mjs`.
- Modify `.env.example`.
- Modify `src-tauri/src/license/commands.rs` to separate checkout and portal URL contracts.
- Modify `src-tauri/src/lyrics.rs` for the disabled-promo runtime regression.

## Website production boundary

- Add `src/lib/checkout.ts`.
- Add `src/lib/checkout.test.mts`.
- Add `scripts/launch-contract.test.mjs`.
- Modify `src/components/PurchaseButton.astro`.
- Modify `src/components/SiteFooter.astro`.
- Modify `src/pages/index.astro`.
- Modify `src/pages/buy.astro`.
- Modify `src/pages/pricing.astro`.
- Modify `src/pages/terms.astro`.
- Modify `src/pages/privacy.astro`.
- Modify `src/pages/download.astro`.
- Add `src/pages/support.astro`.
- Add `src/pages/purchase-complete.astro`.
- Modify `astro.config.mjs`.
- Delete `public/robots.txt` and add `src/pages/robots.txt.ts` so the sitemap follows the validated site origin.
- Add `src/lib/site-origin.mjs`.
- Add `src/lib/site-origin.test.mjs`.
- Modify `.env.example`.
- Modify `package.json`.

Standard closeout files are also expected in both repositories: version manifests, changelogs, this plan, the HUM-10 roadmap and ledger, `BUGS.md` when needed, the Hum website studio state, plus Hum and Websites brain records.

## Website checkout and search contract

- One shared parser accepts only approved Polar HTTPS checkout hosts and rejects missing, HTTP, credentialed, fragmented, deceptive-subdomain, or unrelated values.
- Purchase buttons and `/buy` use the same parsed result. Invalid or missing configuration stays honest and falls back to support email.
- `/buy` and `/purchase-complete` stay out of search results and the sitemap. Pricing and product pages own purchase search intent.
- `PUBLIC_SITE_URL` controls canonicals, Open Graph URLs, and the sitemap. Until the custom domain works, the deployed Vercel URL is the honest default.
- Product schema reports availability only when checkout configuration is valid. It must not advertise a purchasable product when checkout is disconnected.
- Pricing, Buy, Terms, Privacy, Download, Support, footer, and purchase completion agree on price, device count, covered updates, refund window, Polar's role, portal delivery, and support email.
- The Support page has one clear purpose, one h1, unique metadata, and a stable `hello@syvr.dev` route.

## Required red-first tests

### Desktop

- Fresh and reset settings default promos off.
- Legacy settings migrate promos off exactly once.
- A current-policy explicit opt-in survives reload and sanitize.
- Disabled promos emit Ad status without a card or cooldown mutation.
- Frontend fallback is off and Settings uses the approved opt-in description.
- The release workflow supplies all three public Polar values and rejects a packaging run when any is missing.
- Existing signed updater, license, overlay, and settings tests remain green.

### Website

- Missing or hostile checkout URLs fail closed.
- Approved Polar production and sandbox URLs remain exact and usable.
- PurchaseButton and Buy share the parser instead of opening raw environment input.
- Final purchase copy contains $19 once, three devices, Hum 1.x updates, 30-day refunds, Polar portal delivery, and `hello@syvr.dev` wherever relevant.
- Terms contain no launch-draft or unfinished-refund language.
- Download does not promise a direct installer attachment.
- Support and purchase-complete pages use unique metadata, one h1, safe links, and the intended robots behavior.
- Product schema availability follows validated checkout configuration.
- The rendered build keeps unique titles, descriptions, canonicals, sitemap entries, and robots exclusions.

## Strict execution order

1. Add failing desktop promo and workflow tests, then implement the paid-safe migration and public build configuration.
2. Run the complete Hum debug and release gate.
3. Add failing website checkout and launch-contract tests, then implement the validator, final copy, support, completion, canonical, schema, and robots changes.
4. Run the complete website check, build, test, and high-severity audit gate.
5. Run independent cold reviews for both repositories and fix findings red-first.
6. Close, commit, and push the website. Verify the Vercel deployment with checkout still safely disconnected.
7. Close, version, commit, and push the desktop changes.
8. Configure Polar sandbox and prove checkout, portal delivery, license retrieval, and cancellation.
9. Configure the production Polar product, benefits, checkout link, portal URL, GitHub variables, and Vercel checkout variable.
10. Stop for approval before purchasing `humlyrics.com`. After purchase, attach apex and `www` to Vercel, set `PUBLIC_SITE_URL`, verify DNS, TLS, redirects, canonicals, sitemap, Privacy, and Support, then deploy.
11. Complete one real or provider-approved test purchase, retrieve the license and signed installer without developer intervention, activate a release build, open the portal, and prove promos remain off unless explicitly enabled.

## Review amendment

The first cold review found four gaps inside the locked mechanisms: checkout validation accepted general Polar pages while rejecting the official sandbox redirect, `PUBLIC_SITE_URL` was not validated, static robots content could not follow the final domain, and the disabled-promo runtime branch lacked a regression test. The boundary expands from twenty-five to twenty-nine files to split desktop checkout and portal validation, add the Rust false-promo test, validate the site origin, and generate robots content from that origin. This remains below the amendment stop threshold and adds no dependency or new product mechanism.

## Honest proof split

Local automation proves promo migration, explicit opt-in persistence, URL validation, fallback behavior, copy consistency, schema and robots output, frontend and Rust builds, and release-variable wiring.

Polar sandbox proves checkout, return, success, key issuance, file benefit, portal access, and cancellation without a real charge. Production configuration proves the actual public resource identities. A real purchase or provider-approved production test proves the final customer handoff.

HUM-10H retains three successful activations, fourth-device rejection, device release, reinstall restore, refund revocation, Windows scaling, clean install, and prior-version update proof.

## Execution status

- Desktop implementation is complete in v0.13.76. Fresh, reset, and migrated installs start with promos off, explicit later opt-in persists, release builds require exact Polar public configuration, and the complete debug plus release gate passed independent review.
- Website implementation shipped as v0.1.2 at commit `e52b4a4`. The Vercel production deployment passed live route, browser-console, canonical, robots, sitemap, noindex, disconnected-checkout, and schema checks at `https://hum-site.vercel.app`.
- Polar sandbox, production provider resources, checkout delivery, and customer handoff proof remain open.
- `humlyrics.com` remains unpurchased. Domain work stays behind the explicit spending approval in this plan.

## Acceptance criteria

- HUM-10-AC9 is complete: fresh, reset, and migrated paid installs start with promotional cards off, while a later explicit opt-in persists.
- Every website purchase surface publishes the locked policy without draft or contradictory language.
- Invalid checkout configuration cannot create a live purchase button.
- A valid Polar checkout link works from the website and desktop release.
- The Polar portal gives a buyer their license key, signed installer, receipt, and activation controls without developer intervention.
- Support and Privacy are publicly reachable on the final HTTPS domain.
- Local app and website gates, independent reviews, deployment checks, provider proof, and domain proof all pass.

## Amendment gate

Stop for approval if Polar needs a webhook or custom mailer, installer delivery needs Hum-hosted storage, checkout needs a custom domain or provider outside the approved Polar hosts, the slice needs a new runtime dependency, more than thirty production files, or any change to price, device count, refund policy, update entitlement, or license verification policy.

Domain purchase is a separate spending approval. Do not buy `humlyrics.com` without Wes's explicit approval.

## Closeout

Before HUM-10H starts: complete both repository gates, independent reviews, Vercel deployment proof, Polar sandbox and production proof, approved domain purchase and HTTPS proof, a real customer handoff proof, version bumps and changelogs in both repositories, BUGS review, roadmap and studio ledgers, both brain records, commits, pushes, and exact-commit verification.
