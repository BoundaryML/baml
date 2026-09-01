import { getMDXComponents } from '@/components/mdx';
import { VersionSwitcher } from '@/components/version-switcher';
import docsVersions from '@/generated/docs-versions.json';
import { source } from '@/lib/source';
import { createDocsPrerenderPredicate } from '@/lib/static-generation.mjs';
import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
} from 'fumadocs-ui/layouts/docs/page';
import { createRelativeLink } from 'fumadocs-ui/mdx';

const versionCatalog = docsVersions as {
  defaultVersion: string | null;
  versions: Array<{ version: string; channel: string }>;
};

const versionedRoots = [
  ['baml', 'language', 'reference'],
  ['cli', 'commands'],
] as const;

const shouldPreRenderDocsSlug = createDocsPrerenderPredicate(
  versionCatalog.versions.map(({ version }) => version),
);

function versionSwitcher(slug: string[]) {
  const root = versionedRoots.find((candidate) => candidate.every((segment, index) => slug[index] === segment));
  if (!root || !versionCatalog.defaultVersion || versionCatalog.versions.length === 0) return null;

  const tail = slug.slice(root.length);
  const explicitVersion = tail[0]?.match(/^v(.+)$/)?.[1];
  const currentVersion = versionCatalog.versions.some((entry) => entry.version === explicitVersion)
    ? explicitVersion as string
    : versionCatalog.defaultVersion;
  const relativePath = explicitVersion ? tail.slice(1) : tail;
  const routes = Object.fromEntries(versionCatalog.versions.map((entry) => {
    const exactSlug = [...root, `v${entry.version}`, ...relativePath];
    const target = source.getPage(exactSlug) ? exactSlug : [...root, `v${entry.version}`];
    return [entry.version, `/${target.join('/')}`];
  }));

  return (
    <VersionSwitcher
      currentVersion={currentVersion}
      routes={routes}
      versions={versionCatalog.versions}
    />
  );
}

export default async function Page(props: PageProps<'/[...slug]'>) {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  const MDX = page.data.body;

  return (
    <DocsPage toc={page.data.toc} full={page.data.full} className="shadcn-docs-page">
      {versionSwitcher(params.slug)}
      <DocsTitle>{page.data.title}</DocsTitle>
      <DocsDescription>{page.data.description}</DocsDescription>
      <DocsBody>
        <MDX
          components={getMDXComponents({
            a: createRelativeLink(source, page),
          })}
        />
      </DocsBody>
    </DocsPage>
  );
}

export function generateStaticParams() {
  return source.generateParams().filter(({ slug }) => shouldPreRenderDocsSlug(slug));
}

export const dynamicParams = true;

export async function generateMetadata(
  props: PageProps<'/[...slug]'>,
): Promise<Metadata> {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  return {
    title: page.data.title,
    description: page.data.description,
  };
}
