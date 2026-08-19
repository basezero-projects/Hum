import assert from "node:assert/strict";
import test from "node:test";

import {
  canInstallUpdate,
  clampDownloadProgress,
  nativeUpdateStatus,
  promoteUpdateOrigin,
  sanitizeUpdateVersion,
  shouldRequestManualCheck,
  updatePresentation,
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
    state: { phase: "downloading", version: "1.2.3", progress: 42 },
    banner: "Downloading Hum v1.2.3: 42%",
    tray: "Downloading update: 42%",
    action: "none",
    progress: 42,
  },
  {
    state: { phase: "downloading", version: "1.2.3", progress: null },
    banner: "Downloading Hum v1.2.3...",
    tray: "Downloading update...",
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
  assert.equal(canInstallUpdate({ phase: "downloading", version: "1.2.3", progress: null }, true), false);
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
