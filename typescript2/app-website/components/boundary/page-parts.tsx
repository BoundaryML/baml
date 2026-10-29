import Link from 'next/link';
import { cta } from './analytics';

/**
 * openGraph fields that every page shares, the share image among them. A page's `openGraph` object
 * replaces the root layout's rather than merging into it, so these have to be spread back in wherever
 * a page sets its own. The image is a plain public file rather than an `opengraph-image.jpg` beside
 * the layout, because inside a route group Next serves that under a hashed name these can't point
 * at. The URL resolves against metadataBase.
 */
export const OG_SITE = {
  images: [
    {
      alt: 'Boundary',
      height: 630,
      type: 'image/jpeg',
      url: '/boundary/share.jpg',
      width: 1200,
    },
  ],
  locale: 'en_GB',
  siteName: 'Boundary',
};

/** The page container used everywhere on the site. */
export const CONTAINER = 'w-full max-w-[1440px] mx-auto px-5 md:px-12';

/**
 * Back link at the top of an article, in the breadcrumb slot. One level only: these pages are
 * never more than two deep, so a full crumb trail would be three words of furniture.
 */
export function BackLink({ href, label }: { href: string; label: string }) {
  return (
    <nav aria-label="Breadcrumb" className="pt-8 md:pt-12">
      <Link
        className="focus-ring group inline-flex items-center gap-2 text-sm text-ink-2 hover:text-ink transition-colors"
        href={href}
      >
        <span
          aria-hidden
          className="transition-transform duration-200 ease-out group-hover:-translate-x-0.5"
        >
          ←
        </span>
        {label}
      </Link>
    </nav>
  );
}

/**
 * Closing CTA. Same panel, same words and the same two buttons as the homepage, so the end of an
 * article lands where the end of the homepage does. "Start free" goes to the pricing section, which
 * is where the homepage's own "Start free" lives; swap in the real app URL when it exists (the
 * homepage's is still on "#" too). "Book a demo" is the same booking link as the homepage's.
 */
export function CtaBand() {
  return (
    <section className={`${CONTAINER} pt-6 md:pt-10 pb-20 md:pb-30`}>
      <div className="flex flex-col lg:flex-row lg:justify-between lg:items-center gap-8 lg:gap-16 p-8 md:p-14 rounded-[14px] bg-panel">
        <h2 className="h2-display !text-[clamp(28px,2.6vw,38px)] lg:w-140">
          Start free. Grow with Boundary.
        </h2>
        <div className="flex gap-2.5 text-[15px] leading-4.5 font-medium">
          <Link
            href="/#pricing"
            {...cta('start_free', 'cta_band')}
            className="focus-ring py-3.25 px-5 whitespace-nowrap rounded-md bg-primary text-white hover:bg-primary-hover active:translate-y-px transition"
          >
            Start free
          </Link>
          <a
            href="https://cal.com/boundaryml/30min"
            {...cta('book_demo', 'cta_band')}
            className="focus-ring py-3.25 px-5 whitespace-nowrap rounded-md border border-line text-ink hover:border-ink-3 active:translate-y-px transition"
          >
            Book a demo
          </a>
        </div>
      </div>
    </section>
  );
}
