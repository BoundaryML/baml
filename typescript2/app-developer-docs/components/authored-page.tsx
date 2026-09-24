import type { MDXComponents } from 'mdx/types';
import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import {
  BookChapter,
  type BookChapterVersion,
  BookPerspectiveSelector,
} from '@/components/book-chapter';
import { DocsShell } from '@/components/docs-shell';
import { authoredSource, bookVersions } from '@/lib/content/source';
import { documentationMetadata } from '@/lib/metadata';
import { useMDXComponents } from '@/mdx-components';

function pageFromPath(path: string) {
  const slugs = path === '/' ? [] : path.split('/').filter(Boolean);
  return authoredSource.getPage(slugs);
}

export function authoredMetadata(path: string): Metadata {
  const page = pageFromPath(path);
  if (!page) return {};
  return documentationMetadata({
    description: page.data.description,
    path,
    title: page.data.title,
  });
}

export function AuthoredPage({
  components = {},
  path,
}: {
  components?: MDXComponents;
  path: string;
}) {
  const mdxComponents = useMDXComponents(components);
  const page = pageFromPath(path);
  if (!page) notFound();
  if (path === '/baml/book' || path.startsWith('/baml/book/')) {
    const alternatives = bookVersions.filter(
      (alternative) =>
        alternative.chapter === path && alternative.perspective !== 'default',
    );
    const versions: BookChapterVersion[] = [
      { data: page.data, perspective: 'default' as const },
      ...alternatives.map((alternative) => ({
        data: alternative.data,
        perspective: alternative.perspective,
      })),
    ].map(({ data, perspective }) => {
      const Body = data.body;
      return {
        content: (
          <DocsShell
            breadcrumbs={page.data.breadcrumbs}
            description={data.description ?? page.data.description}
            headerControls={<BookPerspectiveSelector />}
            stickyHeaderControls
            title={data.title}
            toc={data.toc.map((item) => ({
              depth: item.depth,
              href: item.url,
              label: item.title,
            }))}
            wideContent={path === '/baml/book/errors'}
          >
            <Body components={mdxComponents} />
          </DocsShell>
        ),
        headings: data.toc.map((item) => decodeURIComponent(item.url.slice(1))),
        perspective,
        sectionKeys: data.sectionKeys ?? {},
      };
    });
    return <BookChapter key={path} versions={versions} />;
  }
  const Content = page.data.body;
  return (
    <DocsShell
      breadcrumbs={page.data.breadcrumbs}
      description={page.data.description}
      title={page.data.title}
      toc={page.data.toc.map((item) => ({
        depth: item.depth,
        href: item.url,
        label: item.title,
      }))}
      wideContent={path === '/baml/book/errors'}
    >
      <Content components={mdxComponents} />
    </DocsShell>
  );
}
