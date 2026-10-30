'use client';

import bamlBlack from '@boundaryml/brand/logo/svg/baml-primary-black.svg';
import bamlWhite from '@boundaryml/brand/logo/svg/baml-primary-white.svg';
import Image from 'next/image';
import { useEffect, useState } from 'react';
import { section } from '../analytics';
import ThemeToggle from '../theme-toggle';
import { Arrow, LINKS, WRAP } from '../ui';

/* Part of the frozen /v1 homepage: the section links and the logo stay on /v1. */
const PRIMARY = [
  ['Language', '/v1#features-title'],
  ['Toolchain', '/v1#toolchain-title'],
  ['Integrations', '/v1#stack-title'],
  ['Boundary', '/v1#boundary-title'],
  ['Pricing', LINKS.pricing],
] as const;

/* Everything the bar no longer has room for still appears in the phone menu, the Status section and
   the footer. The bar deliberately keeps just one text link next to the filled CTA. */
const EXTRA = [
  ['GitHub', LINKS.github],
  ['Demo projects', LINKS.demos],
  ['Discord', LINKS.discord],
] as const;

export default function Nav() {
  const [open, setOpen] = useState(false);

  // Close the phone menu on Escape, and hold the page still underneath it while it's open.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false);
    window.addEventListener('keydown', onKey);
    const { overflow } = document.body.style;
    document.body.style.overflow = 'hidden';
    return () => {
      window.removeEventListener('keydown', onKey);
      document.body.style.overflow = overflow;
    };
  }, [open]);

  return (
    <header
      {...section('nav')}
      className={`sticky top-0 z-30 border-b border-rule backdrop-blur-md ${open ? 'bg-ground' : 'bg-ground/85'}`}
    >
      <nav
        aria-label="Main"
        className={`${WRAP} h-[72px] flex items-center justify-between`}
      >
        <div className="flex items-center gap-8 xl:gap-11">
          {/* Two wordmarks, one per theme: the mark is solid, so it can't be recoloured in place. */}
          <a aria-label="BAML home" className="tap shrink-0" href="/v1#top">
            <Image
              alt="baml"
              className="dark-only"
              height={30}
              priority
              src={bamlWhite}
              width={78}
            />
            <Image
              alt="baml"
              className="light-only"
              height={30}
              priority
              src={bamlBlack}
              width={78}
            />
          </a>
          <ul className="hidden lg:flex gap-6 xl:gap-7 text-sm text-body">
            {PRIMARY.map(([label, href]) => (
              <li key={label}>
                <a
                  className="whitespace-nowrap hover:text-ink transition-colors"
                  href={href}
                >
                  {label}
                </a>
              </li>
            ))}
          </ul>
        </div>

        <div className="flex items-center gap-5 xl:gap-[22px] text-sm text-body">
          <a
            className="group hidden lg:inline whitespace-nowrap hover:text-ink transition-colors"
            href={LINKS.github}
          >
            GitHub <Arrow />
          </a>
          <ThemeToggle />
          {/* Matches the hero's primary action. */}
          <a
            className="max-lg:hidden whitespace-nowrap px-4 py-2.5 rounded-[7px] bg-accent text-on-accent font-medium hover:bg-accent-hover transition"
            href={LINKS.quickstart}
          >
            Install BAML
          </a>
          <button
            aria-controls="mobile-menu"
            aria-expanded={open}
            aria-label={open ? 'Close menu' : 'Open menu'}
            className="lg:hidden -mr-2.5 size-11 grid place-items-center text-ink"
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
      </nav>

      <div
        className={`lg:hidden grid transition-[grid-template-rows] duration-300 ease-out ${open ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'}`}
        id="mobile-menu"
      >
        <div className="overflow-hidden">
          {/* Eleven rows outgrow a short phone, so the sheet scrolls under the sticky bar. */}
          <ul
            className={`${WRAP} pt-3 pb-6 max-h-[calc(100dvh-72px)] overflow-y-auto overscroll-contain`}
          >
            {/* The bar's filled Install BAML is hidden below lg, so the sheet carries the page's
                one primary action instead: it keeps the accent fill it has in the bar, and is the
                only row here that isn't a line of text. */}
            <li className="pb-4">
              <a
                className="flex items-center justify-center h-12 rounded-lg bg-accent text-on-accent font-medium text-[15px] active:translate-y-px transition"
                href={LINKS.quickstart}
                onClick={() => setOpen(false)}
                tabIndex={open ? 0 : -1}
              >
                Install BAML
              </a>
            </li>
            {PRIMARY.map(([label, href]) => (
              <li className="border-b border-rule" key={label}>
                <a
                  className="flex py-3.5 font-medium text-[22px] leading-7 tracking-[-0.03em] text-head"
                  href={href}
                  onClick={() => setOpen(false)}
                  tabIndex={open ? 0 : -1}
                >
                  {label}
                </a>
              </li>
            ))}
            {/* Where the page's own sections end and the places outside it begin. */}
            {EXTRA.map(([label, href]) => (
              <li className="border-b border-rule last:border-0" key={label}>
                <a
                  className="flex py-3.5 text-[19px] leading-7 tracking-[-0.02em] text-body"
                  href={href}
                  onClick={() => setOpen(false)}
                  tabIndex={open ? 0 : -1}
                >
                  {label}
                </a>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </header>
  );
}
