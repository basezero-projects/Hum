# HUM-10 progress ledger

This file is the durable execution cursor for HUM-10. Update it at every slice boundary before starting the next slice.

## Current cursor

- Slice: HUM-10G, paid-product defaults and checkout completion
- Step: Polar production account review
- Next action: Wes completes Stripe photo-ID verification and payout-account setup, then the account review can be submitted and the first real-payment journey can be proven
- Blocker: The remaining Polar steps require Wes's government ID and private payout details. Domain purchase still requires Wes's explicit spending approval.
- Last updated: 2026-08-20 11:59 MDT
- Last completed plan: `docs/superpowers/plans/2026-08-19-hum-10f-trust-support.md`
- Current plan: `docs/superpowers/plans/2026-08-19-hum-10g-paid-launch.md`

## Locked product policy

- Price: $19 one time
- Provider: Polar hosted checkout and license keys
- Devices: three active Windows devices per personal license
- Refunds: 30 days for a full refund
- Updates: Hum 1.x included
- Verification: every 30 days when online, followed by 30 days of offline grace after a service or network failure
- Storage: Windows DPAPI-protected record outside settings
- Accounts: none required for normal app use

## Slice sequence

1. HUM-10A, license policy and entitlement state
2. HUM-10B, protected storage and Polar client
3. HUM-10C, activation and restore experience
4. HUM-10D, first-run setup
5. HUM-10E, signed updates and update states
6. HUM-10F, trust and support surfaces
7. HUM-10G, paid-product defaults and checkout completion
8. HUM-10H, release-path proof

## Completed slices

### HUM-10A, license policy and entitlement state

- Status: Complete
- Version: 0.13.62
- Commit: closeout commit titled `Define the Hum license policy`
- Remote: pushed to `origin/main`
- Validation: frontend frozen install, typecheck, and build passed; 202 Rust tests passed; Cargo all-target check passed; Clippy passed with warnings denied; full-tree Rust formatting passed; diff check passed
- Review: the changed-file audit found an overdue verification countdown pointing at the past deadline; a failing boundary test proved it, the countdown was corrected to the grace deadline, and the full gate passed again
- Acceptance criteria: no HUM-10 phase criterion claimed by this foundation alone
- Known deferrals: protected storage, Polar network calls, activation UI, overlay gating, and production provider setup remain in HUM-10B onward

### HUM-10B, protected storage and Polar client

- Status: Complete
- Version: 0.13.63
- Commit: closeout commit titled `Protect Hum license activations`
- Remote: push to `origin/main` during closeout
- Validation: frozen install, frontend typecheck and build, 229 Rust tests, debug and release all-target checks, Clippy with warnings denied in both profiles, full Rust formatting, and diff validation
- Review: red-first repairs covered activation ID redaction, malformed grant rollback, concurrent operation serialization, release startup without provider configuration, and release-only ownership compilation
- Acceptance criteria: the protected backend supports HUM-10-AC1 through AC3, but none are checked until HUM-10C exposes and enforces the customer workflow
- Known deferrals: activation UI, checkout, restore presentation, entitlement enforcement, and live Polar proof remain in HUM-10C onward

### HUM-10C, activation and restore experience

- Status: Complete
- Version: 0.13.64
- Commit: closeout commit titled `Add the Hum activation experience`
- Remote: push to `origin/main` during closeout
- Validation: frozen frontend install, typecheck, and production build passed; 234 Rust tests passed; debug and release all-target checks passed; debug and release Clippy passed with warnings denied; full Rust formatting and diff validation passed
- Review: red-first repairs made the final entitlement the sole deactivation success criterion, kept failed device release visible and recoverable, scoped accessibility errors to the action that produced them, and removed misleading retry and device-management actions when no protected license exists
- Visual proof: the native release build showed the license window while the overlay remained hidden; the default 780 by 600 and minimum 680 by 520 layouts had no clipping or unintended scrolling; keyboard submission, key reveal and hide, close and reopen, safe missing-checkout errors, and secret redaction were exercised
- Acceptance criteria: HUM-10-AC1 through HUM-10-AC3 are complete for the local paid-product workflow
- Known deferrals: the production Polar organization, Hum product, hosted checkout, receipt delivery, customer portal, and real purchase proof remain in HUM-10G and HUM-10H

### HUM-10D, first-run setup

- Status: Complete
- Version: 0.13.65
- Commit: closeout commit titled `Polish Hum's first run and restore lyrics`
- Remote: push to `origin/main` during closeout
- Validation: frozen frontend install, typecheck, and production build passed; 244 Rust tests passed with one network test intentionally ignored; debug and release all-target checks passed; debug and release Clippy passed with warnings denied; full Rust formatting and diff validation passed; the ignored live NetEase smoke test passed separately against two exact songs
- Review: red-first tests protected the customer-window plan, route recommendation, wire contract, backward-compatible setup version, reset-safe completion, exact song matching across video-duration differences, and title-only duration safety. Native replay found and repaired a persistent hidden-window busy state plus a provider-session failure that prevented fallback lyrics after LRCLib failed.
- Visual proof: all four steps fit at 940 by 700 and 760 by 620 without page overflow. Finish saved version one and Locked mode, reopen returned to Place it in Edit mode with active controls, and Reset Settings kept completion. Native lyrics rendered through NetEase, and the approved hummingbird appeared in Setup, the overlay, and the Windows title bar.
- Acceptance criteria: HUM-10-AC4 is complete for the local first-run workflow
- Known deferrals: the four-scale Windows matrix remains in HUM-10H

### HUM-10E, signed updates and update states

- Status: Complete
- Version: 0.13.74
- Commit: closeout commit titled `Close signed Windows updates`
- Remote: push to `origin/main` during closeout
- Validation: frozen frontend install, 18 JavaScript tests, frontend typecheck and production build, 247 passing Rust tests with one live provider test intentionally ignored, debug and release all-target checks, debug and release Clippy with warnings denied, full Rust formatting, diff validation, exact version consistency, and long-dash validation
- Review: independent reviews approved the signed command boundary, release-target isolation, canonical SignTool checks, installed-payload verification, current Tauri `.exe` plus `.exe.sig` updater contract, generated feed, proof hashes, and guarded publication path
- Signed proof: workflow run `32315736382` passed on commit `8c503d29a17b9a05f38399329359e31d94d65f1e`. Windows SignTool verified both customer targets, and private artifact `9388432007` contained exactly the installer, `.exe.sig`, `latest.json`, and `release-proof.json`. The downloaded installer SHA-256 was `a6d58d4ddd5d389fae2b62abb417003db6fe1268b13c9a67df51f9bd1f328fec` and passed a second local SignTool audit with its extracted `hum.exe`.
- Acceptance criteria: HUM-10-AC6 is complete. HUM-10-AC5 remains open until HUM-10H installs an update from the previous signed version. HUM-10-AC7 was deferred by this slice and completed in HUM-10F.
- Known deferrals: public tag publishing, prior-version update and relaunch, feed withdrawal, and manual rollback remain in HUM-10H

### HUM-10F, trust and support surfaces

- Status: Complete
- Version: 0.13.75
- Commit: closeout commit titled `Add Hum support and diagnostics`
- Remote: push to `origin/main` during closeout
- Validation: 24 frontend tests passed; frontend typecheck and production build passed; 257 Rust tests passed in both debug and release profiles with one live provider test intentionally ignored; debug and release all-target checks and Clippy with warnings denied passed; full Rust formatting and diff validation passed
- Review: independent review found and closed inaccurate diagnostic disclosure copy, omitted aspect-lock and update-banner pointer capabilities, and a Vite versus Rust debug-build mismatch. The final re-review approved the corrected privacy contract and single Rust-owned build signal.
- Native proof: the debug tray retained its developer-console action, the optimized release tray omitted it, Settings exposed every About and support action through UI Automation, and a real exported diagnostic contained only the documented keys without license secrets, personal media, URLs, or user paths
- Acceptance criteria: HUM-10-AC7 and HUM-10-AC8 are complete
- Known deferrals: historical log bundles, broad opener hardening, website-domain launch proof, and a full Settings navigation redesign remain outside this slice

## Execution log

- 2026-08-19 12:05 MDT: HUM-10 started. The purchase and license policy was locked, ADR-0002 was accepted, and HUM-10A entered implementation.
- 2026-08-19 12:32 MDT: HUM-10A passed 202 Rust tests and the full gate after a red-first countdown correction. The slice closed in v0.13.62 and the cursor advanced to HUM-10B survey.
- 2026-08-19 12:44 MDT: HUM-10B locked an eleven-file production boundary for the versioned secret record, storage and provider interfaces, Polar client, license service, Windows DPAPI adapter, startup ownership, and existing Windows API feature additions.
- 2026-08-19 13:00 MDT: HUM-10B passed 229 Rust tests and the complete debug and release gate after four red-first review repairs. The slice closed in v0.13.63 and the cursor advanced to HUM-10C survey.
- 2026-08-19 13:12 MDT: HUM-10C locked a nine-file production boundary for safe license commands, native window gating, tray recovery, strict Polar links, and a dedicated activation experience.
- 2026-08-19 13:33 MDT: HUM-10C passed 234 Rust tests, the complete debug and release gate, and native release-window QA after four review and visual repairs. The slice closed in v0.13.64 and the cursor advanced to HUM-10D survey.
- 2026-08-19 13:47 MDT: HUM-10D locked a ten-file production boundary for versioned completion state, one shared customer-window plan, a predeclared Setup window, live audio and appearance choices, core-control guidance, tray recovery, and reset-safe persistence.
- 2026-08-19 14:32 MDT: HUM-10D passed 244 Rust tests, the complete debug and release gate, a live two-song NetEase smoke test, and native setup, lyrics, overlay-brand, and title-bar QA. The slice repaired provider fallback lyrics and replaced the remaining waveform assets with the approved hummingbird before v0.13.65 shipped. The cursor advanced to HUM-10E survey.
- 2026-08-19 14:48 MDT: HUM-10E locked a twelve-file production boundary for a Hum-specific updater key, Azure Trusted Signing inside the Tauri build, private signed proof runs, tag-only GitHub publishing, checked release metadata, and complete customer-visible update states.
- 2026-08-19 14:45 MDT: A real startup exposed the overlay's one-shot settings hydration race. The failure left Ribbon defaults rendered inside saved Square geometry, shrinking every lyric. A red-first retry test and native 620 by 620 replay closed the v0.13.66 correction before HUM-10E implementation resumed.
- 2026-08-19 15:07 MDT: HUM-10E completed its red-first local implementation. Hum now owns a protected updater key, GitHub holds only encrypted release secrets, the committed config contains only the public key, update states and tray projections are tested, and the manual workflow is ready for its first private signed proof run.
- 2026-08-19 15:39 MDT: The first v0.13.67 private proof passed the complete test gate, Azure setup, and updater key match, then failed when a copied `signtool.exe` could not load from outside its Windows SDK directory. A red-first workflow contract now requires Tauri's structured command form with the original SDK path. The v0.13.68 repair candidate is entering the full gate.
- 2026-08-19 15:48 MDT: Independent review found that a manual workflow started from a tag could satisfy the original publish condition. Red-first coverage now requires a pushed tag event, and the workflow's generated signing JSON is tested with spaced SDK, Azure library, and metadata paths plus Tauri's `%1` placeholder.
- 2026-08-19 16:06 MDT: The v0.13.68 proof successfully signed its first target with Azure, then exposed the developer-only UI inspector inside Tauri's release binary scan. A red-first Cargo metadata test now requires Hum as the only binary and keeps `dump_uia` available as an explicit example. The v0.13.69 repair is entering the complete local gate.
- 2026-08-19 16:37 MDT: The v0.13.69 proof passed the complete gate and signed both production targets, then the separate Authenticode step failed to parse `$target:` in its error message. A red-first workflow contract now requires the explicit `${target}:` boundary. The v0.13.70 repair is entering the complete local gate.
- 2026-08-19 17:02 MDT: The v0.13.70 proof passed the complete gate and Azure reported successful signatures for Hum and its installer, but PowerShell returned `NotSigned` for `hum.exe`. A red-first workflow contract now requires the Windows SDK SignTool to verify both files with Authenticode policy and rejects every nonzero exit code. The v0.13.71 repair is entering the complete local gate.
- 2026-08-19 17:28 MDT: The v0.13.71 proof passed the complete gate and signing, then SignTool confirmed that Tauri's restored development executable is intentionally unsigned. Tauri had already packaged the signed executable into NSIS before restoring the original build output. A red-first workflow contract now extracts the completed installer, requires exactly one installed `hum.exe`, and verifies that customer payload plus the installer. The v0.13.72 repair is entering the complete local gate.
- 2026-08-19 17:59 MDT: The v0.13.72 proof passed the full gate, Azure signing, installer extraction, and strict SignTool verification for both the installed `hum.exe` and the completed installer. Metadata preparation then found that current Tauri pairs the installer with `.exe.sig` instead of building a separate `.nsis.zip`. A red-first release-contract repair now uses the installer as the Windows updater payload. The v0.13.73 repair is entering the complete local gate.
- 2026-08-19 18:24 MDT: The exact v0.13.73 proof passed every gate, both Authenticode checks, metadata generation, and private artifact upload. The downloaded four-file artifact matched its feed, updater signature, and hashes, and a second local SignTool audit verified the installer plus extracted application. HUM-10E is closing in v0.13.74, and the cursor advances to HUM-10F survey.
- 2026-08-19 18:40 MDT: HUM-10F locked an eight-file production boundary for privacy-safe diagnostic export, fixed support and privacy destinations, a customer-facing About and support section, and build-aware removal of developer-console entry points.
- 2026-08-19 19:23 MDT: HUM-10F passed the complete debug and release gate, independent review, native debug and optimized-release tray checks, and a real diagnostic privacy audit. The slice closes in v0.13.75, and the cursor advances to HUM-10G survey.
- 2026-08-19 19:45 MDT: HUM-10G locked a two-repository paid-launch plan. Polar's built-in license-key, file-download, checkout-link, refund, and customer-portal features support the basic Hum flow without a webhook. Local work starts with paid-safe promo defaults, then a validated checkout and final public purchase policy.
- 2026-08-19 20:14 MDT: The first cross-repository review found an official Polar sandbox URL mismatch, an unvalidated canonical origin, a staging-only robots sitemap, and one missing promo regression. The approved boundary expands to twenty-nine files for red-first repairs without adding a dependency or product mechanism.
- 2026-08-20 01:37 MDT: The review repairs passed the complete Hum debug and release gates plus the full website gate. Website v0.1.2 shipped at commit `e52b4a4`, and the Vercel production deployment passed live route, console, canonical, robots, sitemap, noindex, disconnected-checkout, and schema checks. Desktop v0.13.76 is entering closeout before Polar sandbox configuration.
- 2026-08-20 01:49 MDT: Desktop v0.13.76 passed the restarted closeout gate after registering the promo-policy tests in the standard frontend command. Twenty-eight frontend tests, 262 debug Rust tests, 262 optimized Rust tests, both Clippy profiles, formatting, builds, and scope audits passed. HUM-10-AC9 is complete, and the cursor advances to Polar sandbox configuration.
- 2026-08-20 10:03 MDT: The isolated Polar sandbox now has a BaseZero organization in USD, a $19 one-time Hum product, a visible `HUM` license-key benefit limited to three activations with customer self-deactivation, the exact sandbox checkout link with success and return URLs, and a Hum-only 100% test code. The audited signed v0.13.73 installer matches SHA-256 `a6d58d4ddd5d389fae2b62abb417003db6fe1268b13c9a67df51f9bd1f328fec`, but Chrome blocked the local file handoff before Polar received it. The cursor remains on the installer benefit until extension file access is enabled.
- 2026-08-20 11:59 MDT: The isolated Polar sandbox journey passed from checkout through delivery. A zero-dollar discounted order produced the paid order, `HUM` license grant, signed installer grant, customer email code, and authenticated portal view with the receipt, license key, download, and activation controls. Production Polar now has the $19 Hum product, three-activation license benefit, signed v0.13.73 installer benefit, hosted checkout, and BaseZero customer portal. The three public Polar values are configured in GitHub Actions, the checkout and site origin are configured in Vercel, and the live Buy page reaches the production checkout. Polar still blocks real payments until Wes completes Stripe photo-ID verification, connects a payout account, and submits the account review. The public product description, website, and `info@syvr.dev` support contact are saved. The cursor is the private Polar identity and payout handoff, followed by account-review submission and the first real-payment proof.
