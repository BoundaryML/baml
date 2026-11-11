'use client';

import { ChevronDown } from 'lucide-react';
import Link from 'next/link';
import { useEffect, useRef, useState } from 'react';

// The bar's one menu, laid out like the infra sites' (Temporal, PlanetScale, Vercel): plain text
// links to real pages in labeled columns, no icons or descriptions. Only pages that exist are
// listed; a column or link goes in once its page does.

// The third entry marks a link that's only in the menu on a phone, where the bar has no room for it.
const COLUMNS: {
  links: [label: string, href: string, phoneOnly?: boolean][];
  title: string;
}[] = [
  {
    links: [
      ['Overview', '/'],
      ['Adopt', '/adopt'],
      ['Pricing', '/pricing', true],
    ],
    title: 'Platform',
  },
  {
    links: [
      ['Engineering blog', '/blog'],
      ['Podcast: ai that works', '/podcast'],
      ['Discord', '/discord'],
      ['Company', '/who-are-we'],
    ],
    title: 'Resources',
  },
];

/** "Platform" and its panel. It opens on hover with a mouse and on click or Enter otherwise, and
 *  closes on Escape, a click outside, or the pointer leaving. */
export function PlatformMenu() {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
        button.current?.focus();
      }
    };
    document.addEventListener('pointerdown', outside);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', outside);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div
      className="sm:relative"
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setOpen(false);
      }}
      onPointerEnter={(e) => {
        if (e.pointerType === 'mouse') setOpen(true);
      }}
      onPointerLeave={(e) => {
        if (e.pointerType === 'mouse') setOpen(false);
      }}
      ref={ref}
    >
      <button
        aria-controls="platform-menu"
        aria-expanded={open}
        className={`tap flex cursor-pointer items-center gap-1 whitespace-nowrap transition-colors ${open ? 'text-head' : 'text-body hover:text-ink'}`}
        onClick={() => setOpen((o) => !o)}
        ref={button}
        type="button"
      >
        Platform
        <ChevronDown
          aria-hidden
          className={`size-3.5 transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>
      {/* On a phone the panel spans the screen under the bar; from sm it hangs under the trigger.
          The top padding bridges the gap, so the pointer can cross to the panel without closing it. */}
      <div
        className={`absolute inset-x-4 top-full z-40 pt-1 sm:inset-x-auto sm:-left-3 sm:pt-5 ${open ? '' : 'hidden'}`}
        id="platform-menu"
      >
        <nav
          aria-label="Platform"
          className="flex gap-2 rounded-lg border border-rule bg-[#111113] p-1.5 shadow-2xl shadow-black/60"
        >
          {COLUMNS.map((c, i) => (
            <div
              className={`min-w-0 flex-1 sm:min-w-[180px] ${i ? 'border-l border-rule pl-2' : ''}`}
              key={c.title}
            >
              <p className="px-3 pt-1 pb-1.5 font-mono text-[11px] uppercase tracking-[0.08em] text-muted">
                {c.title}
              </p>
              <ul>
                {c.links.map(([label, href, phoneOnly]) => (
                  <li
                    className={phoneOnly ? 'sm:hidden' : undefined}
                    key={href}
                  >
                    <Link
                      className="focus-ring block whitespace-nowrap rounded-md px-3 py-1.5 text-[14px] text-ink transition-colors hover:bg-white/[0.06] hover:text-head"
                      href={href}
                      onClick={() => setOpen(false)}
                    >
                      {label}
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </nav>
      </div>
    </div>
  );
}
