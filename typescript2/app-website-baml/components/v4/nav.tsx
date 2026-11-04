'use client';

import boundaryBlack from '@boundaryml/brand/logo/svg/boundary-primary-black.svg';
import boundaryWhite from '@boundaryml/brand/logo/svg/boundary-primary-white.svg';
import Image from 'next/image';
import { useEffect, useState } from 'react';
import { section } from '../analytics';
import { Arrow, LINKS, WRAP } from '../ui';
import { BOOK } from './links';

/* The /v4 nav: five of the page's sections, GitHub, and the booking button. The bar has no install
   button; the page's one action is booking a call. */
const PRIMARY = [
  ['Why BAML', '#why'],
  ['How it works', '#how'],
  ['Build', '#build'],
  ['Observe', '#observe'],
  ['Get started', '#adopt'],
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
          <a aria-label="Boundary home" className="tap shrink-0" href="#top">
            <Image
              alt="Boundary"
              className="dark-only w-[124px] sm:w-[146px] h-auto"
              height={25}
              priority
              src={boundaryWhite}
              width={146}
            />
            <Image
              alt="Boundary"
              className="light-only w-[124px] sm:w-[146px] h-auto"
              height={25}
              priority
              src={boundaryBlack}
              width={146}
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
          <a
            className="max-lg:hidden whitespace-nowrap px-4 py-2.5 rounded-[7px] bg-accent text-on-accent font-medium hover:bg-accent-hover transition"
            href={BOOK}
          >
            Book a call
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
            {/* The bar's Book a call button is hidden below lg, so the sheet carries it instead. */}
            <li className="pb-4">
              <a
                className="flex items-center justify-center h-12 rounded-lg bg-accent text-on-accent font-medium text-[15px] active:translate-y-px transition"
                href={BOOK}
                onClick={() => setOpen(false)}
                tabIndex={open ? 0 : -1}
              >
                Book a call
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
