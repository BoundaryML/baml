import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'The developing Boundary cloud platform for BAML applications.',
  title: 'Boundary Cloud Services',
};

export default function BcsPage() {
  return (
    <DocsShell
      breadcrumbs={[{ label: 'Boundary Cloud Services' }]}
      description="The future cloud product surface for operating BAML applications and workflows."
      title="Boundary Cloud Services"
      toc={[
        { href: '#status', label: 'Current status' },
        { href: '#scope', label: 'Documentation scope' },
      ]}
    >
      <div className="rounded-xl border bg-muted/40 p-5">
        <p className="m-0 text-sm font-medium text-foreground">
          Detailed documentation is coming soon.
        </p>
        <p className="mt-2">
          The product surface is still being defined, so this portal does not
          invent APIs or workflows ahead of the product.
        </p>
      </div>
      <h2 id="status">Current status</h2>
      <p>
        This page reserves a clear home for Boundary Cloud Services while the
        product and its developer workflows become concrete.
      </p>
      <h2 id="scope">Documentation scope</h2>
      <p>
        Deeper BCS routes intentionally return 404 until real deployment,
        observability, debugging, API, or service-limit documentation exists.
      </p>
    </DocsShell>
  );
}
