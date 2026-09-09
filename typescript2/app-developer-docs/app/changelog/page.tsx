import { ChangelogContent } from '@/components/changelog-content';
import { DocsShell } from '@/components/docs-shell';
import { loadCanonicalChangelog } from '@/lib/changelog/loader';
import { documentationMetadata } from '@/lib/metadata';

const description = 'Canonical BAML language and toolchain release history.';

export const metadata = documentationMetadata({
  description,
  path: '/changelog',
  title: 'Changelog',
});

export default async function ChangelogPage() {
  const changelog = await loadCanonicalChangelog();
  return (
    <DocsShell
      breadcrumbs={[{ label: 'Changelog' }]}
      description={description}
      title="Changelog"
      toc={changelog.entries.map((entry) => ({
        href: `#${entry.id}`,
        label: entry.version,
      }))}
    >
      <p>
        This page renders the repository&apos;s canonical release history
        directly. It is not copied into a second documentation source.
      </p>
      <ChangelogContent
        headingIds={changelog.headingIds}
        markdown={changelog.markdown}
      />
    </DocsShell>
  );
}
