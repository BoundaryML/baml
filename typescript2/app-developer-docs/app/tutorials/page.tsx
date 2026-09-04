import { DocsCard } from '@/components/docs-card';
import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Goal-oriented guides for building applications with BAML.',
  title: 'Tutorials',
};

export default function TutorialsPage() {
  return (
    <DocsShell
      breadcrumbs={[{ label: 'Tutorials' }]}
      description="Goal-oriented guides for building complete applications with BAML."
      title="Tutorials"
      toc={[
        { href: '#about', label: 'About tutorials' },
        { href: '#start', label: 'Where to start' },
      ]}
    >
      <h2 id="about">About tutorials</h2>
      <p>
        Tutorials can cross BAML, the CLI, and host-language boundaries. Each
        route exists only when the corresponding guide has been authored and its
        compatibility requirements are explicit.
      </p>
      <h2 id="start">Where to start</h2>
      <div className="docs-card-grid">
        <DocsCard
          description="Model a receipt, extract typed data, call the generated client, and harden the boundary."
          href="/tutorials/structured-extraction"
          title="Build a structured extractor"
        />
        <DocsCard
          description="Build the language and toolchain foundation first."
          href="/baml/get-started"
          title="BAML quickstart"
        />
      </div>
    </DocsShell>
  );
}
