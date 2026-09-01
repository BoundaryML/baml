import { source } from '@/lib/source';
import { baseOptions } from '@/lib/layout.shared';
import { DocsSiteHeader } from '@/components/site-header';
import { GitHubLink, GitHubLinkFallback } from '@/components/github-link';
import { HeaderGitHubProvider } from '@/components/header-github';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { Suspense } from 'react';

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <HeaderGitHubProvider
      link={(
        <Suspense fallback={<GitHubLinkFallback />}>
          <GitHubLink />
        </Suspense>
      )}
    >
      <DocsLayout
        tree={source.getPageTree()}
        {...baseOptions()}
        containerProps={{ className: 'shadcn-docs-layout' }}
        slots={{ header: DocsSiteHeader }}
      >
        {children}
      </DocsLayout>
    </HeaderGitHubProvider>
  );
}
