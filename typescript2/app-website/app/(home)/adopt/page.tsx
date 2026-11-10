import { createMetadata } from '@/app/_lib/metadata';
import { AdoptGuide } from '@/components/adopt/guide';
import { highlightAll } from '@/components/adopt/highlight';
import { PageShell } from '@/components/home/page-shell';

export const metadata = createMetadata({
  description:
    'Adopt BAML incrementally, send your OpenTelemetry traces to Boundary Cloud, and see how BAML runs inside your app.',
  ogSubtitle: 'Incrementally adoptable.',
  ogTitle: 'Adopting Boundary and BAML',
  path: '/adopt',
  title: 'Adopting Boundary and BAML',
});

// Where the homepage's adoption cards lead: why not to rewrite, the three ways in (each with an
// anchor the cards link to), then how BAML runs, for anyone who wants the detail.

export default async function Page() {
  const html = await highlightAll();
  return (
    <PageShell>
      <AdoptGuide html={html} />
    </PageShell>
  );
}
