'use client';

import { useMemo } from 'react';
import { ConvexProvider, ConvexReactClient } from 'convex/react';
import PledgeWall from './PledgeWall';
import PledgeForm from './PledgeForm';

// Both the pledge wall and the share form talk to Convex, so they live under a
// single ConvexProvider built from the shared deployment URL (the same
// NEXT_PUBLIC_CONVEX_URL the Sheep Council uses). Mirrors the safe-degrade
// pattern in app/sheep-council/council-gate.tsx: if the URL isn't configured
// the section simply renders nothing rather than crashing the page.
export default function PledgeSection() {
  const url = process.env.NEXT_PUBLIC_CONVEX_URL;
  const client = useMemo(() => (url ? new ConvexReactClient(url) : null), [url]);

  if (!client) {
    return null;
  }

  return (
    <ConvexProvider client={client}>
      <section className="pb-16 sm:pb-20">
        <PledgeWall />
      </section>

      <section className="mx-auto max-w-2xl px-6 pb-28">
        <h2 className="mb-8 text-center text-3xl font-bold tracking-tight text-wos-ink sm:text-4xl">Share</h2>
        <PledgeForm />
      </section>
    </ConvexProvider>
  );
}
