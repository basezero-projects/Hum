export function wordFillPercent(
  wordIndex: number,
  currentIndex: number,
  currentProgress: number,
): number {
  if (wordIndex < currentIndex) return 100;
  if (wordIndex > currentIndex) return 0;
  return 100 * Math.max(0, Math.min(1, currentProgress));
}

