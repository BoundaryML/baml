import { loader } from 'fumadocs-core/source';
import { toFumadocsSource } from 'fumadocs-mdx/runtime/server';
import { book, docs } from '@/.source/server';
import { bookFileIdentity, validateBookLayout } from './book-layout';
import { authoredPageSchema } from './book-perspective-schema';
import { validateBookVersions } from './book-perspectives';

validateBookLayout(book.docs.map((data) => data.info.path));

export const bookVersions = book.docs.map((data) => ({
  ...bookFileIdentity(data.info.path),
  data,
}));

validateBookVersions(
  bookVersions.map((version) => ({
    chapter: version.chapter,
    headings: version.data.toc.map((heading) =>
      decodeURIComponent(heading.url.slice(1)),
    ),
    perspective: version.perspective,
    sectionKeys: version.data.sectionKeys,
  })),
);

// Expose only the canonical chapter to routing, metadata, and navigation.
// Perspective filenames never become public URLs.
export const authoredSource = loader({
  baseUrl: '/',
  source: toFumadocsSource(
    [
      ...docs.docs,
      ...bookVersions
        .filter((version) => version.perspective === 'default')
        .map((version) => ({
          ...version.data,
          ...authoredPageSchema
            .pick({ breadcrumbs: true, description: true })
            .parse({
              breadcrumbs: version.data.breadcrumbs,
              description: version.data.description,
            }),
          info: {
            ...version.data.info,
            path:
              version.chapter === '/baml/book'
                ? 'baml/book/index.mdx'
                : `${version.chapter.slice(1)}.mdx`,
          },
        })),
    ],
    docs.meta,
  ),
});
