'use client';

import { ConvexProvider, ConvexReactClient } from 'convex/react';
import { useMemo } from 'react';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP } from '@/components/home/page-shell';
import PledgeForm from './pledge-form';
import PledgeWall from './pledge-wall';

// Both the pledge wall and the share form talk to Convex, so they live under a
// single ConvexProvider built from the shared deployment URL (the same
// NEXT_PUBLIC_CONVEX_URL the Sheep Council uses). Mirrors the safe-degrade
// pattern in ../../sheep-council/council-gate.tsx: if the URL isn't configured
// the section simply renders nothing rather than crashing the page.
export default function PledgeSection() {
  const url = process.env.NEXT_PUBLIC_CONVEX_URL;
  const client = useMemo(
    () => (url ? new ConvexReactClient(url) : null),
    [url],
  );

  if (!client) {
    return null;
  }

  return (
    <ConvexProvider client={client}>
      <section {...section('slop_pledges')} className="pb-16 md:pb-20">
        <PledgeWall />
      </section>

      <section
        {...section('slop_share')}
        className={`${PAGE_WRAP} pb-16 md:pb-24`}
      >
        <div className="mx-auto max-w-2xl">
          <h2 className="mb-8 text-center v4-h3">Share</h2>
          <PledgeForm />
        </div>
      </section>
    </ConvexProvider>
  );
}
