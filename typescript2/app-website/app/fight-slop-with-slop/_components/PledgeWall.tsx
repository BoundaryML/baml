'use client';

import { useQuery } from 'convex/react';
import { api } from '@/convex/_generated/api';

type Pledge = { id: string; name: string; description: string; createdAt: number };

const FALLBACK_PLEDGES: Pledge[] = [
  {
    id: 'fallback-1',
    name: 'Frontier Engineer',
    description: 'Replacing vague AI sludge with small, sharp tools that actually ship.',
    createdAt: 0,
  },
  {
    id: 'fallback-2',
    name: 'Design Infantry',
    description: 'Holding the line against disposable interfaces and beige autocomplete.',
    createdAt: 0,
  },
  {
    id: 'fallback-3',
    name: 'Build Cavalry',
    description: 'Turning model chaos into typed workflows, tests, and real product surfaces.',
    createdAt: 0,
  },
];

function PledgeCard({ pledge, clone }: { pledge: Pledge; clone?: boolean }) {
  return (
    <div aria-hidden={clone || undefined} className="tweet-font w-72 shrink-0 rounded-lg border border-wos-ink/10 bg-white/10 px-4 py-3 shadow-sm backdrop-blur-sm">
      <div className="text-[15px] font-bold leading-tight text-wos-ink">{pledge.name}</div>
      <p className="mt-1.5 text-[13px] leading-snug text-wos-ink-2">{pledge.description}</p>
    </div>
  );
}

export default function PledgeWall() {
  const pledges = useQuery(api.slopPledges.list);

  const visiblePledges = pledges && pledges.length > 0 ? pledges : FALLBACK_PLEDGES;

  // Repeat the list until it's wide enough to fill the row, THEN duplicate that
  // whole block once. The marquee scrolls exactly -50% (one block), so it loops
  // seamlessly for any number of pledges — no mid-set jump.
  let filled = [...visiblePledges];
  while (filled.length < 6) filled = [...filled, ...visiblePledges];
  const loop = [...filled, ...filled];

  // a second row, offset so the two rows don't show identical cards in step
  const filled2 = [...filled.slice(Math.floor(filled.length / 2)), ...filled.slice(0, Math.floor(filled.length / 2))];
  const loop2 = [...filled2, ...filled2];

  return (
    <div className="flex w-full flex-col gap-3 overflow-hidden">
      <div className="marquee-track flex w-max gap-3">
        {/* the second block is a visual clone only — hide it from a11y so
            screen readers don't read every pledge twice */}
        {loop.map((p, i) => (
          <PledgeCard key={`${p.id}-${i}`} pledge={p} clone={i >= filled.length} />
        ))}
      </div>
      <div className="marquee-track-rev flex w-max gap-3" aria-hidden="true">
        {loop2.map((p, i) => (
          <PledgeCard key={`r2-${p.id}-${i}`} pledge={p} clone />
        ))}
      </div>
    </div>
  );
}
