import assert from "node:assert/strict";
import test from "node:test";

import { wordFillPercent } from "./karaoke-presentation.ts";

test("past and future karaoke words use stable readable fills", () => {
  assert.equal(wordFillPercent(2, 3, 0.4), 100);
  assert.equal(wordFillPercent(4, 3, 0.4), 0);
});

test("the active karaoke word follows and clamps playback progress", () => {
  assert.equal(wordFillPercent(3, 3, 0), 0);
  assert.equal(wordFillPercent(3, 3, 0.42), 42);
  assert.equal(wordFillPercent(3, 3, 1), 100);
  assert.equal(wordFillPercent(3, 3, -1), 0);
  assert.equal(wordFillPercent(3, 3, 2), 100);
});

