# Hum launch checklist

Last reviewed: 2026-08-25
Shipped so far: v0.13.95 and v0.13.96 (section 1, everything except the
primary-surface decision and the proof items, which need a real release)

This is the work list for getting Hum sellable. It is deliberately separate from
[the 1.0 release checklist](verification/1.0-release-checklist.md), which is the
pass or stop gate you run against a finished build. This file is what still has
to be built, wired, or bought first.

## The headline

Every piece of release machinery exists and none of it has ever run.
`basezero-projects/Hum` is public, all eight signing secrets are set, all three
Polar variables point at a real production checkout, and the workflow does
Authenticode signing through Azure Trusted Signing plus minisign updater
signing with a key it verifies against the shipped pubkey.

`gh release list` returns nothing. There has never been a tagged release, so:

- The updater endpoint `releases/latest/download/latest.json` currently 404s.
- No customer has ever downloaded anything.
- HUM-10-AC5 ("an update is tested from the previous release") cannot pass
  until at least two releases exist.

The first job is not writing more code. It is cutting v0.14.0 and watching the
pipeline work.

## Decision 0: what does "released" mean

The roadmap defines 1.0 as HUM-00 through HUM-70. Five of those phases
(Lyrics Control Center, automatic audio profiles, premium presentation,
language and offline, Creator Studio) are still Proposed, with no contract
locked. That is months of work.

Nothing in this file assumes you wait for it. Hum already resolves lyrics at
92 percent on a measured real-world run, has a license system, onboarding, a
settings surface, an OBS server, and a signed installer. That is a sellable
product today at a price that sets expectations honestly.

Pick one before working through the rest:

- **Ship a paid 0.14.x now** and treat the roadmap phases as the paid updates
  buyers were promised. Revise the website copy to describe what ships today.
- **Hold for full 1.0** and work the roadmap in order. Then this file is just
  the launch machinery half and the roadmap is the other half.

The rest of this checklist is required either way.

## 1. Auto-update, the SimSweep model adapted for Hum

Hum's update system is more complete than it looks. There is a real state
machine (`src/update-state.ts`), a tray item whose label changes to
"Install update vX", a banner in every overlay layout, error stages with
retry, and native signature enforcement. What it does not do is behave the way
SimSweep does.

### The actual gap

SimSweep pre-downloads the update silently and keeps the banner hidden until
the bytes are on disk (`useUpdater.ts`, the `ready` flag). Hum flips straight
to `available` the moment `check()` returns and only starts downloading when
the user clicks. So the banner reads "Hum v0.14.1 is ready to install" while
nothing has been downloaded, and the click the user thinks is instant is
actually the whole download.

- [x] Pre-download after a successful check. Keep the state internal until the
      bytes land, then flip to `available`. This makes the existing copy true
      instead of rewriting it.
- [x] Gate ready on an actual `Finished` event, not on `download()` resolving.
      SimSweep added `finishedSeen` for exactly this: a promise that resolves
      without `Finished` would otherwise hand `install()` a partial file.
- [x] Keep the download quiet. No banner, no tray change, no progress until it
      is done and there is something worth clicking.

### Where Hum has to differ from SimSweep

SimSweep gets launched, used, and closed. Hum is an always-on overlay that can
run for weeks, and it sits on top of whatever the person is doing.

- [x] Add a periodic re-check. Right now `runUpdateCheck("automatic")` fires
      once when the overlay mounts and never again, so somebody who leaves Hum
      running never learns an update exists. Every six hours is reasonable.
- [x] Re-check after the machine wakes from sleep. A laptop that slept through
      a release should catch up when it opens.
- [x] Do not restart during playback without asking. Shipped in v0.13.96 as a
      confirm step rather than the deferred install this originally proposed.
      `tauri-plugin-updater` calls `std::process::exit(0)` inside `install()`
      on Windows (`updater.rs:865`), so "install when I close Hum" cannot be
      built on this plugin. A click during playback now names the consequence
      and waits for a second click, reverting after ten seconds.
- [ ] Decide whether the overlay banner or the tray item is the primary
      surface. The tray already carries the right label and costs the user no
      screen space over their music. The banner may be better as an opt-in.
- [x] Make the banner dismissible for the session. An always-on-top window is
      the worst place for a notice that cannot be closed.

### Error copy

Hum currently renders "Could not download Hum v0.14.1. Try again." for
everything. SimSweep maps the real failures to something actionable in
`friendlyUpdateError()`.

- [x] Map access denied and "os error 13" to the install-folder explanation.
- [x] Map signature, verify, and minisign failures to "downloaded but could
      not be verified, so it was not installed", plus the manual download link.
- [x] Map network, DNS, and timeout to a connection message.
- [x] Keep the raw error available for support without showing it by default.

### Proving it works

- [ ] Publish v0.14.0, then publish v0.14.1, then confirm a real machine
      running 0.14.0 finds it, downloads it silently, and installs on one
      click. This is HUM-10-AC5 and it cannot be faked.
- [ ] Confirm the NSIS `passive` install mode does not throw a UAC prompt for
      a per-user install.
- [ ] Test the update path with Hum running as an autostart app.
- [ ] Test a withdrawn release. Delete the newer release and confirm clients
      settle instead of erroring in a loop.

## 2. Distribution

- [ ] Tag and publish v0.14.0 through the existing workflow. Everything is
      already wired, this is a `git tag` away.
- [ ] Verify `latest.json` is served at the configured endpoint and its
      signature validates.
- [ ] Download the published installer on a machine that has never had Hum,
      install it, and confirm SmartScreen stays quiet. Azure Trusted Signing
      should handle this but reputation builds over downloads, so check.
- [ ] Confirm Defender scans the installer and installed files clean.
- [ ] Keep the previous signed installer available for rollback before
      publishing anything newer.

## 3. Commerce

The Polar side is further along than the app side. The production checkout link
is set as a repo variable and the site validates it against `buy.polar.sh`.

- [ ] Confirm the Polar product is live rather than sandbox, priced at $19
      one-time, and issues license keys.
- [ ] Confirm the license key format Polar issues is what
      `license/polar.rs` expects. Run a real key end to end.
- [ ] Buy Hum with a real card. Activate it on a clean machine. Then refund
      it and confirm the license revokes and the app explains what happened.
- [ ] Activate on three machines, confirm the fourth is refused with a message
      that points at the customer portal.
- [ ] Free an activation from the Polar portal and confirm the fourth machine
      then works.
- [ ] Confirm the 30-day offline revalidation warns before the grace period
      ends rather than after.
- [ ] Confirm refund policy, device count, and "includes 1.x updates" match
      between Polar, the website, and the in-app copy.

## 4. Website

The site is built and good. Eleven pages including privacy, terms, support,
and four SEO landing pages. It is not connected to anything.

- [ ] Register a domain. The app already hardcodes
      `https://humlyrics.com/privacy` in `trust.rs`, so either register that or
      change the app. Do not ship a build pointing at a domain you do not own.
- [ ] Point the Vercel project at the domain and set `PUBLIC_SITE_URL`. It is
      currently `hum-site.vercel.app`.
- [ ] Set `PUBLIC_CHECKOUT_URL` in Vercel to the production Polar link. It is
      empty locally, so checkout is disconnected.
- [ ] Point the download page at the signed release artifact.
- [ ] Serve `https://syvrstudios.com/hum/promos.json`. It has 404d on every
      startup for the whole session. Either serve it or disable the promo
      fetch for launch. A 404 on every boot is not acceptable in a paid app.
- [ ] Rewrite the privacy page's service list. It currently names Apple,
      Polar, and syvr.dev. The app actually talks to: LRCLib, the
      `lyrics.syvr.dev` proxy, NetEase (`music.163.com`), iTunes, Deezer,
      TheAudioDB, Wikipedia, Ticketmaster, GitHub releases, syvrstudios.com,
      and Polar. NetEase in particular is a Chinese service and a buyer
      deserves to know a lyric lookup can reach it.
- [ ] Disclose that auto-contrast samples pixels behind the overlay, how often,
      and how to turn it off. This is a screen-reading behavior and burying it
      would be indefensible.
- [ ] Trim any claim the shipped build does not support. If translation,
      romanization, or Creator Studio are not in the release, they cannot be on
      the site.
- [ ] Make support contact and expected response time visible before purchase.
- [ ] Confirm the LLC name on terms matches SYVR Studios LLC exactly.

## 5. App bugs that block a paid launch

From `BUGS.md`, in the order they matter:

- [ ] `promos.json` 404 on every startup. Listed above under website but it is
      an app-visible failure.
- [ ] Double resolution. One track logs two rows and makes two round trips
      because the raw and bridge-normalized titles hash differently. Roughly 15
      percent wasted provider calls, and it is rude to LRCLib at scale.
- [ ] Truncated normalized title (`Where Is The Love (BCX`). A malformed query
      is a false-match risk, which is the failure mode that matters most.
- [ ] The remix decision. Every miss and every timing downgrade in the
      51-track run was a remix. Decide whether plain untimed words on a remix
      beat showing nothing.
- [ ] Move promo link opening behind a Rust allowlist. `Overlay.tsx` still
      opens remote promotional URLs through `opener:default`.
- [ ] Make "Reset all settings" actually reapply the backdrop, stop the OBS
      server, and clear autostart instead of only persisting defaults.

## 6. Trust and support

- [ ] Confirm the release build genuinely requires a license. The dev
      entitlement is gated on `#[cfg(any(debug_assertions, not(windows)))]`,
      which looks right, but verify against a real signed release build.
- [ ] Confirm the dev console does not ship. `get_build_info().developer_console`
      is tied to `debug_assertions` and there is a test, so verify on the
      artifact rather than trusting the test.
- [ ] Confirm no license key, activation id, or token appears in the settings
      JSON, logs, or a diagnostics export.
- [ ] Set up the support inbox and actually watch it. `info@syvr.dev` is the
      destination baked into the app.
- [ ] Write the refund and rollback response before you need it.

## 7. Before you charge anyone

A short list of things that would be embarrassing rather than merely broken.

- [ ] Install the signed artifact on a Windows machine that has never had a Rust
      toolchain, Node, or Hum on it. Use a standard user account, not admin.
- [ ] Play a song. Confirm lyrics appear without touching a setting.
- [ ] Confirm the overlay does not open offscreen on a single-monitor 1080p
      machine at 125 percent scaling, which is the most common Windows setup
      there is.
- [ ] Leave it running for four hours with real playback and confirm it does
      not leak or freeze.
- [ ] Uninstall it. Confirm it leaves nothing behind that would break a
      reinstall.
