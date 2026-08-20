import assert from "node:assert/strict";
import test from "node:test";

import { hydrateSubscribedState } from "./live-state-hydration.ts";

test("state hydration waits until subscriptions are ready", async () => {
  let releaseSubscriptions!: () => void;
  const subscriptionsReady = new Promise<void>((resolve) => {
    releaseSubscriptions = resolve;
  });
  let reads = 0;
  const applied: string[] = [];

  const hydration = hydrateSubscribedState({
    subscriptionsReady,
    readSnapshot: async () => {
      reads += 1;
      return "synced lyrics";
    },
    currentRevision: () => 0,
    applySnapshot: (value) => applied.push(value),
    isActive: () => true,
  });

  await Promise.resolve();
  assert.equal(reads, 0);
  releaseSubscriptions();
  await hydration;

  assert.equal(reads, 1);
  assert.deepEqual(applied, ["synced lyrics"]);
});

test("a newer event prevents an older snapshot from overwriting it", async () => {
  let revision = 0;
  let finishRead!: (value: string) => void;
  const applied: string[] = [];

  const hydration = hydrateSubscribedState({
    subscriptionsReady: Promise.resolve(),
    readSnapshot: () =>
      new Promise<string>((resolve) => {
        finishRead = resolve;
      }),
    currentRevision: () => revision,
    applySnapshot: (value) => applied.push(value),
    isActive: () => true,
  });

  await Promise.resolve();
  revision += 1;
  applied.push("new event");
  finishRead("old no lyrics snapshot");
  await hydration;

  assert.deepEqual(applied, ["new event"]);
});

test("an unmounted overlay ignores a late snapshot", async () => {
  let active = true;
  const applied: string[] = [];

  const hydration = hydrateSubscribedState({
    subscriptionsReady: Promise.resolve(),
    readSnapshot: async () => "late lyrics",
    currentRevision: () => 0,
    applySnapshot: (value) => applied.push(value),
    isActive: () => active,
  });
  active = false;
  await hydration;

  assert.deepEqual(applied, []);
});
