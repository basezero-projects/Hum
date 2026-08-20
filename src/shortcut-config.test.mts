import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  displayShortcut,
  triggerFromKeyboardInput,
  triggerFromMouseInput,
} from "./shortcut-config.ts";

test("shortcut labels keep the fixed global chord", () => {
  assert.equal(displayShortcut("ArrowLeft"), "Ctrl + Alt + Left");
  assert.equal(displayShortcut("Mouse4"), "Ctrl + Alt + Mouse 4");
});

test("keyboard recording requires Ctrl and Alt", () => {
  assert.equal(
    triggerFromKeyboardInput({ code: "ArrowRight", ctrlKey: true, altKey: true }),
    "ArrowRight",
  );
  assert.equal(
    triggerFromKeyboardInput({ code: "ArrowRight", ctrlKey: true, altKey: false }),
    null,
  );
  assert.equal(
    triggerFromKeyboardInput({ code: "ControlLeft", ctrlKey: true, altKey: true }),
    null,
  );
});

test("mouse recording accepts only global back and forward buttons", () => {
  assert.equal(
    triggerFromMouseInput({ button: 3, ctrlKey: true, altKey: true }),
    "Mouse4",
  );
  assert.equal(
    triggerFromMouseInput({ button: 4, ctrlKey: true, altKey: true }),
    "Mouse5",
  );
  assert.equal(
    triggerFromMouseInput({ button: 0, ctrlKey: true, altKey: true }),
    null,
  );
  assert.equal(
    triggerFromMouseInput({ button: 3, ctrlKey: false, altKey: true }),
    null,
  );
});

test("Settings records bindings through the native shortcut commands", () => {
  const source = readFileSync(new URL("./Settings.tsx", import.meta.url), "utf8");

  assert.match(source, /invoke<Settings>\("set_shortcut_binding"/);
  assert.match(source, /invoke<Settings>\("reset_shortcuts"\)/);
  assert.match(source, /triggerFromKeyboardInput\(event\)/);
  assert.match(source, /triggerFromMouseInput\(event\)/);
  assert.doesNotMatch(source, /Ctrl\+Alt\+\[/);
});
