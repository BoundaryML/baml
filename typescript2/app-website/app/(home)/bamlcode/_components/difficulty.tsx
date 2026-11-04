import type { Difficulty } from '../_lib/types';

const LEVEL: Record<Difficulty, number> = { Easy: 1, Hard: 3, Medium: 2 };

/** A problem's difficulty: one, two, or three Clover pips and the label. The
 *  pips carry the level, so the scale reads without a traffic-light palette. */
export function DifficultyTag({
  difficulty,
  label = difficulty,
}: {
  difficulty: Difficulty;
  label?: string;
}) {
  return (
    <span className="bc-diff" data-level={LEVEL[difficulty]}>
      <span aria-hidden className="bc-pips">
        <i />
        <i />
        <i />
      </span>
      {label}
    </span>
  );
}
