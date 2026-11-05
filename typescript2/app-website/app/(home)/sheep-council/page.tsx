import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PageShell } from '@/components/home/page-shell';
import { CouncilGate } from './council-gate';

export const metadata = createMetadata({
  description: 'Members only. Speak the words to enter the chamber.',
  indexable: false,
  ogTitle: 'The Sheep Council',
  path: '/sheep-council',
  title: 'The Sheep Council',
});

/** The Sheep Council's members-only page, in the homepage's BAML lane. It isn't linked from the nav
 *  or the footer. The hand-drawn council chamber fills the space between the nav and the footer,
 *  under a scrim of the theme's ground so the glass card reads on top. */
export default function SheepCouncilPage() {
  return (
    <PageShell>
      <section
        {...section('sheep_council')}
        className="relative isolate flex min-h-[calc(100svh-72px)] items-center justify-center px-(--frame-pad) py-16"
      >
        <div
          aria-hidden
          className="absolute inset-0 -z-10 bg-[url('/sheep-council.png')] bg-cover bg-center"
        />
        <div
          aria-hidden
          className="absolute inset-0 -z-10 bg-[radial-gradient(ellipse_at_center,color-mix(in_srgb,var(--ground)_40%,transparent)_0%,color-mix(in_srgb,var(--ground)_80%,transparent)_100%)]"
        />
        <CouncilGate />
      </section>
    </PageShell>
  );
}
