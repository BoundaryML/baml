'use client';

import { useAnimateInView } from '../../learn3/_lib/use-animate-in-view';

/**
 * Recursive self-improvement — "the sweep". A field of small squares;
 * three ripples of refinement radiate from the center per cycle. Each
 * pass, a deterministic subset of cells keeps its improvement (and
 * deepens on later passes), then the whole field resets and the loop
 * starts over. CSS-only: per-cell animation-delay carries the ripple,
 * the latch classes carry the accumulation. Holds until scrolled into
 * view.
 */

const COLS = 16;
const ROWS = 9;
const CX = (COLS - 1) / 2;
const CY = (ROWS - 1) / 2;

interface SweepCell {
  key: string;
  /** Ripple offset: distance from the field's center. */
  delay: number;
  /** 0 = never latches; 1..3 = keeps its improvement after pass N. */
  latch: 0 | 1 | 2 | 3;
}

const CELLS: SweepCell[] = [];
for (let r = 0; r < ROWS; r += 1) {
  for (let c = 0; c < COLS; c += 1) {
    // Deterministic scatter (no randomness — keeps SSR/client identical):
    // ~27% of cells latch, spread across the three passes.
    const h = (r * 31 + c * 17) % 11;
    CELLS.push({
      delay: Math.hypot(c - CX, r - CY) * 0.09,
      key: `${r}-${c}`,
      latch: h < 3 ? ((h + 1) as 1 | 2 | 3) : 0,
    });
  }
}

export function SelfImprove() {
  const { ref, holdClass } = useAnimateInView();
  return (
    <div aria-hidden className={`l6-si-sweep${holdClass}`} ref={ref}>
      {CELLS.map((cell) => (
        <span
          className={`l6-swp-cell${cell.latch ? ` l6-swp-l${cell.latch}` : ''}`}
          key={cell.key}
          style={{ animationDelay: `${cell.delay.toFixed(2)}s` }}
        />
      ))}
    </div>
  );
}
