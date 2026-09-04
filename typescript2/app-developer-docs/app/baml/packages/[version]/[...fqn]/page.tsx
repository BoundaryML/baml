import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { DocsShell } from '@/components/docs-shell';
import {
  GeneratedReferenceContent,
  referencePageTableOfContents,
} from '@/components/generated-reference';
import {
  listGeneratedReleaseRouteVersions,
  loadGeneratedReleaseSnapshot,
} from '@/lib/generated-content/build-content';

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
  return page ?? null;
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ fqn: string[]; version: string }>;
}): Promise<Metadata> {
  const { fqn, version } = await params;
  const page = await loadPage(version, fqn);
  if (!page) return {};
  return {
    description:
      page.page_data.summary ??
      `${page.page_kind} reference for ${page.qualified_name}.`,
    title: `${page.qualified_name} — ${version}`,
  };
}

export default async function ReferencePage({
  params,
}: {
  params: Promise<{ fqn: string[]; version: string }>;
}) {
  const { fqn, version } = await params;
  const page = await loadPage(version, fqn);
  if (!page) notFound();
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
      toc={referencePageTableOfContents(page.page_data)}
    >
      <GeneratedReferenceContent page={page.page_data} routeVersion={version} />
    </DocsShell>
  );
}
