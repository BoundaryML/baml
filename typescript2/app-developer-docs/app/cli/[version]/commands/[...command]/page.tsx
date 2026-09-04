import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { DocsShell } from '@/components/docs-shell';
import { CliCommandContent } from '@/components/generated-cli';
import { GeneratedVersionSwitcher } from '@/components/generated-version-switcher';
import {
  isPrereleaseVersion,
  listGeneratedReleaseRouteVersions,
  loadGeneratedReleaseSnapshot,
} from '@/lib/generated-content/build-content';
import {
  findCliCommand,
  flattenCliCommands,
} from '@/lib/generated-content/cli-routes';
import { listGeneratedVersionOptions } from '@/lib/generated-content/discovery';
import { documentationMetadata } from '@/lib/metadata';

export const dynamicParams = false;

export async function generateStaticParams() {
  const versions = await listGeneratedReleaseRouteVersions();
  const params = await Promise.all(
    versions.map(async (version) => {
      const snapshot = await loadGeneratedReleaseSnapshot(version);
      return snapshot
        ? flattenCliCommands(snapshot.cli.payload.root).map((command) => ({
            command: command.command_path,
            version,
          }))
        : [];
    }),
  );
  return params.flat();
}

async function loadCommand(version: string, commandPath: readonly string[]) {
  const snapshot = await loadGeneratedReleaseSnapshot(version);
  if (!snapshot) return null;
  return findCliCommand(snapshot.cli.payload.root, commandPath);
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ command: string[]; version: string }>;
}): Promise<Metadata> {
  const { command, version } = await params;
  const commandNode = await loadCommand(version, command);
  if (!commandNode) return {};
  const snapshot = await loadGeneratedReleaseSnapshot(version);
  const description =
    commandNode.description ?? `Reference for baml ${command.join(' ')}.`;
  return documentationMetadata({
    description,
    index: snapshot !== null && !isPrereleaseVersion(snapshot.release.version),
    path: `/cli/${version}/commands/${command.join('/')}`,
    title: `baml ${command.join(' ')} — ${version}`,
  });
}

export default async function CliCommandPage({
  params,
}: {
  params: Promise<{ command: string[]; version: string }>;
}) {
  const { command, version } = await params;
  const commandNode = await loadCommand(version, command);
  if (!commandNode) notFound();
  const versionOptions = await listGeneratedVersionOptions({
    commandPath: command,
    kind: 'cli-command',
  });
  const breadcrumbs = [
    { href: '/cli', label: 'BAML CLI' },
    { href: `/cli/${version}`, label: version },
    { href: `/cli/${version}/commands`, label: 'Commands' },
    ...command.map((token, index) => ({
      href:
        index === command.length - 1
          ? undefined
          : `/cli/${version}/commands/${command.slice(0, index + 1).join('/')}`,
      label: token,
    })),
  ];
  const toc = [
    { href: '#usage', label: 'Usage' },
    ...(commandNode.subcommands.length > 0
      ? [{ href: '#subcommands', label: 'Subcommands' }]
      : []),
    ...(commandNode.arguments.length > 0
      ? [{ href: '#arguments', label: 'Arguments' }]
      : []),
    ...(commandNode.flags.length > 0
      ? [{ href: '#options', label: 'Options' }]
      : []),
  ];

  return (
    <DocsShell
      breadcrumbs={breadcrumbs}
      description={
        commandNode.description ?? 'Exact-version generated command reference.'
      }
      title={`baml ${command.join(' ')}`}
      toc={toc}
    >
      <GeneratedVersionSwitcher options={versionOptions} />
      <CliCommandContent command={commandNode} routeVersion={version} />
    </DocsShell>
  );
}
