import { ChangelogContent } from '@/components/changelog-content';
import { DocsShell } from '@/components/docs-shell';
import { loadCanonicalChangelog } from '@/lib/changelog/loader';
import { documentationMetadata } from '@/lib/metadata';

export const metadata = documentationMetadata({
  description: 'Canonical BAML language and toolchain release history.',
  path: '/changelog',
  title: 'Changelog',
});

export default async function ChangelogPage() {
  const changelog = await loadCanonicalChangelog();
  return (
    <DocsShell
      breadcrumbs={[{ label: 'Changelog' }]}
      description="Canonical release history for the BAML language and toolchain."
      title="Changelog"
      toc={changelog.entries.map((entry) => ({
        href: `#${entry.id}`,
        label: entry.version,
      }))}
    >
      <p>
        This page renders the repository&apos;s canonical{' '}
        <code>baml_language/CHANGELOG.md</code> directly. There is no copied
        changelog or separate releases section.
      </p>
      <ChangelogContent
        headingIds={changelog.entries.map((entry) => entry.id)}
        markdown={changelog.markdown}
      />
    </DocsShell>
  );
}
