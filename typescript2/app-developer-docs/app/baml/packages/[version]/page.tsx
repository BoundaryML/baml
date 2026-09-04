import Link from 'next/link';
import { notFound } from 'next/navigation';

import { DocsShell } from '@/components/docs-shell';
import { GeneratedVersionSwitcher } from '@/components/generated-version-switcher';
import {
  isPrereleaseVersion,
  listGeneratedReleaseRouteVersions,
  loadGeneratedReleaseSnapshot,
} from '@/lib/generated-content/build-content';
import { listGeneratedVersionOptions } from '@/lib/generated-content/discovery';
import { documentationMetadata } from '@/lib/metadata';

export const dynamicParams = false;

export async function generateStaticParams() {
  const versions = await listGeneratedReleaseRouteVersions();
  return versions.map((version) => ({ version }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ version: string }>;
}) {
  const { version } = await params;
  const snapshot = await loadGeneratedReleaseSnapshot(version);
  if (!snapshot) return {};
  const channelLabel = snapshot.channels.length
    ? ` (${snapshot.channels.join(', ')})`
    : '';
  const description = `Immutable package reference generated from BAML ${snapshot.release.version}${channelLabel}.`;
  return documentationMetadata({
    description,
    index: !isPrereleaseVersion(snapshot.release.version),
    path: `/baml/packages/${version}`,
    title: `Standard packages ${version}`,
  });
}

export default async function PackageVersionPage({
  params,
}: {
  params: Promise<{ version: string }>;
}) {
  const { version } = await params;
  const snapshot = await loadGeneratedReleaseSnapshot(version);
  if (!snapshot) notFound();
  const packagePages = snapshot.pages.filter(
    (page) => page.page_kind === 'package',
  );
  const channelLabel = snapshot.channels.length
    ? ` (${snapshot.channels.join(', ')})`
    : '';
  const versionOptions = await listGeneratedVersionOptions({
    kind: 'package',
  });

  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { href: '/baml/packages', label: 'Standard packages' },
        { label: version },
      ]}
      description={`Immutable package reference generated from BAML ${snapshot.release.version}${channelLabel}.`}
      title={`Standard packages ${version}`}
      toc={[
        { href: '#packages', label: 'Packages' },
        { href: '#release', label: 'Release provenance' },
      ]}
    >
      <GeneratedVersionSwitcher options={versionOptions} />
      <h2 id="packages">Packages</h2>
      <ul>
        {packagePages.map((page) => (
          <li key={page.route_path}>
            <Link href={`/baml/packages/${version}/${page.route_path}`}>
              <code>{page.qualified_name}</code>
            </Link>
          </li>
        ))}
      </ul>
      <h2 id="release">Release provenance</h2>
      <dl>
        <dt>Released</dt>
        <dd>{snapshot.release.released_at.toISOString()}</dd>
        <dt>Source commit</dt>
        <dd>
          <code>{snapshot.release.source_commit}</code>
        </dd>
        <dt>Generator revision</dt>
        <dd>
          <code>{snapshot.release.generator_version}</code>
        </dd>
      </dl>
    </DocsShell>
  );
}
