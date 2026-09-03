import { DocsCard } from '@/components/docs-card';
import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Compatibility guidance between BAML and host languages.',
  title: 'Language bridges',
};

export default function BridgesPage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { label: 'Language bridges' },
      ]}
      description="Compatibility and type-transition guidance between BAML and application host languages."
      title="Language bridges"
      toc={[
        { href: '#contract', label: 'The bridge contract' },
        { href: '#languages', label: 'Host languages' },
      ]}
    >
      <h2 id="contract">The bridge contract</h2>
      <p>
        Bridge pages focus on compatibility, type mappings, nullability,
        streaming, errors, generated names, and host-language gotchas.
      </p>
      <h2 id="languages">Host languages</h2>
      <p>
        Each supported host language receives one canonical compatibility page.
        Speculative deeper routes are intentionally absent.
      </p>
      <div className="docs-card-grid">
        <DocsCard
          description="Type mappings, generated client calls, streaming, errors, and TypeScript-specific gotchas."
          href="/baml/bridges/typescript"
          title="TypeScript"
        />
      </div>
    </DocsShell>
  );
}
