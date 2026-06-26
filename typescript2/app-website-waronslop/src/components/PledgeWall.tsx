'use client';

import { useQuery } from 'convex/react';
import { api } from '../../convex/_generated/api';

type Pledge = { id: string; name: string; description: string; createdAt: number };

function PledgeCard({ pledge, clone }: { pledge: Pledge; clone?: boolean }) {
  return (
    <div aria-hidden={clone || undefined} className="tweet-font w-72 shrink-0 rounded-xl border border-black/10 bg-white p-3">
      <div className="flex items-center gap-2">
        <div className="flex h-8 w-8 items-center justify-center rounded-full bg-black/[0.06] text-sm font-bold text-black/60">
          {pledge.name.charAt(0).toUpperCase()}
        </div>
        <div className="text-[14px] font-bold leading-tight text-black">{pledge.name}</div>
      </div>
      <p className="mt-2 text-[13px] leading-snug text-black">{pledge.description}</p>
    </div>
  );
}

export default function PledgeWall() {
  const pledges = useQuery(api.submissions.list);

  // Nothing to show yet (loading or empty) — render nothing.
  if (pledges === undefined || pledges.length === 0) {
    return null;
  }

  // Repeat the list until it's wide enough to fill the row, THEN duplicate that
  // whole block once. The marquee scrolls exactly -50% (one block), so it loops
  // seamlessly for any number of pledges — no mid-set jump.
  let filled = [...pledges];
  while (filled.length < 6) filled = [...filled, ...pledges];
  const loop = [...filled, ...filled];

  return (
    <div className="w-full overflow-hidden">
      <div className="marquee-track flex w-max gap-3">
        {/* the second block is a visual clone only — hide it from a11y so
            screen readers don't read every pledge twice */}
        {loop.map((p, i) => (
          <PledgeCard key={`${p.id}-${i}`} pledge={p} clone={i >= filled.length} />
        ))}
      </div>
    </div>
  );
}
