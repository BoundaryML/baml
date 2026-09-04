import Link from 'next/link';
import { notFound } from 'next/navigation';
import { DocsShell } from '@/components/docs-shell';
import { CliCommandTree } from '@/components/generated-cli';
import {
  listGeneratedReleaseRouteVersions,
  loadGeneratedReleaseSnapshot,
} from '@/lib/generated-content/build-content';

export const dynamicParams = false;

export async function generateStaticParams() {
  const versions = await listGeneratedReleaseRouteVersions();
  return versions.map((version) => ({ version }));
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
