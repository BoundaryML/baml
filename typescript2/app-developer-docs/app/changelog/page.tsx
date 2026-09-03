import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Canonical BAML language and toolchain release history.',
  title: 'Changelog',
};

export default function ChangelogPage() {
  return (
    <DocsShell
      breadcrumbs={[{ label: 'Changelog' }]}
      description="Canonical release history for the BAML language and toolchain."
      title="Changelog"
      toc={[
        { href: '#canonical-source', label: 'Canonical source' },
        { href: '#rendering', label: 'Portal rendering' },
      ]}
    >
      <h2 id="canonical-source">Canonical source</h2>
      <p>
        Release history is maintained in <code>baml_language/CHANGELOG.md</code>
        . The portal does not maintain a copied changelog or a separate releases
        section.
      </p>
      <h2 id="rendering">Portal rendering</h2>
      <p>
        Direct rendering of canonical release entries is part of the
        architecture-proof content pass. Versioned package and CLI catalogs will
        link back to the matching headings here.
      </p>
    </DocsShell>
  );
}
