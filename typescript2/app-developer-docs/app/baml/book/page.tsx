import { DocsCard } from '@/components/docs-card';
import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Learn BAML in a deliberate, chapter-by-chapter sequence.',
  title: 'The BAML book',
};

export default function BookPage() {
  return (
    <DocsShell
      breadcrumbs={[{ href: '/baml', label: 'BAML' }, { label: 'Book' }]}
      description="A systematic path through BAML, from core language ideas to reliable production workflows."
      title="The BAML book"
      toc={[
        { href: '#about', label: 'About the book' },
        { href: '#contents', label: 'Contents' },
      ]}
    >
      <h2 id="about">About the book</h2>
      <p>
        The book is a first-class part of this portal. Chapters share the same
        navigation, search, syntax highlighting, and checked examples as the
        rest of the BAML documentation.
      </p>
      <h2 id="contents">Contents</h2>
      <p>
        Each part begins with an orientation page and continues into focused
        chapters. Start with foundations to learn how BAML functions establish a
        typed boundary around model calls.
      </p>
      <div className="docs-card-grid">
        <DocsCard
          description="Learn the source-file structure, type declarations, and functions that everything else builds upon."
          href="/baml/book/foundations"
          title="Part I · Foundations"
        />
        <DocsCard
          description="Take the shortest path to a working project before reading sequentially."
          href="/baml/get-started"
          title="Start with the quickstart"
        />
      </div>
    </DocsShell>
  );
}
