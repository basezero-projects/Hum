const LYRICS_COMPOSITION_WIDTH_PX = 760;

export function ribbonCompositionMaxWidth({
  availableWidth,
  scale,
}: {
  availableWidth: number;
  scale: number;
}): number {
  const safeWidth = Math.max(0, availableWidth);
  const safeScale = Math.max(0, scale);
  const intendedWidth = LYRICS_COMPOSITION_WIDTH_PX * safeScale;

  return Math.min(safeWidth, intendedWidth);
}

export function ribbonLineFontSize({
  baseSize,
  role,
}: {
  baseSize: number;
  role: "current" | "adjacent";
}): number {
  return role === "current" ? baseSize : Math.max(8, baseSize * 0.6);
}

export function stableRibbonLineMetrics({
  baseSize,
  role,
}: {
  baseSize: number;
  role: "current" | "adjacent";
}): {
  fontSize: number;
  lineHeight: number;
  maxLines: number;
} {
  const fontSize = ribbonLineFontSize({ baseSize, role });
  const lineHeight = fontSize * 1.08;
  const maxLines = role === "current" ? 2 : 1;

  return {
    fontSize,
    lineHeight,
    maxLines,
  };
}

export function ribbonContentHeight({
  windowHeight,
  scale,
}: {
  windowHeight: number;
  scale: number;
}): number {
  return Math.max(0, windowHeight - 24 * Math.max(0, scale));
}

export function ribbonAlbumArtSize({
  windowHeight,
  scale,
}: {
  windowHeight: number;
  scale: number;
}): number {
  const safeScale = Math.max(0, scale);
  return Math.min(92 * safeScale, ribbonContentHeight({ windowHeight, scale }));
}
