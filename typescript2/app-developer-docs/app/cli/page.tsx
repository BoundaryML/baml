import { DocsCard } from '@/components/docs-card';
import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Installation and exact-version reference for the BAML CLI.',
  title: 'BAML CLI',
};

export default function CliPage() {
  return (
    <DocsShell
      breadcrumbs={[{ label: 'BAML CLI' }]}
      description="Install, configure, and use the exact BAML toolchain version that matches your project."
      title="BAML CLI"
      toc={[
        { href: '#overview', label: 'Overview' },
        { href: '#reference', label: 'Versioned reference' },
      ]}
    >
      <h2 id="overview">Overview</h2>
      <p>
        The CLI documentation covers installation, public commands,
        configuration, environment variables, and exit behavior. Command nesting
        mirrors the tokens accepted by the executable.
      </p>
      <h2 id="reference">Versioned reference</h2>
      <p>
        Exact command pages are generated from the noncolored help emitted by
        the corresponding compiled wrapper and toolchain binaries. Unknown
        versions remain real 404s rather than silently falling forward.
      </p>
      <div className="docs-card-grid">
        <DocsCard
          description="Create a normal BAML project and check it with the toolchain."
          href="/baml/get-started"
          title="Get started with BAML"
        />
        <DocsCard
          description="Review changes to the language and toolchain over time."
          href="/changelog"
          title="Read the changelog"
        />
      </div>
    </DocsShell>
  );
}
