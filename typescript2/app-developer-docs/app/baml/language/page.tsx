import { DocsCard } from '@/components/docs-card';
import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Reference for the current supported BAML language.',
  title: 'Language reference',
};

export default function LanguagePage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { label: 'Language reference' },
      ]}
      description="Current BAML syntax, types, declarations, expressions, attributes, and diagnostics."
      title="Language reference"
      toc={[
        { href: '#scope', label: 'Reference scope' },
        { href: '#taxonomy', label: 'Content taxonomy' },
      ]}
    >
      <h2 id="scope">Reference scope</h2>
      <p>
        This section documents the currently supported language. Historical
        differences belong in the changelog and migration material rather than a
        version selector for the whole language guide.
      </p>
      <h2 id="taxonomy">Content taxonomy</h2>
      <p>
        Topics follow BAML&apos;s conceptual model: syntax, types, declarations,
        expressions, control flow, attributes, constraints, and diagnostics.
      </p>
      <div className="docs-card-grid">
        <DocsCard
          description="Declare typed inputs and outputs, then choose an expression or model-backed body."
          href="/baml/language/functions"
          title="Functions"
        />
        <DocsCard
          description="Learn the same concepts in a deliberate, chapter-by-chapter order."
          href="/baml/book/foundations"
          title="Book: Foundations"
        />
      </div>
    </DocsShell>
  );
}
