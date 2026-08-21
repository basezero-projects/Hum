const SLOW_LINE_LEAD_MAX_MS = 420;
const LINE_GAP_LEAD_RATIO = 0.18;

export type TimedLyric = { time_ms: number };

export function lyricPresentationThresholdMs(
  lines: TimedLyric[],
  index: number,
): number {
  const line = lines[index];
  if (!line) return Number.POSITIVE_INFINITY;

  const referenceTime = index > 0
    ? lines[index - 1]?.time_ms
    : lines[index + 1]?.time_ms;
  const gapMs = referenceTime === undefined
    ? 0
    : Math.abs(line.time_ms - referenceTime);
  const leadMs = Math.min(SLOW_LINE_LEAD_MAX_MS, gapMs * LINE_GAP_LEAD_RATIO);

  return Math.max(0, line.time_ms - leadMs);
}

export function lyricPresentationCursorIndex(
  lines: TimedLyric[],
  positionMs: number,
): number {
  let low = 0;
  let high = lines.length;
  let found = -1;

  while (low < high) {
    const middle = (low + high) >> 1;
    if (lyricPresentationThresholdMs(lines, middle) <= positionMs) {
      found = middle;
      low = middle + 1;
    } else {
      high = middle;
    }
  }

  return found;
}
