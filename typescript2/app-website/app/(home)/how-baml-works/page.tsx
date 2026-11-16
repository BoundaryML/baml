import { createMetadata } from '@/app/_lib/metadata';
import { ArchitecturePage } from '@/components/adopt/architecture';
import { PageShell } from '@/components/home/page-shell';

export const metadata = createMetadata({
  description:
    'How BAML runs inside your app: built ahead of time, shipped as a library with no sidecar or server, and what happens at startup, on a call, on an exception and at shutdown.',
  ogSubtitle: 'Inside your app, as a library.',
  ogTitle: 'How BAML works',
  path: '/how-baml-works',
  title: 'How BAML works',
});

// The engineering behind BAML, split out of /adopt so that page stays about getting started. The
// nav's Platform menu and /adopt link here.

export default function Page() {
  return (
    <PageShell>
      <section className="v3-wrap mx-auto px-(--frame-pad) pt-16 pb-20 md:pt-24 md:pb-28">
        <ArchitecturePage />
      </section>
    </PageShell>
  );
}
