import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { resolveWindowSurface } from "./window-route.ts";

test("Rust release profile never selects the developer console", async () => {
  const loadReleaseProfile = async () => ({ developer_console: false });
  assert.equal(await resolveWindowSurface("main", loadReleaseProfile), "settings");
  assert.equal(await resolveWindowSurface("unknown-window", loadReleaseProfile), "settings");
  assert.equal(await resolveWindowSurface(null, loadReleaseProfile), "settings");
});

test("Rust debug profile retains the intended developer surface", async () => {
  const loadDebugProfile = async () => ({ developer_console: true });
  assert.equal(await resolveWindowSurface("main", loadDebugProfile), "dev_console");
  assert.equal(await resolveWindowSurface("unknown-window", loadDebugProfile), "dev_console");
  assert.equal(await resolveWindowSurface(null, loadDebugProfile), "dev_console");
});

test("build profile command failures fail closed to settings", async () => {
  const rejectedProfile = async (): Promise<{ developer_console: boolean }> => {
    throw new Error("native command unavailable");
  };
  assert.equal(await resolveWindowSurface("main", rejectedProfile), "settings");
  assert.equal(await resolveWindowSurface(null, rejectedProfile), "settings");
});

test("malformed build profile responses fail closed to settings", async () => {
  const malformedProfile = async () => ({ developer_console: "yes" as unknown as boolean });
  assert.equal(await resolveWindowSurface("main", malformedProfile), "settings");
});

test("customer window labels route without loading the build profile", async () => {
  let loads = 0;
  const loadProfile = async () => {
    loads += 1;
    return { developer_console: true };
  };
  for (const surface of ["overlay", "settings", "activation", "setup"] as const) {
    assert.equal(await resolveWindowSurface(surface, loadProfile), surface);
  }
  assert.equal(loads, 0);
});

test("diagnostic disclosure names its limited license summary and exact exclusions", () => {
  const settingsSource = readFileSync(new URL("./Settings.tsx", import.meta.url), "utf8");
  const normalizedSource = settingsSource.replace(/\s+/g, " ");

  assert.match(normalizedSource, /Includes limited license status and device details\./);
  assert.match(
    normalizedSource,
    /It leaves out license keys, activation details, verification timestamps, song information, lyrics, file paths, and cache contents\./,
  );
  assert.doesNotMatch(normalizedSource, /leaves out license details/i);
});
