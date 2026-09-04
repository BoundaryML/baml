import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { DocsShell } from '@/components/docs-shell';
import {
  GeneratedReferenceContent,
  type ReferenceChildLink,
  referencePageTableOfContents,
} from '@/components/generated-reference';
import { GeneratedVersionSwitcher } from '@/components/generated-version-switcher';
import {
  isPrereleaseVersion,
  listGeneratedReleaseRouteVersions,
  loadGeneratedReleaseSnapshot,
} from '@/lib/generated-content/build-content';
import { listGeneratedVersionOptions } from '@/lib/generated-content/discovery';
import { directRouteChildren } from '@/lib/generated-content/routes';
import { documentationMetadata } from '@/lib/metadata';

export const dynamicParams = false;

export async function generateStaticParams() {
  const versions = await listGeneratedReleaseRouteVersions();
  const params = await Promise.all(
    versions.map(async (version) => {
      const snapshot = await loadGeneratedReleaseSnapshot(version);
      return (snapshot?.pages ?? []).map((page) => ({
        fqn: page.route_path.split('/'),
        version,
      }));
    }),
  );
  return params.flat();
}

async function loadPage(version: string, fqn: readonly string[]) {
  const snapshot = await loadGeneratedReleaseSnapshot(version);
  const routePath = fqn.join('/');
  const page = snapshot?.pages.find(
    (candidate) => candidate.route_path === routePath,
  );
  return page && snapshot ? { page, snapshot } : null;
}

function directNamespacedChildren(
  routePath: string,
  pages: NonNullable<
    Awaited<ReturnType<typeof loadGeneratedReleaseSnapshot>>
  >['pages'],
): ReferenceChildLink[] {
  return directRouteChildren(routePath, pages).map((candidate) => ({
    page_kind: candidate.page_kind,
    qualified_name: candidate.qualified_name,
    route_path: candidate.route_path,
  }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ fqn: string[]; version: string }>;
}): Promise<Metadata> {
  const { fqn, version } = await params;
  const loaded = await loadPage(version, fqn);
  if (!loaded) return {};
  const { page, snapshot } = loaded;
  const description =
    page.page_data.summary ??
    `${page.page_kind} reference for ${page.qualified_name}.`;
  return documentationMetadata({
    description,
    index: !isPrereleaseVersion(snapshot.release.version),
    path: `/baml/packages/${version}/${fqn.join('/')}`,
    title: `${page.qualified_name} — ${version}`,
  });
}

export default async function ReferencePage({
  params,
}: {
  params: Promise<{ fqn: string[]; version: string }>;
}) {
  const { fqn, version } = await params;
  const loaded = await loadPage(version, fqn);
  if (!loaded) notFound();
  const { page, snapshot } = loaded;
  const namespacedChildren =
    page.page_kind === 'package' || page.page_kind === 'namespace'
      ? []
      : directNamespacedChildren(page.route_path, snapshot.pages);
  const versionOptions = await listGeneratedVersionOptions({
    kind: 'package',
    routePath: page.route_path,
  });
  const qualifiedParts = page.qualified_name.split('.');
  const breadcrumbs = [
    { href: '/baml', label: 'BAML' },
    { href: '/baml/packages', label: 'Standard packages' },
    { href: `/baml/packages/${version}`, label: version },
    ...qualifiedParts.map((part, index) => {
      const isLast = index === qualifiedParts.length - 1;
      return {
        href: isLast
          ? undefined
          : `/baml/packages/${version}/${qualifiedParts.slice(0, index + 1).join('/')}`,
        label: part,
      };
    }),
  ];

  return (
    <DocsShell
      breadcrumbs={breadcrumbs}
      description={
        page.page_data.summary ??
        `${page.page_kind} in the ${page.page_data.package_name} package.`
      }
      title={page.qualified_name}
      toc={referencePageTableOfContents(page.page_data, namespacedChildren)}
    >
      <GeneratedVersionSwitcher options={versionOptions} />
      <GeneratedReferenceContent
        namespacedChildren={namespacedChildren}
        page={page.page_data}
        routeVersion={version}
      />
    </DocsShell>
  );
}
