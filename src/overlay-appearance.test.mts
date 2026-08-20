import assert from "node:assert/strict";
import test from "node:test";

import { resolveOverlayTextAppearance } from "./overlay-appearance.ts";

const configured = {
  textColor: "#f4f0e8",
  textColorDim: "#a8a39a",
};

test("transparent overlays keep bright text when the sampled desktop is light", () => {
  const appearance = resolveOverlayTextAppearance({
    ...configured,
    autoContrast: true,
    surfaceIsLight: true,
    backgroundHidden: true,
  });

  assert.equal(appearance.textColor, "#ffffff");
  assert.equal(appearance.textColorDim, "#c8c8c8");
  assert.equal(appearance.useDarkLogo, false);
  assert.match(appearance.textShadow, /rgba\(0,0,0/);
});

test("owned light backgrounds use dark text", () => {
  const appearance = resolveOverlayTextAppearance({
    ...configured,
    autoContrast: true,
    surfaceIsLight: true,
    backgroundHidden: false,
  });

  assert.equal(appearance.textColor, "#0a0a0a");
  assert.equal(appearance.textColorDim, "#5a5a5a");
  assert.equal(appearance.useDarkLogo, true);
  assert.match(appearance.textShadow, /rgba\(255,255,255/);
});

test("owned dark backgrounds use bright text", () => {
  const appearance = resolveOverlayTextAppearance({
    ...configured,
    autoContrast: true,
    surfaceIsLight: false,
    backgroundHidden: false,
  });

  assert.equal(appearance.textColor, "#ffffff");
  assert.equal(appearance.textColorDim, "#c8c8c8");
  assert.equal(appearance.useDarkLogo, false);
});

test("disabled auto contrast preserves configured colors", () => {
  const appearance = resolveOverlayTextAppearance({
    ...configured,
    autoContrast: false,
    surfaceIsLight: true,
    backgroundHidden: false,
  });

  assert.equal(appearance.textColor, configured.textColor);
  assert.equal(appearance.textColorDim, configured.textColorDim);
  assert.equal(appearance.useDarkLogo, false);
});
