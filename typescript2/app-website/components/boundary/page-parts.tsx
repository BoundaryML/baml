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

/** Booking link for a call with the team, the site's main call to action. */
export const BOOK_MEETING = 'https://cal.com/boundaryml/30min';

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
 * Closing CTA, shared by the homepage, the blog and the case studies, so every page ends on the same
 * action: a call with the team.
 */
export function CtaBand() {
  return (
    <section className={`${CONTAINER} pt-6 md:pt-10 pb-20 md:pb-30`}>
      <div className="flex flex-col lg:flex-row lg:justify-between lg:items-center gap-8 lg:gap-16 p-8 md:p-14 rounded-[14px] bg-panel">
        <div className="flex flex-col gap-3 lg:w-180">
          <h2 className="h2-display !text-[clamp(28px,2.6vw,38px)]">
            Let your agents build faster.
          </h2>
          <p className="text-[17px] leading-[27px] text-ink-2">
            The AI observability platform for shipping faster to production.
          </p>
        </div>
        <a
          href={BOOK_MEETING}
          {...cta('book_meeting', 'cta_band')}
          className="focus-ring self-start lg:self-center py-3.25 px-5 whitespace-nowrap rounded-md bg-primary text-white text-[15px] leading-4.5 font-medium hover:bg-primary-hover active:translate-y-px transition"
        >
          Book a meeting
        </a>
      </div>
    </section>
  );
}
