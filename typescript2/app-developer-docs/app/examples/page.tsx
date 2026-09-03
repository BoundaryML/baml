import { DocsCard } from '@/components/docs-card';
import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Focused, checked examples for building with BAML.',
  title: 'Examples',
};

export default function ExamplesPage() {
  return (
    <DocsShell
      breadcrumbs={[{ label: 'Examples' }]}
      description="Focused, compatibility-aware BAML examples backed by canonical checked source."
      title="Examples"
      toc={[
        { href: '#model', label: 'Source model' },
        { href: '#explore', label: 'Explore' },
      ]}
    >
      <h2 id="model">Source model</h2>
      <p>
        Executable examples come from canonical files under the portal&apos;s
        content tree. Displayed regions and compilation checks resolve from the
        same source so copied snippets cannot silently drift.
      </p>
      <h2 id="explore">Explore</h2>
      <div className="docs-card-grid">
        <DocsCard
          description="A compact enum-returning classifier with a prompt and test case."
          href="/examples/classify-support-tickets"
          title="Classify support tickets"
        />
        <DocsCard
          description="Learn how BAML concepts transition into host languages."
          href="/baml/bridges"
          title="Language bridges"
        />
      </div>
    </DocsShell>
  );
}
