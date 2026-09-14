import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
import { GeneratedReleaseCatalog } from '@/components/generated-release-catalog';
import {
  listDocumentReleaseSummaries,
  readDocumentRoute,
} from '@/lib/generated-content/document-store';

export const metadata = authoredMetadata('/cli');
export const dynamic = 'force-dynamic';
export const revalidate = 0;

export default async function CliPage() {
  const releases = await listDocumentReleaseSummaries();
  const featuredRelease =
    releases.find((release) => release.aliases.includes('stable')) ??
    releases.find((release) => release.aliases.includes('canary')) ??
    releases.find((release) => release.aliases.includes('nightly')) ??
    releases[0];
  const featured = featuredRelease
    ? await readDocumentRoute(featuredRelease.routeVersion, 'cli')
    : null;

  return (
    <AuthoredPage
      components={{
        GeneratedReleaseCatalog: () => (
          <GeneratedReleaseCatalog
            featured={featured}
            product="cli"
            releases={releases}
          />
        ),
      }}
      path="/cli"
    />
  );
}
