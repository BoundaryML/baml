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
  const description = `Every public command captured from the exact BAML ${snapshot.release.version} executable.`;
  return documentationMetadata({
    description,
    index: !isPrereleaseVersion(snapshot.release.version),
    path: `/cli/${version}/commands`,
    title: `Command index — ${version}`,
  });
}

export default async function CliCommandsPage({
  params,
}: {
  params: Promise<{ version: string }>;
}) {
  const { version } = await params;
  const snapshot = await loadGeneratedReleaseSnapshot(version);
  if (!snapshot) notFound();
  const versionOptions = await listGeneratedVersionOptions({
    kind: 'cli-command-index',
  });

  return (
    <DocsShell
      breadcrumbs={[
        { href: '/cli', label: 'BAML CLI' },
        { href: `/cli/${version}`, label: version },
        { label: 'Commands' },
      ]}
      description={`Every public command captured from the exact BAML ${snapshot.release.version} executable.`}
      title="Command index"
      toc={[{ href: '#commands', label: 'Commands' }]}
    >
      <GeneratedVersionSwitcher options={versionOptions} />
      <h2 id="commands">Commands</h2>
      <CliCommandTree
        commands={snapshot.cli.payload.root.subcommands}
        routeVersion={version}
      />
    </DocsShell>
  );
}
