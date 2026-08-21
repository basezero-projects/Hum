import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  ribbonCompositionMaxWidth,
  ribbonContentHeight,
  ribbonLineFontSize,
  ribbonAlbumArtSize,
  stableRibbonLineMetrics,
} from "./ribbon-composition.ts";

test("the lyrics-only ribbon stays inside a compact composition", () => {
  assert.equal(
    ribbonCompositionMaxWidth({ availableWidth: 1100, scale: 1 }),
    760,
  );
});

test("compact compositions remain responsive on narrow ribbons", () => {
  assert.equal(
    ribbonCompositionMaxWidth({ availableWidth: 640, scale: 1 }),
    640,
  );
  assert.equal(
    ribbonCompositionMaxWidth({ availableWidth: 540, scale: 1.2 }),
    540,
  );
});

test("invalid measurements cannot create negative layout widths", () => {
  assert.equal(
    ribbonCompositionMaxWidth({ availableWidth: -20, scale: 1 }),
    0,
  );
  assert.equal(
    ribbonCompositionMaxWidth({ availableWidth: 1100, scale: -1 }),
    0,
  );
});

test("each lyric role keeps one fixed font size", () => {
  assert.equal(ribbonLineFontSize({ baseSize: 40, role: "current" }), 40);
  assert.equal(ribbonLineFontSize({ baseSize: 40, role: "adjacent" }), 24);
  assert.equal(ribbonLineFontSize({ baseSize: 10, role: "adjacent" }), 8);
});

test("the active ribbon line can wrap without changing its type scale", () => {
  assert.deepEqual(
    stableRibbonLineMetrics({ baseSize: 40, role: "current" }),
    { fontSize: 40, lineHeight: 43.2, maxLines: 2 },
  );
  assert.deepEqual(
    stableRibbonLineMetrics({ baseSize: 40, role: "adjacent" }),
    { fontSize: 24, lineHeight: 25.92, maxLines: 1 },
  );
});

test("ribbon art stays independent from changing lyric height", () => {
  assert.equal(ribbonContentHeight({ windowHeight: 130, scale: 1 }), 106);
  assert.equal(ribbonAlbumArtSize({ windowHeight: 130, scale: 1 }), 92);
  assert.equal(ribbonAlbumArtSize({ windowHeight: 80, scale: 1 }), 56);
});

test("Overlay wires the fixed grid and stable lyric slots into Ribbon", () => {
  const overlay = readFileSync(new URL("./Overlay.tsx", import.meta.url), "utf8");

  assert.match(overlay, /display: "grid"/);
  assert.match(overlay, /gridTemplateColumns: showArt && albumArt/);
  assert.match(overlay, /stableSlot/);
  assert.doesNotMatch(
    overlay,
    /fittedSize|measureRef|shrink-to-fit|setArtSize|lyricsColEl/,
  );
});
