# HUM-10F trust and support plan

Date: 2026-08-19
Status: Complete
Final version: 0.13.75
Related phase: [HUM-10](../../roadmap/1.0/HUM-10-purchase-trust-first-run.md)
Progress ledger: [HUM-10 ledger](../../roadmap/1.0/HUM-10-progress-ledger.md)

## Outcome

A customer can confirm which Hum build they are using, manage their license, check for updates, contact support, read the privacy policy, and export a useful diagnostic snapshot from Settings. Release builds no longer expose the developer console through the tray or an accidental window route.

## Product contract

- Settings gains one polished `About & support` section. It shows the Hum name and version, operating system and architecture, a concise license summary, and actions for license management, updates, support, privacy, and diagnostic export.
- Support opens `mailto:info@syvr.dev?subject=Hum%20support`.
- Privacy opens `https://humlyrics.com/privacy`.
- Rust owns both destinations through a closed enum. The frontend cannot pass an arbitrary URL.
- A successful diagnostic export shows the exact saved file path. A failure stays visible beside the action in plain language.
- Update checks continue through the signed production updater from HUM-10E. This slice adds no simulation or alternate update path.
- Debug builds retain the developer console. Release tray construction and release window routing cannot reveal it.

## Privacy-safe diagnostics contract

The export is a versioned JSON snapshot written to the resolved Downloads folder with create-new semantics. It contains only information needed to reproduce product behavior:

- Hum version, diagnostic schema version, operating system, architecture, and generation time
- Platform capability booleans without application paths
- An explicit allowlist of layout, timing, source, startup, appearance, accessibility, and OBS settings
- License status, licensed state, device limit, and days until action
- Cache existence, item count, and byte totals without cache contents or filenames

The export must never contain:

- A license key, masked key, activation ID, verification timestamps, Polar configuration, or protected license bytes
- Absolute application paths, the Windows user name, environment variables, or machine identifiers
- Current track, artist, album, artwork URL, lyrics, listening history, or source media metadata
- Cache payloads, cache filenames, raw console output, or raw provider errors

The implementation uses a dedicated diagnostic DTO. It must not serialize `Settings`, `PlatformInfo`, or the public license DTO wholesale, because later fields could silently widen the privacy boundary.

## Production file boundary

- Add `src-tauri/src/trust.rs`.
- Modify `src-tauri/src/lib.rs`.
- Modify `src/Settings.tsx`.
- Modify `src/types.ts`.
- Modify `src/main.tsx`.
- Add `src/window-route.ts`.
- Add `src/window-route.test.mts`.
- Modify `package.json` to register the route contract test.

Standard closeout files are also expected: version manifests, `docs/CHANGELOG.md`, this plan, the HUM-10 roadmap and ledger, `BUGS.md` when needed, plus Hum brain records.

No dependency, Tauri capability, updater, license-storage, overlay, website, or DevConsole change is planned.

## Implementation order

1. Add failing Rust tests for the diagnostic allowlist, forbidden markers, fixed destinations, About metadata, safe export filenames, create-new behavior, and release tray planning.
2. Add failing frontend tests proving production routing never selects DevConsole while development routing still can.
3. Implement the Rust trust module and commands for product metadata, fixed destinations, update requests, and diagnostic export.
4. Make tray planning build-aware, and remove the release handler path for the developer console.
5. Extract the frontend route decision and make production fallbacks customer-safe.
6. Add the `About & support` Settings section with accessible pending, success, and failure states.
7. Audit the complete source tree for demo updater paths, arbitrary trust URLs, protected diagnostic fields, and release DevConsole reachability.

## Required red-first tests

- Diagnostic JSON contains every allowlisted reproduction field with stable snake-case names.
- Hostile marker values placed in excluded license, path, media, and cache fields never appear in serialized diagnostics.
- Platform paths are absent even when the safe capability flags are present.
- Support and privacy map to the exact fixed destinations without accepting caller-provided URLs.
- About metadata uses the running package name and version.
- Export chooses a safe `Hum-diagnostics-*.json` filename, creates a new file, never overwrites an existing file, and returns path or write failures.
- Release tray planning excludes `toggle-console`, while debug tray planning retains it.
- Production window routing never selects DevConsole for `main`, an unknown label, or browser fallback. Development routing retains the intended developer surface.
- Existing updater-state tests remain green, proving the only update path is still the signed production flow.

## Acceptance criteria

- HUM-10-AC7 is complete: release builds hide the developer console and contain no demo-only updater path.
- HUM-10-AC8 is complete: About, support, privacy, and diagnostics are reachable from Settings.
- The diagnostic artifact passes an explicit secret and personal-media audit.
- Support and privacy use fixed Rust-owned destinations.
- Settings actions expose visible, accessible success and failure feedback.
- Debug and release tests, checks, Clippy, formatting, frontend typecheck, and production build all pass.

## Known deferrals

- A historical log bundle needs a persistent logging system and broader instrumentation. HUM-10F exports a current diagnostic snapshot only.
- The production window declaration may still include the hidden developer window, but release tray and routing code make it unreachable. Profile-specific Tauri configuration is not required for HUM-10-AC7.
- Broad `opener:default` removal for promotional links belongs to a separate security hardening slice.
- Registering and launch-testing `humlyrics.com` belongs to the purchase-site work in HUM-10G.
- A full Settings navigation redesign is outside this focused trust slice.

## Completion proof

- Twenty-four frontend contract tests passed with production routing, failed build-info resolution, and customer-window bypass covered.
- Two hundred fifty-seven Rust tests passed in both debug and release profiles, with one live provider smoke test intentionally ignored in each profile.
- Debug and release all-target checks and Clippy with warnings denied passed, along with frontend typecheck, production build, Rust formatting, and diff validation.
- Independent review found and closed three gaps: inaccurate privacy copy, two omitted window capability flags, and conflicting frontend and Rust definitions of a development build.
- Native Windows proof confirmed that the debug tray retains DevConsole, the optimized release tray omits it, Settings exposes every trust action, and a real diagnostic export contains only the documented key sets without protected license material, personal media, URLs, or user paths.

## Amendment gate

Stop for approval if the slice needs a persistent logging subsystem, arbitrary external URLs, a new dependency, profile-specific Tauri configuration, a Settings information architecture rewrite, more than sixteen production files, or any change to license, update, or purchase policy.

## Closeout

Before HUM-10G starts: complete the red-first tests, full debug and release gate, independent cold review, diagnostic privacy audit, native Settings and tray proof, version bump, changelog, BUGS review, ledger and brain writes, commit, push, and exact-commit verification.
