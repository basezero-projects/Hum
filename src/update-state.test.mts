import assert from "node:assert/strict";
import test from "node:test";

import {
  canInstallUpdate,
  clampDownloadProgress,
  friendlyUpdateError,
  nativeUpdateStatus,
  promoteUpdateOrigin,
  sanitizeUpdateVersion,
  shouldRequestManualCheck,
  shouldRunPeriodicCheck,
  updatePresentation,
  UPDATE_CHECK_INTERVAL_MS,
  type UpdateState,
} from "./update-state.ts";

const CASES: Array<{
  state: UpdateState;
  banner: string | null;
  tray: string;
  action: "none" | "install" | "retry";
  progress: number | null;
}> = [
  {
    state: { phase: "idle" },
    banner: null,
    tray: "Check for updates",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "checking", origin: "manual" },
    banner: "Checking for updates...",
    tray: "Checking for updates...",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "checking", origin: "automatic" },
    banner: null,
    tray: "Check for updates",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "current", origin: "manual" },
    banner: "Hum is up to date",
    tray: "Hum is up to date",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "current", origin: "automatic" },
    banner: null,
    tray: "Check for updates",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "available", version: "1.2.3" },
    banner: "Hum v1.2.3 is ready to install",
    tray: "Install update v1.2.3",
    action: "install",
    progress: null,
  },
  {
    state: {
      phase: "downloading",
      version: "1.2.3",
      progress: 42,
      origin: "manual",
    },
    banner: "Downloading Hum v1.2.3: 42%",
    tray: "Downloading update: 42%",
    action: "none",
    progress: 42,
  },
  {
    state: {
      phase: "downloading",
      version: "1.2.3",
      progress: null,
      origin: "manual",
    },
    banner: "Downloading Hum v1.2.3...",
    tray: "Downloading update...",
    action: "none",
    progress: null,
  },
  {
    state: {
      phase: "downloading",
      version: "1.2.3",
      progress: 42,
      origin: "automatic",
    },
    banner: null,
    tray: "Check for updates",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "installing", version: "1.2.3" },
    banner: "Installing Hum v1.2.3...",
    tray: "Installing update...",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "restarting", version: "1.2.3" },
    banner: "Restarting Hum...",
    tray: "Restarting Hum...",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "error", stage: "check", origin: "manual" },
    banner: "Could not check for updates. Try again.",
    tray: "Retry update check",
    action: "retry",
    progress: null,
  },
  {
    state: { phase: "error", stage: "check", origin: "automatic" },
    banner: null,
    tray: "Check for updates",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "error", stage: "download", version: "1.2.3" },
    banner: "Could not download Hum v1.2.3. Try again.",
    tray: "Retry update v1.2.3",
    action: "retry",
    progress: null,
  },
  {
    state: {
      phase: "error",
      stage: "download",
      version: "1.2.3",
      origin: "automatic",
    },
    banner: null,
    tray: "Check for updates",
    action: "none",
    progress: null,
  },
  {
    state: { phase: "error", stage: "install", version: "1.2.3" },
    banner: "Could not install Hum v1.2.3. Try again.",
    tray: "Retry update v1.2.3",
    action: "retry",
    progress: null,
  },
  {
    state: { phase: "error", stage: "restart", version: "1.2.3" },
    banner: "Hum was updated, but could not restart. Try again.",
    tray: "Retry restart",
    action: "retry",
    progress: null,
  },
  {
    state: {
      phase: "error",
      stage: "download",
      version: "1.2.3",
      message: "Hum could not reach the update server. Check your connection and try again.",
    },
    banner: "Hum could not reach the update server. Check your connection and try again.",
    tray: "Retry update v1.2.3",
    action: "retry",
    progress: null,
  },
];

test("every update state has exact customer and tray presentation", () => {
  for (const expected of CASES) {
    const actual = updatePresentation(expected.state);
    assert.equal(actual.bannerText, expected.banner, JSON.stringify(expected.state));
    assert.equal(actual.trayText, expected.tray, JSON.stringify(expected.state));
    assert.equal(actual.action, expected.action, JSON.stringify(expected.state));
    assert.equal(actual.progress, expected.progress, JSON.stringify(expected.state));
  }
});

test("an automatic download is invisible until the bytes are on disk", () => {
  // The whole point of pre-downloading is that the user is not bothered by it.
  // A progress bar on an always-on-top overlay for work nobody asked for is
  // exactly the nag this replaces.
  for (const progress of [null, 0, 55, 100]) {
    const silent = updatePresentation({
      phase: "downloading",
      version: "9.9.9",
      progress,
      origin: "automatic",
    });
    assert.equal(silent.bannerText, null);
    assert.equal(silent.trayText, "Check for updates");
    assert.equal(silent.action, "none");
  }

  // And once it lands, the offer is real rather than a promise to start work.
  const ready = updatePresentation({ phase: "available", version: "9.9.9" });
  assert.equal(ready.bannerText, "Hum v9.9.9 is ready to install");
  assert.equal(ready.action, "install");
});

test("a manual download stays visible because the user asked for it", () => {
  const shown = updatePresentation({
    phase: "downloading",
    version: "9.9.9",
    progress: 12,
    origin: "manual",
  });
  assert.equal(shown.bannerText, "Downloading Hum v9.9.9: 12%");
  assert.equal(shown.progress, 12);
});

test("download progress clamps and stays unknown without a usable total", () => {
  assert.equal(clampDownloadProgress(50, 100), 50);
  assert.equal(clampDownloadProgress(500, 100), 100);
  assert.equal(clampDownloadProgress(-20, 100), 0);
  assert.equal(clampDownloadProgress(25, undefined), null);
  assert.equal(clampDownloadProgress(25, 0), null);
});

test("install requires both an available state and a live updater resource", () => {
  const available: UpdateState = { phase: "available", version: "1.2.3" };
  assert.equal(canInstallUpdate(available, true), true);
  assert.equal(canInstallUpdate(available, false), false);
  assert.equal(
    canInstallUpdate(
      { phase: "downloading", version: "1.2.3", progress: null, origin: "manual" },
      true,
    ),
    false,
  );
});

test("native status contains no provider errors and disables nonactions", () => {
  assert.deepEqual(nativeUpdateStatus({ phase: "available", version: "1.2.3" }), {
    phase: "available",
    version: "1.2.3",
    progress: null,
    retryable: true,
    stage: null,
  });
  assert.deepEqual(
    nativeUpdateStatus({ phase: "error", stage: "check", origin: "automatic" }),
    { phase: "idle", version: null, progress: null, retryable: false, stage: null },
  );
  assert.deepEqual(nativeUpdateStatus({ phase: "installing", version: "1.2.3" }), {
    phase: "installing",
    version: "1.2.3",
    progress: null,
    retryable: false,
    stage: null,
  });
  assert.deepEqual(
    nativeUpdateStatus({ phase: "error", stage: "restart", version: "1.2.3" }),
    {
      phase: "error",
      version: "1.2.3",
      progress: null,
      retryable: true,
      stage: "restart",
    },
  );
});

test("the tray hides every background update phase the banner hides", () => {
  // These two surfaces disagreeing is how a user ends up with a silent banner
  // and a tray item announcing a download they never started.
  const background: UpdateState[] = [
    { phase: "checking", origin: "automatic" },
    { phase: "current", origin: "automatic" },
    { phase: "downloading", version: "1.2.3", progress: 40, origin: "automatic" },
    { phase: "error", stage: "check", origin: "automatic" },
    { phase: "error", stage: "download", version: "1.2.3", origin: "automatic" },
  ];
  for (const state of background) {
    assert.equal(updatePresentation(state).bannerText, null, JSON.stringify(state));
    assert.equal(nativeUpdateStatus(state).phase, "idle", JSON.stringify(state));
    assert.equal(nativeUpdateStatus(state).retryable, false, JSON.stringify(state));
  }
});

test("unsafe update versions are rejected before rendering", () => {
  assert.equal(sanitizeUpdateVersion("1.2.3-beta.1"), "1.2.3-beta.1");
  assert.equal(sanitizeUpdateVersion("<script>alert(1)</script>"), null);
  assert.equal(sanitizeUpdateVersion("a".repeat(80)), null);
});

test("a manual request promotes an in-flight automatic check", () => {
  assert.equal(promoteUpdateOrigin("automatic", "manual"), "manual");
  assert.equal(promoteUpdateOrigin("manual", "automatic"), "manual");
  assert.equal(promoteUpdateOrigin("automatic", "automatic"), "automatic");
});

test("a tray click can promote checking but cannot overlap installation", () => {
  assert.equal(shouldRequestManualCheck("none", "idle"), true);
  assert.equal(shouldRequestManualCheck("none", "checking"), true);
  assert.equal(shouldRequestManualCheck("none", "installing"), false);
  assert.equal(shouldRequestManualCheck("install", "idle"), false);
});

test("the periodic check fires on first run and once per interval after", () => {
  const hour = 60 * 60 * 1000;

  // Never checked: run immediately on startup.
  assert.equal(shouldRunPeriodicCheck(null, 1_000_000), true);

  // Inside the window: stay quiet.
  assert.equal(shouldRunPeriodicCheck(1_000_000, 1_000_000 + hour), false);
  assert.equal(
    shouldRunPeriodicCheck(1_000_000, 1_000_000 + UPDATE_CHECK_INTERVAL_MS - 1),
    false,
  );

  // Exactly at the boundary and beyond: run.
  assert.equal(
    shouldRunPeriodicCheck(1_000_000, 1_000_000 + UPDATE_CHECK_INTERVAL_MS),
    true,
  );

  // A machine that slept eight hours checks the moment it wakes, which is why
  // this compares wall clock instead of counting timer ticks.
  assert.equal(shouldRunPeriodicCheck(1_000_000, 1_000_000 + 8 * hour), true);

  // A clock corrected backwards must not park the next check in the far
  // future, so a negative elapsed time forces a check.
  assert.equal(shouldRunPeriodicCheck(1_000_000, 500_000), true);
});

test("raw updater failures become something a person can act on", () => {
  const denied = friendlyUpdateError(new Error("failed to write: os error 13"));
  assert.match(denied, /Windows blocked Hum from replacing itself/);
  assert.match(denied, /Close any other copy of Hum/);

  const locked = friendlyUpdateError("The process cannot access the file, used by another process");
  assert.match(locked, /Windows blocked Hum from replacing itself/);

  // A signature failure must never suggest a way to install it anyway.
  const bad = friendlyUpdateError(new Error("minisign: signature verification failed"));
  assert.match(bad, /could not be verified, so Hum did not install it/);
  assert.doesNotMatch(bad, /skip|ignore|anyway|force/i);

  assert.match(
    friendlyUpdateError(new Error("no space left on device")),
    /not enough free disk space/,
  );
  assert.match(
    friendlyUpdateError(new Error("error sending request: dns error")),
    /could not reach the update server/,
  );

  // Anything unrecognized still says what happened and what to do next.
  const unknown = friendlyUpdateError({ weird: true });
  assert.match(unknown, /The update did not go through/);
  assert.equal(unknown.length > 0, true);
});

test("friendly errors never leak raw provider text to the overlay", () => {
  // The overlay sits on top of whatever somebody is doing. Dumping a Rust
  // panic or a file path into it is both useless and alarming. The raw error
  // belongs in the log, not the banner.
  const raw = "thread 'main' panicked at C:\\Users\\someone\\src\\updater.rs:42";
  const message = friendlyUpdateError(new Error(raw));
  assert.doesNotMatch(message, /panicked|\.rs:|C:\\/);
});
