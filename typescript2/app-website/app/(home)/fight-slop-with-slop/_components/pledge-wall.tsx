'use client';

import { useQuery } from 'convex/react';
import { api } from '@/convex/_generated/api';

type Pledge = {
  id: string;
  name: string;
  description: string;
  createdAt: number;
};

const FALLBACK_PLEDGES: Pledge[] = [
  {
    createdAt: 0,
    description:
      'Replacing vague AI sludge with small, sharp tools that actually ship.',
    id: 'fallback-1',
    name: 'Frontier Engineer',
  },
  {
    createdAt: 0,
    description:
      'Holding the line against disposable interfaces and beige autocomplete.',
    id: 'fallback-2',
    name: 'Design Infantry',
  },
  {
    createdAt: 0,
    description:
      'Turning model chaos into typed workflows, tests, and real product surfaces.',
    id: 'fallback-3',
    name: 'Build Cavalry',
  },
];

/** One pledge's name and description. A `clone` is hidden from screen readers. */
function PledgeCard({ pledge, clone }: { pledge: Pledge; clone?: boolean }) {
  return (
    <div
      aria-hidden={clone || undefined}
      className="w-72 shrink-0 rounded-xl border border-rule bg-raised px-4 py-3"
    >
      <div className="font-display text-[15px] font-medium leading-tight text-head">
        {pledge.name}
      </div>
      <p className="mt-1.5 text-[13px] leading-snug text-body">
        {pledge.description}
      </p>
    </div>
  );
}

/**
 * Two rows of pledges scrolling in opposite directions, from Convex or a
 * fallback set when there are none yet.
 */
export default function PledgeWall() {
  const pledges = useQuery(api.slopPledges.list);

  const visiblePledges =
    pledges && pledges.length > 0 ? pledges : FALLBACK_PLEDGES;

  // Repeat the list until it's wide enough to fill the row, THEN duplicate that
  // whole block once. The marquee scrolls exactly -50% (one block), so it loops
  // seamlessly for any number of pledges — no mid-set jump.
  let filled = [...visiblePledges];
  while (filled.length < 6) filled = [...filled, ...visiblePledges];
  const loop = [...filled, ...filled];

  // a second row, offset so the two rows don't show identical cards in step
  const filled2 = [
    ...filled.slice(Math.floor(filled.length / 2)),
    ...filled.slice(0, Math.floor(filled.length / 2)),
  ];
  const loop2 = [...filled2, ...filled2];

  return (
    <div className="flex w-full flex-col gap-3 overflow-hidden">
      <div className="marquee-track flex w-max gap-3">
        {/* the second block is a visual clone only — hide it from a11y so
            screen readers don't read every pledge twice */}
        {loop.map((p, i) => (
          <PledgeCard
            clone={i >= filled.length}
            key={`${p.id}-${i}`}
            pledge={p}
          />
        ))}
      </div>
      <div aria-hidden="true" className="marquee-track-rev flex w-max gap-3">
        {loop2.map((p, i) => (
          <PledgeCard clone key={`r2-${p.id}-${i}`} pledge={p} />
        ))}
      </div>
    </div>
  );
}
