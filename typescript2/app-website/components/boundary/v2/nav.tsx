'use client';

import logoBlack from '@boundaryml/brand/logo/svg/boundary-primary-black.svg';
import logoWhite from '@boundaryml/brand/logo/svg/boundary-primary-white.svg';
import Image from 'next/image';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { type MouseEvent, useEffect, useState } from 'react';
import { cta } from '../analytics';
import { BOOK_MEETING } from '../page-parts';

// Part of the /v2 homepage: the in-page section and the logo stay on /v2. Customers is
// the case studies index, Docs is the docs site, and Blog is app/(home)/blog. Routes go through
// next/link; hash links and external links stay plain anchors.
const LINKS: [label: string, href: string][] = [
  ['Product', '/v2#product'],
  ['Customers', '/case-studies'],
  ['Docs', 'https://docs.boundaryml.com'],
  ['Blog', '/blog'],
];
// BAML's own pages (app/(baml)).
const BAML = '/what-is-baml';
const isRoute = (href: string) =>
  !href.includes('#') && !href.startsWith('http');
// PostHog CTA name for a nav item (see ./analytics.ts): "Book a meeting" -> "book_a_meeting".
const ctaName = (label: string) =>
  label.replace(' ↗', '').toLowerCase().replace(/ /g, '_');
// A route item is current on its own page and on anything under it, so a post keeps Blog marked.
// Nothing matches on "/", so the homepage looks exactly as it did.
const isCurrent = (href: string, path: string) =>
  isRoute(href) && (path === href || path.startsWith(`${href}/`));

export default function Nav() {
  const path = usePathname();
  // On the homepage the logo scrolls back to the top instead of re-navigating (smooth, or instant
  // for reduced motion) and drops any #section from the URL. Elsewhere it's a normal link to "/".
  const toTopOnHome = (e: MouseEvent<HTMLAnchorElement>) => {
    if (
      path !== '/v2' ||
      e.metaKey ||
      e.ctrlKey ||
      e.shiftKey ||
      e.altKey ||
      e.button !== 0
    )
      return;
    e.preventDefault();
    setOpen(false);
    const reduce = window.matchMedia(
      '(prefers-reduced-motion: reduce)',
    ).matches;
    window.scrollTo({ behavior: reduce ? 'instant' : 'smooth', top: 0 });
    if (window.location.hash)
      window.history.replaceState(window.history.state, '', '/v2');
  };
  // Once the page moves, the nav gets a hairline and a stronger blur so it separates from content.
  const [scrolled, setScrolled] = useState(false);
  // Phones and tablets get a menu button; the sheet closes when a link is tapped or on Escape.
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open]);
  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 24);
    onScroll();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  }, []);

  return (
    <nav
      // Named so page transitions leave it in place while the content under it changes (boundary.css).
      className={`sticky top-0 z-30 border-b transition-[background-color,border-color,backdrop-filter] duration-300 ${
        open
          ? 'bg-black border-white/10'
          : scrolled
            ? 'bg-black/70 backdrop-blur-xl border-white/10'
            : 'bg-black/40 backdrop-blur-md border-transparent'
      }`}
    >
      <div className="max-w-[1440px] mx-auto flex items-center justify-between h-18 px-5 md:px-12">
        <Link
          aria-label="Boundary home"
          className="shrink-0"
          href="/v2"
          onClick={toTopOnHome}
        >
          <Image
            alt="Boundary"
            className="w-[124px] sm:w-[146px] h-auto dark:hidden"
            height={25}
            priority
            src={logoBlack}
            width={146}
          />
          <Image
            alt="Boundary"
            className="w-[124px] sm:w-[146px] h-auto hidden dark:block"
            height={25}
            priority
            src={logoWhite}
            width={146}
          />
        </Link>
        <ul className="hidden lg:flex gap-8 text-sm text-ink-2">
          {LINKS.map(([label, href]) => {
            const here = isCurrent(href, path);
            return (
              <li key={label}>
                {isRoute(href) ? (
                  <Link
                    href={href}
                    {...cta(ctaName(label), 'nav')}
                    aria-current={here ? 'page' : undefined}
                    className={`transition-colors ${here ? 'text-ink' : 'hover:text-ink'}`}
                  >
                    {label}
                  </Link>
                ) : (
                  <a
                    href={href}
                    {...cta(ctaName(label), 'nav')}
                    className="hover:text-ink transition-colors"
                  >
                    {label}
                  </a>
                )}
              </li>
            );
          })}
        </ul>
        <div className="flex items-center gap-3 sm:gap-5.5 text-sm text-ink-2">
          <a
            href={BAML}
            {...cta('baml', 'nav')}
            className="group hidden sm:inline hover:text-ink transition-colors"
          >
            BAML <Arrow />
          </a>
          <a
            href={BOOK_MEETING}
            {...cta('book_meeting', 'nav')}
            className="focus-ring py-2.5 px-4 whitespace-nowrap rounded-full bg-white text-black font-medium hover:bg-pasture-100 active:translate-y-px transition"
          >
            {/* Phones have room for the logo and one word beside the menu button. */}
            <span className="sm:hidden">Book</span>
            <span className="max-sm:hidden">Book a meeting</span>
          </a>
          <button
            aria-controls="mobile-menu"
            aria-expanded={open}
            aria-label={open ? 'Close menu' : 'Open menu'}
            className="lg:hidden -mr-2 w-10 h-10 grid place-items-center text-ink"
            onClick={() => setOpen((o) => !o)}
            type="button"
          >
            <svg
              aria-hidden="true"
              fill="none"
              height="20"
              stroke="currentColor"
              strokeWidth="1.6"
              viewBox="0 0 20 20"
              width="20"
            >
              <path
                className="transition-transform duration-200 origin-center"
                d="M3 6.5h14"
                style={{
                  transform: open ? 'translateY(3.5px) rotate(45deg)' : 'none',
                }}
              />
              <path
                className="transition-transform duration-200 origin-center"
                d="M3 13.5h14"
                style={{
                  transform: open
                    ? 'translateY(-3.5px) rotate(-45deg)'
                    : 'none',
                }}
              />
            </svg>
          </button>
        </div>
      </div>
      <div
        className={`lg:hidden grid transition-[grid-template-rows] duration-300 ease-out ${open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}`}
        id="mobile-menu"
      >
        <div className="overflow-hidden">
          <ul className="flex flex-col px-5 md:px-12 pt-2 pb-6">
            {[...LINKS, ['BAML ↗', BAML] as const].map(([label, href]) => {
              const cls =
                'flex py-3.5 font-display text-[22px] leading-7 tracking-[-0.03em] text-ink';
              return (
                <li className="border-b border-hair last:border-0" key={label}>
                  {isRoute(href) ? (
                    <Link
                      href={href}
                      {...cta(ctaName(label), 'mobile_menu')}
                      aria-current={isCurrent(href, path) ? 'page' : undefined}
                      className={cls}
                      onClick={() => setOpen(false)}
                      tabIndex={open ? 0 : -1}
                    >
                      {label}
                    </Link>
                  ) : (
                    <a
                      href={href}
                      {...cta(ctaName(label), 'mobile_menu')}
                      className={cls}
                      onClick={() => setOpen(false)}
                      tabIndex={open ? 0 : -1}
                    >
                      {label}
                    </a>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      </div>
    </nav>
  );
}

/** ↗ that nudges up and right when its link (a `group`) is hovered. */
export function Arrow() {
  return (
    <span className="inline-block transition-transform duration-200 ease-out group-hover:-translate-y-0.5 group-hover:translate-x-0.5">
      ↗
    </span>
  );
}
