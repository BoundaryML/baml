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

export default async function CliCommandsPage({
  params,
}: {
  params: Promise<{ version: string }>;
}) {
  const { version } = await params;
  const snapshot = await loadGeneratedReleaseSnapshot(version);
  if (!snapshot) notFound();

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
      <h2 id="commands">Commands</h2>
      <CliCommandTree
        commands={snapshot.cli.payload.root.subcommands}
        routeVersion={version}
      />
    </DocsShell>
  );
}
