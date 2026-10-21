import type { Metadata } from 'next';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { cache } from 'react';

import { DocsShell } from '@/components/docs-shell';
import {
  GeneratedReferenceContent,
  referencePageTableOfContents,
} from '@/components/generated-reference';
import { GeneratedVersionSwitcher } from '@/components/generated-version-switcher';
import { readDocumentRoute } from '@/lib/generated-content/document-store';
import { packageDocumentPath } from '@/lib/generated-content/routes';
import { isPrereleaseVersion } from '@/lib/generated-content/versions';
import { documentationMetadata } from '@/lib/metadata';

export const dynamic = 'force-dynamic';
export const revalidate = 0;

const loadDocument = cache(readDocumentRoute);

export async function generateMetadata({
  params,
}: {
  params: Promise<{ fqn?: string[]; version: string }>;
}): Promise<Metadata> {
  const { fqn, version } = await params;
  const path = packageDocumentPath(fqn);
  if (path === null) notFound();
  const document = await loadDocument(version, path);
  if (!document) return {};
  return documentationMetadata({
    description: document.route_metadata.description,
    index: !isPrereleaseVersion(document.version),
    path: document.route_metadata.publicPath,
    title: document.route_metadata.title,
  });
}

export default async function PackageDocumentPage({
  params,
}: {
  params: Promise<{ fqn?: string[]; version: string }>;
}) {
  const { fqn, version } = await params;
  const path = packageDocumentPath(fqn);
  if (path === null) notFound();
  const document = await loadDocument(version, path);
  if (!document || document.route_metadata.surface !== 'packages') notFound();
  const block = document.content.blocks[0];
  const toc = document.content.headings.map((heading) => ({
    href: `#${heading.id}`,
    label: heading.label,
  }));

  if (block?.type === 'packageIndex') {
    return (
      <DocsShell
        breadcrumbs={[
          { href: '/baml', label: 'BAML' },
          { href: '/baml/packages', label: 'Standard packages' },
          { label: version },
        ]}
        description={document.route_metadata.description}
        headerControls={
          <GeneratedVersionSwitcher
            currentHref={document.route_metadata.publicPath}
            currentRouteVersion={version}
            storedPath={document.path}
          />
        }
        title={`${document.content.title} ${version}`}
        toc={toc}
      >
        <h2 id="packages">Packages</h2>
        <ul>
          {block.packages.map((page) => (
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
          <dd>{document.route_metadata.releasedAt}</dd>
          <dt>Source commit</dt>
          <dd>
            <code>{document.route_metadata.sourceRevision}</code>
          </dd>
          <dt>Generator revision</dt>
          <dd>
            <code>{document.route_metadata.generatorVersion}</code>
          </dd>
        </dl>
      </DocsShell>
    );
  }

  if (block?.type !== 'bamlReference') notFound();
  const qualifiedParts = block.page.qualified_name.split('.');
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { href: '/baml/packages', label: 'Standard packages' },
        { href: `/baml/packages/${version}`, label: version },
        ...qualifiedParts.map((part, index) => ({
          href:
            index === qualifiedParts.length - 1
              ? undefined
              : `/baml/packages/${version}/${qualifiedParts.slice(0, index + 1).join('/')}`,
          label: part,
        })),
      ]}
      description={document.route_metadata.description}
      headerControls={
        <GeneratedVersionSwitcher
          currentHref={document.route_metadata.publicPath}
          currentRouteVersion={version}
          storedPath={document.path}
        />
      }
      title={document.content.title}
      toc={referencePageTableOfContents(block.page, block.namespacedChildren)}
      wideContent
    >
      <GeneratedReferenceContent
        namespacedChildren={block.namespacedChildren}
        page={block.page}
        routeVersion={version}
      />
    </DocsShell>
  );
}
