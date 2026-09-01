'use client';

import { useSearchContext } from 'fumadocs-ui/contexts/search';
import { useDocsLayout } from 'fumadocs-ui/layouts/docs';
import { ThemeSwitch } from 'fumadocs-ui/layouts/shared/slots/theme-switch';
import { BookOpen, Menu, Search } from 'lucide-react';
import Link from 'next/link';
import { type ComponentProps, type ReactNode } from 'react';
import { useHeaderGitHub } from './header-github';

const navigation = [
  ['Home', '/'],
  ['BAML', '/baml'],
  ['CLI', '/cli'],
  ['BWS', '/bws'],
  ['Tutorials', '/tutorials'],
  ['Examples', '/examples'],
] as const;

function HeaderContents({ sidebarTrigger }: { sidebarTrigger?: ReactNode }) {
  const { setOpenSearch } = useSearchContext();
  const githubLink = useHeaderGitHub();

  return (
    <>
      <div className="shadcn-header__mobile">
        {sidebarTrigger}
        <Link href="/">Boundary Developer</Link>
      </div>

      <nav className="shadcn-header__nav" aria-label="Primary navigation">
        {navigation.map(([label, href]) => (
          <Link key={href} href={href}>{label}</Link>
        ))}
      </nav>

      <div className="shadcn-header__actions">
        <button
          className="shadcn-header__search"
          type="button"
          onClick={() => setOpenSearch(true)}
        >
          <Search aria-hidden="true" />
          <span>Search documentation...</span>
          <kbd>⌘ K</kbd>
        </button>
        <span className="shadcn-header__separator" aria-hidden="true" />
        {githubLink}
        <span className="shadcn-header__separator" aria-hidden="true" />
        <ThemeSwitch className="shadcn-header__theme" />
        <span className="shadcn-header__separator shadcn-header__book-separator" aria-hidden="true" />
        <Link className="shadcn-header__book" href="/baml/book">
          <BookOpen aria-hidden="true" /> Book
        </Link>
      </div>
    </>
  );
}

export function SiteHeader(props: ComponentProps<'header'>) {
  return (
    <header {...props} className={`shadcn-header ${props.className ?? ''}`}>
      <HeaderContents />
    </header>
  );
}

export function DocsSiteHeader(props: ComponentProps<'header'>) {
  const { slots } = useDocsLayout();
  const SidebarTrigger = slots.sidebar.trigger;

  return (
    <header id="nd-subnav" {...props} className={`shadcn-header ${props.className ?? ''}`}>
      <HeaderContents
        sidebarTrigger={
          <SidebarTrigger className="shadcn-header__menu" aria-label="Open navigation">
            <Menu aria-hidden="true" />
          </SidebarTrigger>
        }
      />
    </header>
  );
}
