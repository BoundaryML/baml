import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { cache } from 'react';

import { DocsShell } from '@/components/docs-shell';
import { CliCommandContent, CliCommandTree } from '@/components/generated-cli';
import { GeneratedVersionSwitcher } from '@/components/generated-version-switcher';
import { readDocumentRoute } from '@/lib/generated-content/document-store';
import { isPrereleaseVersion } from '@/lib/generated-content/versions';
import { documentationMetadata } from '@/lib/metadata';

export const dynamic = 'force-dynamic';
export const revalidate = 0;

const loadDocument = cache(readDocumentRoute);

function storedPath(path: readonly string[] | undefined): string {
  return path?.length ? `cli/${path.join('/')}` : 'cli';
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ path?: string[]; version: string }>;
}): Promise<Metadata> {
  const { path, version } = await params;
  const document = await loadDocument(version, storedPath(path));
  if (!document) return {};
  return documentationMetadata({
    description: document.route_metadata.description,
    index: !isPrereleaseVersion(document.version),
    path: document.route_metadata.publicPath,
    title: document.route_metadata.title,
  });
}

export default async function CliDocumentPage({
  params,
}: {
  params: Promise<{ path?: string[]; version: string }>;
}) {
  const { path, version } = await params;
  const document = await loadDocument(version, storedPath(path));
  if (!document || document.route_metadata.surface !== 'cli') notFound();
  const block = document.content.blocks[0];
  const controls = (
    <GeneratedVersionSwitcher
      currentHref={document.route_metadata.publicPath}
      currentRouteVersion={version}
      storedPath={document.path}
    />
  );
  const toc = document.content.headings.map((heading) => ({
    href: `#${heading.id}`,
    label: heading.label,
  }));

  if (block?.type === 'cliOverview') {
    return (
      <DocsShell
        breadcrumbs={[{ href: '/cli', label: 'BAML CLI' }, { label: version }]}
        description={document.route_metadata.description}
        headerControls={controls}
        title={`${document.content.title} ${version}`}
        toc={toc}
      >
        <h2 id="usage">Usage</h2>
        <pre>
          <code>{block.root.usage}</code>
        </pre>
        <h2 id="commands">Commands</h2>
        <p>
          <Link href={`/cli/${version}/commands`}>
            Browse the command index
          </Link>
        </p>
        <CliCommandTree
          commands={block.root.subcommands}
          routeVersion={version}
        />
        <h2 id="release">Release provenance</h2>
        <p>
          Generated from wrapper{' '}
          <code>{document.route_metadata.wrapperVersion}</code> and BAML{' '}
          <code>{document.version}</code> at source commit{' '}
          <code>{document.route_metadata.sourceRevision}</code>.
        </p>
      </DocsShell>
    );
  }

  if (block?.type === 'cliCommandIndex') {
    return (
      <DocsShell
        breadcrumbs={[
          { href: '/cli', label: 'BAML CLI' },
          { href: `/cli/${version}`, label: version },
          { label: 'Commands' },
        ]}
        description={document.route_metadata.description}
        headerControls={controls}
        title={document.content.title}
        toc={toc}
      >
        <h2 id="commands">Commands</h2>
        <CliCommandTree commands={block.commands} routeVersion={version} />
      </DocsShell>
    );
  }

  if (block?.type !== 'cliCommand') notFound();
  const commandPath = block.command.command_path;
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/cli', label: 'BAML CLI' },
        { href: `/cli/${version}`, label: version },
        { href: `/cli/${version}/commands`, label: 'Commands' },
        ...commandPath.map((token, index) => ({
          href:
            index === commandPath.length - 1
              ? undefined
              : `/cli/${version}/commands/${commandPath.slice(0, index + 1).join('/')}`,
          label: token,
        })),
      ]}
      description={document.route_metadata.description}
      headerControls={controls}
      title={document.content.title}
      toc={toc}
    >
      <CliCommandContent command={block.command} routeVersion={version} />
    </DocsShell>
  );
}
