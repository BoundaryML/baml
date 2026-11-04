import boundaryMark from '@boundaryml/brand/logo/svg/boundary-logomark-white.svg';
import boundaryWhite from '@boundaryml/brand/logo/svg/boundary-primary-white.svg';
import Image from 'next/image';
import Link from 'next/link';
import { section } from './analytics';
import { BOOK } from './links';
import { WRAP } from './ui';

/* The bar on the homepage and the blog: the Boundary logo, one link to the blog, and the booking
   button. Both pages are dark only, so the logos are the white ones. On a phone the full logo leaves no
   room beside the link and the button, so the bar shows the gate alone, as the brand allows where the
   wordmark won't fit. Both logos stay at the brand's 32px minimum height. */
export default function Nav({ current }: { current?: 'blog' }) {
  return (
    <header
      {...section('nav')}
      className="sticky top-0 z-30 border-b border-rule bg-ground/85 backdrop-blur-md"
    >
      <nav
        aria-label="Main"
        className={`${WRAP} h-[72px] flex items-center justify-between gap-6`}
      >
        <Link aria-label="Boundary home" className="tap shrink-0" href="/">
          <Image
            alt="Boundary"
            className="hidden sm:block h-8 w-auto"
            height={32}
            priority
            src={boundaryWhite}
            width={187}
          />
          <Image
            alt="Boundary"
            className="sm:hidden h-8 w-auto"
            height={32}
            priority
            src={boundaryMark}
            width={51}
          />
        </Link>

        <div className="flex items-center gap-6 text-sm">
          <Link
            aria-current={current === 'blog' ? 'page' : undefined}
            className={`tap whitespace-nowrap transition-colors ${current === 'blog' ? 'text-head' : 'text-body hover:text-ink'}`}
            href="/blog"
          >
            Blog
          </Link>
          <a
            className="whitespace-nowrap px-4 py-2.5 rounded-[7px] bg-accent text-on-accent font-medium hover:bg-accent-hover transition"
            href={BOOK}
          >
            Get early access
          </a>
        </div>
      </nav>
    </header>
  );
}
