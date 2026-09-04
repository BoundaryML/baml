import { AuthoredPage, authoredMetadata } from '@/components/authored-page';
import { GeneratedReleaseCatalog } from '@/components/generated-release-catalog';
import {
  listGeneratedReleaseSummaries,
  loadGeneratedReleaseSnapshot,
  selectFeaturedGeneratedRelease,
} from '@/lib/generated-content/build-content';

export const metadata = authoredMetadata('/cli');

export default async function CliPage() {
  const releases = await listGeneratedReleaseSummaries();
  const featuredRelease = selectFeaturedGeneratedRelease(releases);
  const featured = featuredRelease
    ? await loadGeneratedReleaseSnapshot(featuredRelease.routeVersion)
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
