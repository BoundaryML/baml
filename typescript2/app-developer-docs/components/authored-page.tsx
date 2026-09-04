import type { Metadata } from 'next';
import { notFound } from 'next/navigation';

import { DocsShell } from '@/components/docs-shell';
import { authoredSource } from '@/lib/content/source';
import { useMDXComponents } from '@/mdx-components';

function pageFromPath(path: string) {
  const slugs = path === '/' ? [] : path.split('/').filter(Boolean);
  return authoredSource.getPage(slugs);
}

export function authoredMetadata(path: string): Metadata {
  const page = pageFromPath(path);
  if (!page) return {};
  return {
    description: page.data.description,
    title: page.data.title,
  };
}

export function AuthoredPage({ path }: { path: string }) {
  const page = pageFromPath(path);
  if (!page) notFound();
  const Content = page.data.body;
  return (
    <DocsShell
      breadcrumbs={page.data.breadcrumbs}
      description={page.data.description}
      title={page.data.title}
      toc={page.data.toc.map((item) => ({
        href: item.url,
        label: item.title,
      }))}
    >
      <Content components={useMDXComponents({})} />
    </DocsShell>
  );
}
