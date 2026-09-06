'use client';

import { Check, ChevronDown } from 'lucide-react';
import { usePathname, useRouter } from 'next/navigation';
import { useEffect, useRef, useState } from 'react';

import type { GeneratedVersionOption } from '@/lib/generated-content/discovery';

export function GeneratedVersionSwitcher({
  options,
}: {
  options: GeneratedVersionOption[];
}) {
  const pathname = usePathname();
  const router = useRouter();
  const current = options.find((option) => option.href === pathname);
  const selected = current ?? options[0];
  const [menuOpen, setMenuOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

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

  if (!selected) return null;

  return (
    <div className="inline-flex min-w-0 max-w-full items-center gap-2 text-sm">
      <span className="shrink-0 font-medium text-foreground">
        Reference version
      </span>
      <div className="relative min-w-0" ref={menuRef}>
        <button
          aria-expanded={menuOpen}
          aria-haspopup="menu"
          className="docs-focus-ring inline-flex h-8 max-w-[min(18rem,60vw)] items-center gap-2 rounded-lg bg-secondary px-3 font-mono text-xs font-medium text-secondary-foreground shadow-none hover:bg-accent md:h-7"
          onClick={() => setMenuOpen((open) => !open)}
          type="button"
        >
          <span className="truncate">{selected.routeVersion}</span>
          <ChevronDown aria-hidden="true" className="size-4 shrink-0" />
        </button>
        {menuOpen ? (
          <div
            className="absolute top-[calc(100%+0.35rem)] left-0 z-50 min-w-full rounded-lg border bg-background/90 p-1 text-sm shadow-lg backdrop-blur-sm"
            role="menu"
          >
            {options.map((option) => (
              <button
                aria-checked={option.href === selected.href}
                className="docs-focus-ring flex h-9 w-full items-center justify-between gap-3 rounded-md px-2 text-left font-mono text-xs whitespace-nowrap hover:bg-accent"
                key={option.href}
                onClick={() => {
                  setMenuOpen(false);
                  if (option.href !== pathname) router.push(option.href);
                }}
                role="menuitemradio"
                type="button"
              >
                {option.routeVersion}
                <Check
                  aria-hidden="true"
                  className={
                    option.href === selected.href
                      ? 'size-4'
                      : 'size-4 opacity-0'
                  }
                />
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}
