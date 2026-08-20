import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  SETTINGS_PAGES,
  availableSettingsPages,
  normalizeSettingsPage,
} from "./settings-navigation.ts";

test("settings categories are concise, ordered, and unique", () => {
  assert.deepEqual(
    SETTINGS_PAGES.map(({ id }) => id),
    [
      "general",
      "timing",
      "text",
      "background",
      "layout",
      "features",
      "shortcuts",
      "streaming",
      "about",
    ],
  );
  assert.equal(new Set(SETTINGS_PAGES.map(({ label }) => label)).size, SETTINGS_PAGES.length);
});

test("unavailable shortcut settings disappear without breaking the current page", () => {
  assert.equal(normalizeSettingsPage("shortcuts", false), "general");
  assert.equal(normalizeSettingsPage("timing", false), "timing");
  assert.equal(normalizeSettingsPage("unknown", true), "general");
  assert.equal(
    availableSettingsPages(false).some(({ id }) => id === "shortcuts"),
    false,
  );
});

test("Settings renders a categorized tab workspace instead of one long document", () => {
  const source = readFileSync(new URL("./Settings.tsx", import.meta.url), "utf8");
  const config = JSON.parse(
    readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
  );
  const settingsWindow = config.app.windows.find(({ label }: { label: string }) => label === "settings");

  assert.match(source, /role="tablist"/);
  assert.match(source, /aria-selected=/);
  assert.match(source, /activePage === "general"/);
  assert.match(source, /className="settings-nav"/);
  assert.equal(settingsWindow.width, 920);
  assert.equal(settingsWindow.minWidth, 720);
});
