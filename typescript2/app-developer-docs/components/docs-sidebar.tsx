'use client';

import { ChevronRight } from 'lucide-react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { z } from 'zod';

import {
  type DocumentationLink,
  documentationNavigation,
  flattenedDocumentationNavigation,
} from '@/lib/navigation';

const SCROLL_STORAGE_KEY = 'baml-developer-docs-sidebar-scroll';
const OPEN_STORAGE_KEY = 'baml-developer-docs-sidebar-open';
const scrollStateSchema = z
  .object({
    pathname: z.string(),
    scrollTop: z.number().finite().nonnegative(),
  })
  .strict();
const openStateSchema = z.record(z.string(), z.boolean());

function readScrollState() {
  try {
    return scrollStateSchema.parse(
      JSON.parse(sessionStorage.getItem(SCROLL_STORAGE_KEY) ?? ''),
    );
  } catch {
    return null;
  }
}

function saveScrollState(container: HTMLElement) {
  try {
    sessionStorage.setItem(
      SCROLL_STORAGE_KEY,
      JSON.stringify({
        pathname: location.pathname,
        scrollTop: container.scrollTop,
      }),
    );
  } catch {}
}

function readOpenState(storageId: string) {
  try {
    return openStateSchema.parse(
      JSON.parse(
        sessionStorage.getItem(`${OPEN_STORAGE_KEY}:${storageId}`) ?? '{}',
      ),
    );
  } catch {
    return {};
  }
}

function containsPath(link: DocumentationLink, pathname: string): boolean {
  return (
    pathname === link.href ||
    (link.children ?? []).some((child) => containsPath(child, pathname))
  );
}

function NavigationBranch({
  depth,
  link,
  onNavigate,
  openSections,
  setOpenSections,
  storageId,
  variant,
}: {
  depth: number;
  link: DocumentationLink;
  onNavigate?: () => void;
  openSections: Record<string, boolean>;
  setOpenSections: React.Dispatch<
    React.SetStateAction<Record<string, boolean>>
  >;
  storageId: string;
  variant: 'desktop' | 'mobile';
}) {
  const pathname = usePathname();
  const active = pathname === link.href;
  const branchActive = containsPath(link, pathname);
  const hasChildren = Boolean(link.children?.length);
  const open = hasChildren && (openSections[link.href] ?? branchActive);

  const toggle = () => {
    setOpenSections((current) => {
      const next = { ...current, [link.href]: !open };
      try {
        sessionStorage.setItem(
          `${OPEN_STORAGE_KEY}:${storageId}`,
          JSON.stringify(next),
        );
      } catch {}
      return next;
    });
  };

  return (
    <li>
      <div
        className={`group/nav-item flex h-8 items-center rounded-lg text-[0.8rem] transition-colors hover:bg-accent/70 data-[active=true]:bg-accent data-[active=true]:text-accent-foreground ${
          variant === 'mobile' ? 'w-full' : ''
        }`}
        data-active={active}
        data-branch-active={branchActive}
        style={
          variant === 'desktop'
            ? { width: `calc(13.25rem - ${depth * 1.5}rem)` }
            : undefined
        }
      >
        <Link
          aria-current={active ? 'page' : undefined}
          className="docs-focus-ring flex h-full min-w-0 flex-1 items-center rounded-lg px-2 font-medium text-foreground/85 no-underline group-data-[branch-active=true]/nav-item:text-foreground"
          data-docs-tree-link=""
          href={link.href}
          onClick={onNavigate}
        >
          <span className="truncate">{link.label}</span>
        </Link>
        {hasChildren ? (
          <button
            aria-expanded={open}
            aria-label={`${open ? 'Collapse' : 'Expand'} ${link.label}`}
            className="docs-focus-ring mr-0.5 inline-flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-background/70 hover:text-foreground group-data-[branch-active=true]/nav-item:text-[var(--docs-purple)]"
            onClick={toggle}
            type="button"
          >
            <ChevronRight
              aria-hidden="true"
              className={`size-3.5 ${open ? 'rotate-90' : ''}`}
            />
          </button>
        ) : null}
      </div>
      {hasChildren ? (
        <div
          aria-hidden={!open}
          className={`grid ${open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}`}
          inert={!open}
        >
          <div className="overflow-hidden">
            <ul className="relative ml-3.5 flex flex-col gap-0.5 border-l border-border/70 py-0.5 pl-2.5">
              {link.children?.map((child) => (
                <NavigationBranch
                  depth={depth + 1}
                  key={child.href}
                  link={child}
                  onNavigate={onNavigate}
                  openSections={openSections}
                  setOpenSections={setOpenSections}
                  storageId={storageId}
                  variant={variant}
                />
              ))}
            </ul>
          </div>
        </div>
      ) : null}
    </li>
  );
}

export function DocsNavigationTree({
  links,
  onNavigate,
  storageId,
  variant = 'desktop',
}: {
  links: DocumentationLink[];
  onNavigate?: () => void;
  storageId: string;
  variant?: 'desktop' | 'mobile';
}) {
  const [openSections, setOpenSections] = useState<Record<string, boolean>>({});

  useLayoutEffect(() => {
    setOpenSections(readOpenState(storageId));
  }, [storageId]);

  return (
    <ul className="flex flex-col gap-0.5">
      {links.map((link) => (
        <NavigationBranch
          depth={0}
          key={link.href}
          link={link}
          onNavigate={onNavigate}
          openSections={openSections}
          setOpenSections={setOpenSections}
          storageId={storageId}
          variant={variant}
        />
      ))}
    </ul>
  );
}

export function DocsSidebar() {
  const pathname = usePathname();
  const contentRef = useRef<HTMLDivElement>(null);
  const activeHref = flattenedDocumentationNavigation
    .filter(
      (link) => pathname === link.href || pathname.startsWith(`${link.href}/`),
    )
    .sort((left, right) => right.href.length - left.href.length)[0]?.href;

  useLayoutEffect(() => {
    const container = contentRef.current;
    if (!container) return;

    const scrollState = readScrollState();
    if (scrollState?.pathname === pathname) {
      container.scrollTop = scrollState.scrollTop;
    } else {
      const active = container.querySelector<HTMLElement>(
        `[href="${activeHref}"]`,
      );
      active?.scrollIntoView({ block: 'nearest' });
    }
    saveScrollState(container);
  }, [activeHref, pathname]);

  useEffect(() => {
    const container = contentRef.current;
    if (!container) return;
    const onScroll = () => saveScrollState(container);
    container.addEventListener('scroll', onScroll, { passive: true });
    return () => container.removeEventListener('scroll', onScroll);
  }, []);

  return (
    <aside
      aria-label="Documentation navigation"
      className="sticky top-[calc(var(--header-height)+0.6rem)] z-30 hidden h-[calc(100svh-10rem)] overflow-hidden overscroll-none bg-transparent lg:flex"
    >
      <div className="absolute top-12 right-2 bottom-0 hidden h-full w-px bg-[linear-gradient(to_bottom,transparent_0%,var(--border)_10%,var(--border)_90%,transparent_100%)] lg:flex" />
      <div
        className="scroll-fade scrollbar-none w-56 overflow-x-hidden pl-2.5"
        data-docs-sidebar-content=""
        ref={contentRef}
      >
        {documentationNavigation.map((group, groupIndex) => (
          <section
            className={groupIndex === 0 ? 'pt-12' : 'pt-5'}
            key={group.label}
          >
            <h2 className="flex h-8 items-center px-2 text-xs font-medium text-muted-foreground">
              {groupIndex === 0 ? 'Sections' : group.label}
            </h2>
            <DocsNavigationTree links={group.links} storageId={group.label} />
          </section>
        ))}
      </div>
    </aside>
  );
}
