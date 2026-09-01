import { SiteHeader } from '@/components/site-header';
import { GitHubLink, GitHubLinkFallback } from '@/components/github-link';
import { HeaderGitHubProvider } from '@/components/header-github';
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
      <div className="flex min-h-screen flex-col">
        <SiteHeader />
        {children}
      </div>
    </HeaderGitHubProvider>
  );
}
