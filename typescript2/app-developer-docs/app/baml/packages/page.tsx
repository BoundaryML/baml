import { DocsShell } from '@/components/docs-shell';
import { GeneratedReleaseCatalog } from '@/components/generated-release-catalog';
import {
  listDocumentReleaseSummaries,
  readDocumentRoute,
} from '@/lib/generated-content/document-store';
import { documentationMetadata } from '@/lib/metadata';

export const metadata = documentationMetadata({
  description: 'Versioned reference for BAML standard packages.',
  path: '/baml/packages',
  title: 'Standard packages',
});
export const dynamic = 'force-dynamic';
export const revalidate = 0;

export default async function PackagesPage() {
  const releases = await listDocumentReleaseSummaries();
  const featuredRelease =
    releases.find((release) => release.aliases.includes('stable')) ??
    releases.find((release) => release.aliases.includes('canary')) ??
    releases.find((release) => release.aliases.includes('nightly')) ??
    releases[0];
  const featured = featuredRelease
    ? await readDocumentRoute(featuredRelease.routeVersion, 'baml/packages')
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
    </DocsShell>
  );
}
