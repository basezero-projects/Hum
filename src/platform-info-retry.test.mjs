import assert from "node:assert/strict";
import test from "node:test";

import { loadPlatformInfoWithRetry } from "./types.ts";

const PLATFORM_INFO = {
  platform: "windows",
  media: { playback: true },
  audio_output: {
    discovery: false,
    active_output_changes: false,
  },
  window: {
    supported_backdrops: ["acrylic", "mica", "tabbed_mica", "none"],
    aspect_lock: true,
    click_through: true,
    update_banner_pointer_exception: true,
    screen_sampling: true,
  },
  services: {
    tray: true,
    global_shortcuts: true,
    autostart: true,
    updater: true,
  },
  paths: {
    app_data_dir: "C:/Hum",
    settings_file: "C:/Hum/settings.json",
  },
};

test("platform information recovers after an initial rejected load", async () => {
  let attempts = 0;
  const waited = [];

  const result = await loadPlatformInfoWithRetry(
    async () => {
      attempts += 1;
      if (attempts === 1) throw new Error("temporary command failure");
      return PLATFORM_INFO;
    },
    async (delayMs) => {
      waited.push(delayMs);
    },
    () => true,
  );

  assert.equal(result, PLATFORM_INFO);
  assert.equal(attempts, 2);
  assert.deepEqual(waited, [1000]);
});
