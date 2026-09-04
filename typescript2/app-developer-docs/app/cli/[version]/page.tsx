import Link from 'next/link';
import { notFound } from 'next/navigation';
import { DocsShell } from '@/components/docs-shell';
import { CliCommandTree } from '@/components/generated-cli';
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
  const description =
    snapshot.cli.payload.root.description ??
    `Exact command reference for BAML ${snapshot.release.version}.`;
  return documentationMetadata({
    description,
    index: !isPrereleaseVersion(snapshot.release.version),
    path: `/cli/${version}`,
    title: `BAML CLI ${version}`,
  });
}

export default async function CliVersionPage({
  params,
}: {
  params: Promise<{ version: string }>;
}) {
  const { version } = await params;
  const snapshot = await loadGeneratedReleaseSnapshot(version);
  if (!snapshot) notFound();
  const root = snapshot.cli.payload.root;
  const versionOptions = await listGeneratedVersionOptions({
    kind: 'cli-overview',
  });

  return (
    <DocsShell
      breadcrumbs={[{ href: '/cli', label: 'BAML CLI' }, { label: version }]}
      description={
        root.description ??
        `Exact command reference for BAML ${snapshot.release.version}.`
      }
      title={`BAML CLI ${version}`}
      toc={[
        { href: '#usage', label: 'Usage' },
        { href: '#commands', label: 'Commands' },
        { href: '#release', label: 'Release provenance' },
      ]}
    >
      <GeneratedVersionSwitcher options={versionOptions} />
      <h2 id="usage">Usage</h2>
      <pre>
        <code>{root.usage}</code>
      </pre>
      <h2 id="commands">Commands</h2>
      <p>
        <Link href={`/cli/${version}/commands`}>Browse the command index</Link>
      </p>
      <CliCommandTree commands={root.subcommands} routeVersion={version} />
      <h2 id="release">Release provenance</h2>
      <p>
        Generated from wrapper <code>{snapshot.cli.row.wrapper_version}</code>{' '}
        and BAML <code>{snapshot.release.version}</code> at source commit{' '}
        <code>{snapshot.release.source_commit}</code>.
      </p>
    </DocsShell>
  );
}
