import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  DEFAULT_AD_BREAK_PROMOS_ENABLED,
  PROMO_CARD_DESCRIPTION,
  PROMO_CARD_LABEL,
} from "./promo-policy.ts";

test("paid promo fallback starts off", () => {
  assert.equal(DEFAULT_AD_BREAK_PROMOS_ENABLED, false);
});

test("paid promo copy describes an optional off-by-default choice", () => {
  assert.equal(PROMO_CARD_LABEL, "Show optional Hum offers during ad breaks");
  assert.match(PROMO_CARD_DESCRIPTION, /off by default/i);
  assert.match(PROMO_CARD_DESCRIPTION, /plain "Ad break" label/i);
  assert.doesNotMatch(PROMO_CARD_DESCRIPTION, /SYVR promo cards/i);
});

test("Overlay and Settings consume the shared paid promo policy", async () => {
  const [overlay, settings] = await Promise.all([
    readFile(new URL("./Overlay.tsx", import.meta.url), "utf8"),
    readFile(new URL("./Settings.tsx", import.meta.url), "utf8"),
  ]);

  assert.match(
    overlay,
    /ad_break_promos_enabled:\s*DEFAULT_AD_BREAK_PROMOS_ENABLED/,
  );
  assert.match(settings, /label=\{PROMO_CARD_LABEL\}/);
  assert.match(settings, /<Hint>\{PROMO_CARD_DESCRIPTION\}<\/Hint>/);
});
