'use client';

import {
  ArrowLeft,
  ArrowRight,
  Check,
  ChevronDown,
  Copy,
  ExternalLink,
  FileText,
} from 'lucide-react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';

import { documentationPages } from '@/lib/navigation';
import { siteConfig } from '@/lib/site-config';

function getPromptUrl(baseUrl: string, pageUrl: string) {
  const prompt = `I'm looking at this BAML documentation: ${pageUrl}.
Help me understand how to use it. Be ready to explain concepts, give examples, or help debug based on it.`;

  return `${baseUrl}?q=${encodeURIComponent(prompt)}`;
}

export function DocsPageActions() {
  const pathname = usePathname();
  const [copied, setCopied] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
  const pageIndex = documentationPages.findIndex(
    (page) => page.href === pathname,
  );
  const previous = pageIndex > 0 ? documentationPages[pageIndex - 1] : null;
  const next =
    pageIndex >= 0 && pageIndex < documentationPages.length - 1
      ? documentationPages[pageIndex + 1]
      : null;
  const pageUrl = new URL(pathname, siteConfig.url).toString();

  const copyPage = async () => {
    const content = document.querySelector<HTMLElement>('[data-docs-content]');
    if (!content) return;
    await navigator.clipboard.writeText(content.innerText);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  useEffect(() => {
    if (!menuOpen) return;
    const closeMenu = (event: MouseEvent) => {
      const menu = menuRef.current;
      if (menu && !event.composedPath().includes(menu)) setMenuOpen(false);
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setMenuOpen(false);
    };
    document.addEventListener('mousedown', closeMenu);
    document.addEventListener('keydown', closeOnEscape);
    return () => {
      document.removeEventListener('mousedown', closeMenu);
      document.removeEventListener('keydown', closeOnEscape);
    };
  }, [menuOpen]);

  return (
    <div className="docs-nav flex items-center gap-2">
      <div
        className="relative hidden items-center rounded-lg bg-secondary sm:flex"
        ref={menuRef}
      >
        <button
          className="docs-focus-ring inline-flex h-8 items-center gap-2 rounded-l-lg bg-secondary px-3 text-sm font-medium text-secondary-foreground shadow-none hover:bg-accent md:h-7 md:text-[0.8rem]"
          onClick={copyPage}
          type="button"
        >
          {copied ? (
            <Check aria-hidden="true" className="size-4" />
          ) : (
            <Copy aria-hidden="true" className="size-4" />
          )}
          {copied ? 'Copied' : 'Copy Page'}
        </button>
        <button
          aria-expanded={menuOpen}
          aria-haspopup="menu"
          aria-label="More page actions"
          className="docs-focus-ring inline-flex size-8 items-center justify-center rounded-r-lg border-l border-foreground/5 bg-secondary text-secondary-foreground hover:bg-accent md:size-7"
          onClick={() => setMenuOpen((current) => !current)}
          type="button"
        >
          <ChevronDown aria-hidden="true" className="size-4" />
        </button>
        {menuOpen ? (
          <div
            className="absolute top-[calc(100%+0.35rem)] right-0 z-50 w-52 rounded-lg border bg-background/90 p-1 text-sm shadow-lg backdrop-blur-sm"
            role="menu"
          >
            <button
              className="docs-focus-ring flex h-9 w-full items-center gap-2 rounded-md px-2 text-left hover:bg-accent"
              onClick={() => {
                void copyPage();
                setMenuOpen(false);
              }}
              role="menuitem"
              type="button"
            >
              <FileText
                aria-hidden="true"
                className="size-4 text-muted-foreground"
              />
              Copy as plain text
            </button>
            <a
              className="docs-focus-ring flex h-9 w-full items-center gap-2 rounded-md px-2 text-left hover:bg-accent"
              href={getPromptUrl('https://chatgpt.com', pageUrl)}
              onClick={() => setMenuOpen(false)}
              rel="noopener noreferrer"
              role="menuitem"
              target="_blank"
            >
              <ExternalLink
                aria-hidden="true"
                className="size-4 text-muted-foreground"
              />
              Open in ChatGPT
            </a>
            <a
              className="docs-focus-ring flex h-9 w-full items-center gap-2 rounded-md px-2 text-left hover:bg-accent"
              href={getPromptUrl('https://claude.ai/new', pageUrl)}
              onClick={() => setMenuOpen(false)}
              rel="noopener noreferrer"
              role="menuitem"
              target="_blank"
            >
              <ExternalLink
                aria-hidden="true"
                className="size-4 text-muted-foreground"
              />
              Open in Claude
            </a>
          </div>
        ) : null}
      </div>
      <div className="ml-auto flex gap-2">
        {previous ? (
          <Link
            aria-label={`Previous: ${previous.label}`}
            className="docs-focus-ring inline-flex size-8 items-center justify-center rounded-lg bg-secondary text-secondary-foreground hover:bg-accent md:size-7"
            href={previous.href}
          >
            <ArrowLeft aria-hidden="true" className="size-4" />
          </Link>
        ) : null}
        {next ? (
          <Link
            aria-label={`Next: ${next.label}`}
            className="docs-focus-ring inline-flex size-8 items-center justify-center rounded-lg bg-secondary text-secondary-foreground hover:bg-accent md:size-7"
            href={next.href}
          >
            <ArrowRight aria-hidden="true" className="size-4" />
          </Link>
        ) : null}
      </div>
    </div>
  );
}

export function DocsPageNavigation() {
  const pathname = usePathname();
  const pageIndex = documentationPages.findIndex(
    (page) => page.href === pathname,
  );
  const previous = pageIndex > 0 ? documentationPages[pageIndex - 1] : null;
  const next =
    pageIndex >= 0 && pageIndex < documentationPages.length - 1
      ? documentationPages[pageIndex + 1]
      : null;

  if (!previous && !next) return null;

  return (
    <nav
      aria-label="Documentation pages"
      className="hidden h-16 w-full items-center gap-2 sm:flex"
    >
      {previous ? (
        <Link
          className="docs-focus-ring inline-flex h-8 items-center gap-2 rounded-md bg-secondary px-3 text-sm font-medium text-secondary-foreground shadow-none hover:bg-accent"
          href={previous.href}
        >
          <ArrowLeft aria-hidden="true" className="size-4" />
          {previous.label}
        </Link>
      ) : (
        <span />
      )}
      {next ? (
        <Link
          className="docs-focus-ring ml-auto inline-flex h-8 items-center gap-2 rounded-md bg-secondary px-3 text-sm font-medium text-secondary-foreground shadow-none hover:bg-accent"
          href={next.href}
        >
          {next.label}
          <ArrowRight aria-hidden="true" className="size-4" />
        </Link>
      ) : null}
    </nav>
  );
}
