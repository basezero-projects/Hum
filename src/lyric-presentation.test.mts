import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  lyricPresentationCursorIndex,
  lyricPresentationThresholdMs,
} from "./lyric-presentation.ts";

test("slow lyrics begin a gentle transition before the vocal timestamp", () => {
  const lines = [{ time_ms: 1_000 }, { time_ms: 5_000 }];

  assert.equal(lyricPresentationThresholdMs(lines, 1), 4_580);
  assert.equal(lyricPresentationCursorIndex(lines, 4_579), 0);
  assert.equal(lyricPresentationCursorIndex(lines, 4_580), 1);
});

test("rapid lyrics use a much shorter transition lead", () => {
  const lines = [{ time_ms: 1_000 }, { time_ms: 1_500 }];

  assert.equal(lyricPresentationThresholdMs(lines, 1), 1_410);
  assert.equal(lyricPresentationCursorIndex(lines, 1_409), 0);
  assert.equal(lyricPresentationCursorIndex(lines, 1_410), 1);
});

test("the first lyric can enter gently before singing begins", () => {
  const lines = [{ time_ms: 2_000 }, { time_ms: 5_000 }];

  assert.equal(lyricPresentationThresholdMs(lines, 0), 1_580);
  assert.equal(lyricPresentationCursorIndex(lines, 1_579), -1);
  assert.equal(lyricPresentationCursorIndex(lines, 1_580), 0);
});

test("Overlay and CSS use the adaptive cursor and gentle motion contract", () => {
  const overlay = readFileSync(new URL("./Overlay.tsx", import.meta.url), "utf8");
  const css = readFileSync(new URL("./index.css", import.meta.url), "utf8");

  assert.match(overlay, /lyricPresentationThresholdMs\(lines, idx \+ 1\)/);
  assert.match(css, /560ms cubic-bezier\(0\.22, 1, 0\.36, 1\)/);
  assert.doesNotMatch(css, /cubic-bezier\(0\.34, 1\.56, 0\.64, 1\)/);
});
