import Link from 'next/link';

import { DocsCard } from '@/components/docs-card';
import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'The core ideas that every BAML project builds upon.',
  title: 'Foundations',
};

export default function FoundationsPage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { href: '/baml/book', label: 'Book' },
        { label: 'Foundations' },
      ]}
      description="Part I · Learn how source files, types, functions, and generated clients fit together."
      title="Foundations"
      toc={[
        { href: '#mental-model', label: 'A working mental model' },
        { href: '#chapters', label: 'Chapters' },
        { href: '#reference', label: 'Book or reference?' },
      ]}
    >
      <h2 id="mental-model">A working mental model</h2>
      <p>
        A BAML project defines the contract between application code and an AI
        model. Types describe the data crossing that boundary, functions name
        the operation, and a generated client gives the host application a typed
        way to call it.
      </p>
      <blockquote>
        <p>
          <strong>The source is the contract.</strong> Change the BAML
          declaration, check the project, and regenerate the client before
          relying on the new shape in application code.
        </p>
      </blockquote>
      <h2 id="chapters">Chapters</h2>
      <div className="docs-card-grid">
        <DocsCard
          description="Define typed inputs and outputs, write a body, and understand the generated call surface."
          href="/baml/book/foundations/functions"
          title="1. Functions"
        />
      </div>
      <h2 id="reference">Book or reference?</h2>
      <p>
        Read the book when you want concepts introduced in sequence. Use the{' '}
        <Link href="/baml/language/functions">function reference</Link> when you
        already know the concept and need its exact shape at a glance.
      </p>
    </DocsShell>
  );
}
