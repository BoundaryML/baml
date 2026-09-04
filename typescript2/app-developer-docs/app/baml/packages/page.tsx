import Link from 'next/link';

import { DocsShell } from '@/components/docs-shell';
import { GeneratedReleaseCatalog } from '@/components/generated-release-catalog';
import {
  listGeneratedReleaseSummaries,
  loadGeneratedReleaseSnapshot,
  selectFeaturedGeneratedRelease,
} from '@/lib/generated-content/build-content';
import { documentationMetadata } from '@/lib/metadata';

export const metadata = documentationMetadata({
  description: 'Versioned reference for BAML standard packages.',
  path: '/baml/packages',
  title: 'Standard packages',
});

export default async function PackagesPage() {
  const releases = await listGeneratedReleaseSummaries();
  const featuredRelease = selectFeaturedGeneratedRelease(releases);
  const featured = featuredRelease
    ? await loadGeneratedReleaseSnapshot(featuredRelease.routeVersion)
    : null;

  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { label: 'Standard packages' },
      ]}
      description="Versioned package reference generated from the exact compiled BAML toolchain."
      title="Standard packages"
      toc={[{ href: '#published', label: 'Published reference' }]}
    >
      <h2 id="published">Published reference</h2>
      <p>
        Choose a published release to browse reference generated from that exact
        BAML toolchain. Exact-version URLs never move or silently fall forward
        to another release.
      </p>
      <GeneratedReleaseCatalog
        featured={featured}
        product="packages"
        releases={releases}
      />
      <p>
        Use the <Link href="/changelog">changelog</Link> to review language,
        toolchain, and package changes between releases.
      </p>
    </DocsShell>
  );
}
