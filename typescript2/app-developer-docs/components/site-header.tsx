import { Plus } from 'lucide-react';
import Link from 'next/link';

import { GitHubLink } from '@/components/github-link';
import { MainNav } from '@/components/main-nav';
import { MobileNav } from '@/components/mobile-nav';
import { SearchMenu } from '@/components/search-menu';
import { ThemeToggle } from '@/components/theme-toggle';

export function SiteHeader() {
  return (
    <header className="sticky top-0 z-50 w-full bg-background">
      <div className="container-wrapper px-6">
        <div className="flex h-[var(--header-height)] items-center">
          <MobileNav />
          <MainNav />
          <div className="ml-auto flex items-center gap-2 md:flex-1 md:justify-end">
            <div className="hidden w-full flex-1 md:flex md:w-auto md:flex-none">
              <SearchMenu />
            </div>
            <span className="ml-2 hidden h-4 w-px bg-border lg:block" />
            <GitHubLink />
            <span className="h-4 w-px bg-border" />
            <ThemeToggle />
            <span className="hidden h-4 w-px bg-border sm:block" />
            <Link
              className="docs-focus-ring hidden h-8 items-center gap-1.5 rounded-lg bg-primary px-3 text-sm font-medium text-primary-foreground sm:inline-flex"
              href="/baml/get-started"
            >
              <Plus aria-hidden="true" className="size-4" />
              Start
            </Link>
          </div>
        </div>
      </div>
    </header>
  );
}
