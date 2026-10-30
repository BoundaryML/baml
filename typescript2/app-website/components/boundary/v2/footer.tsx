import logoWhite from '@boundaryml/brand/logo/svg/boundary-primary-white.svg';
import Image from 'next/image';
import Link from 'next/link';
import { cta } from '../analytics';
import { ParallaxImage } from '../motion';
import { BOOK_MEETING } from '../page-parts';
import { Arrow } from './nav';

// Links without a real destination stay on "#" until Boundary supplies the URL. Blog, Changelog, BAML,
// About and the legal pages are the BAML site's (app/(baml)); /changelog and /jobs are redirects in
// next.config.mjs.
// Customers isn't a column on purpose: a fifth item would change the column's height, and the nav already carries it.
const COLUMNS = [
  [
    'Product',
    [
      ['Profiling', '/v2#product'],
      ['Boards', '/v2#product'],
      ['SQL', '/v2#product'],
    ],
  ],
  [
    'Resources',
    [
      ['Docs', 'https://docs.boundaryml.com'],
      ['Blog', '/blog'],
      ['Changelog', '/changelog'],
      ['BAML ↗', '/what-is-baml'],
    ],
  ],
  [
    'Company',
    [
      ['About', '/who-are-we'],
      ['Careers', '/jobs'],
      ['Contact', '#'],
      ['GitHub', 'https://github.com/boundaryml/baml'],
    ],
  ],
] as const;

const LEGAL = [
  ['Privacy', '/privacy-policy'],
  ['Terms', '/tos'],
  ['Security', '#'],
] as const;
// PostHog CTA name for a footer link (see ./analytics.ts): "BAML ↗" -> "baml".
const ctaName = (label: string) => label.replace(' ↗', '').toLowerCase();

/** Footer on the hand-painted background. */
export default function Footer() {
  return (
    <footer className="relative overflow-hidden bg-pasture">
      <ParallaxImage
        overscan={5}
        quality={90}
        sizes="100vw"
        src="/boundary/painting-footer-4k.jpg"
        strength={0.06}
      />
      {/* Brand word texture: BOUNDARY in rows, letters dropped out to show the lamb, laid over the painting in white */}
      {/* biome-ignore lint/performance/noImgElement: word-texture.svg is an SVG, not raster */}
      <img
        alt=""
        aria-hidden
        className="absolute inset-0 w-full h-full object-cover object-left-top opacity-10 md:opacity-15 pointer-events-none select-none"
        loading="lazy"
        src="/boundary/word-texture.svg"
      />
      <div className="absolute inset-x-0 bottom-0 h-140 bg-gradient-to-b from-[#0E2306]/0 to-[#0E2306]/72" />

      <div className="relative max-w-[1440px] mx-auto flex flex-col gap-14 px-5 md:px-12 pt-24 md:pt-40 pb-10">
        <div className="pb-20 md:pb-40">
          <h2 className="max-w-300 font-display font-medium text-[clamp(44px,8.4vw,132px)] leading-[0.98] tracking-[-0.055em] text-white">
            Let your agents ship faster.
          </h2>
          <p className="mt-6 text-xl leading-[1.5] text-white/90">
            The AI observability platform for shipping faster to production.
          </p>
          <a
            href={BOOK_MEETING}
            {...cta('book_meeting', 'cta_band')}
            className="focus-ring inline-block mt-8 py-3.5 px-6 rounded-full bg-white text-black text-[15px] leading-4.5 font-medium hover:bg-pasture-100 active:translate-y-px transition"
          >
            Book a meeting
          </a>
        </div>
        <div className="flex flex-col lg:flex-row lg:justify-between lg:items-end gap-12">
          <div className="flex flex-col gap-6">
            <Image
              alt="Boundary"
              className="w-[min(584px,80vw)] h-auto"
              height={100}
              src={logoWhite}
              width={584}
            />
            <p className="font-pixel text-base leading-[1.2] tracking-[-0.02em] text-white/85">
              Build with BAML. Grow with Boundary.
            </p>
          </div>
          <div className="flex gap-10 md:gap-16">
            {COLUMNS.map(([title, links]) => (
              <div
                className="flex flex-col gap-3 w-28 md:w-30 text-sm leading-5"
                key={title}
              >
                <span className="text-white/60">{title}</span>
                {links.map(([label, href]) => {
                  const text = label.endsWith(' ↗') ? (
                    <>
                      {label.slice(0, -2)} <Arrow />
                    </>
                  ) : (
                    label
                  );
                  const cls =
                    'group text-white hover:text-pasture-200 transition-colors';
                  // Routes inside the site go through next/link so they don't reload the page.
                  return href.startsWith('/') ? (
                    <Link
                      href={href}
                      key={label}
                      {...cta(ctaName(label), 'footer')}
                      className={cls}
                    >
                      {text}
                    </Link>
                  ) : (
                    <a
                      href={href}
                      key={label}
                      {...cta(ctaName(label), 'footer')}
                      className={cls}
                    >
                      {text}
                    </a>
                  );
                })}
              </div>
            ))}
          </div>
        </div>
        <div className="flex justify-between pt-5 border-t border-white/22 text-[13px] leading-[18px] text-white/70">
          <span>© 2026 Boundary</span>
          <span className="flex gap-6">
            {LEGAL.map(([label, href]) => (
              <a
                href={href}
                key={label}
                {...cta(ctaName(label), 'footer')}
                className="hover:text-white"
              >
                {label}
              </a>
            ))}
          </span>
        </div>
      </div>
    </footer>
  );
}
