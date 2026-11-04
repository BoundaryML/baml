import { createMetadata } from '@/app/_lib/metadata';
import { section } from '@/components/home/analytics';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import PledgeSection from './_components/pledge-section';
import WarEpilogue from './_components/war-epilogue';
import WarScene from './_components/war-scene';
import './war-on-slop.css';

export const metadata = createMetadata({
  description:
    'A standing legion against the tide of AI slop. Kill slop with clean constructs at the language level.',
  ogTitle: 'fight slop with slop',
  path: '/fight-slop-with-slop',
  title: 'War on Slop',
});

// The War on Slop campaign page, in the homepage's BAML lane. It isn't linked from the nav or the
// footer.
export default function Page() {
  return (
    <PageShell>
      <div className="war-on-slop w-full overflow-x-clip">
        <section
          {...section('slop_hero')}
          className={`${PAGE_WRAP} relative z-10 pt-16 md:pt-24 text-center`}
        >
          <h1 className="v4-h1">fight slop with slop</h1>
          <p className="mt-4 v4-caption text-accent">
            by{' '}
            <a
              className="hover:underline underline-offset-4"
              href="https://x.com/boundaryml"
              rel="noopener noreferrer"
              target="_blank"
            >
              @boundaryml
            </a>
          </p>
        </section>

        <WarScene />

        <WarEpilogue />

        <PledgeSection />
      </div>
    </PageShell>
  );
}
