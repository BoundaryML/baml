import { DocsShell } from '@/components/docs-shell';

export const metadata = {
  description: 'Versioned reference for BAML standard packages.',
  title: 'Standard packages',
};

const packages = [
  'baml',
  'reflect',
  'boundary',
  'testing',
  'assert',
  'log',
  'ai',
  'openai',
  'anthropic',
  'google',
  'aws',
  'vercel',
  'claude_code',
];

export default function PackagesPage() {
  return (
    <DocsShell
      breadcrumbs={[
        { href: '/baml', label: 'BAML' },
        { label: 'Standard packages' },
      ]}
      description="Versioned package reference generated from the exact compiled BAML toolchain."
      title="Standard packages"
      toc={[
        { href: '#published', label: 'Published versions' },
        { href: '#packages', label: 'Package catalog' },
      ]}
    >
      <h2 id="published">Published versions</h2>
      <p>
        Exact-version pages will appear after their complete immutable package
        and CLI records are published. Canary and nightly snapshots will be
        labeled explicitly and will never be presented as stable.
      </p>
      <h2 id="packages">Package catalog</h2>
      <p>The initial publication allowlist contains:</p>
      <ul className="columns-2 sm:columns-3">
        {packages.map((packageName) => (
          <li key={packageName}>
            <code>{packageName}</code>
          </li>
        ))}
      </ul>
    </DocsShell>
  );
}
