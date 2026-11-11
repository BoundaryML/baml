import Link from 'next/link';
import { section } from './analytics';
import { BoundaryLogo, BoundaryMark } from './boundary-logo';
import { BOOK } from './links';
import { PlatformMenu } from './nav-menu';
import { Socials } from './nav-socials';
import { WRAP } from './ui';

/* The bar on the homepage and the blog: the Boundary logo, the Platform menu (nav-menu.tsx), Pricing,
   the GitHub mark with the repository's star count, the Discord and YouTube marks, and the booking
   button. Below sm, Pricing moves into the menu and the YouTube mark drops out. Both pages are dark only, so the logos are the white ones. On a phone the full logo leaves no
   room beside the link and the button, so the bar shows the gate alone, as the brand allows where the
   wordmark won't fit. Both logos stay at the brand's 32px minimum height. The bar is the page's solid
   ground, not a blur: a blurred bar let the page's animated drawings glitch through it while
   scrolling. */
export default function Nav(_: { current?: 'blog' }) {
  return (
    <header
      {...section('nav')}
      className="sticky top-0 z-30 border-b border-rule bg-ground"
    >
      <nav
        aria-label="Main"
        className={`${WRAP} h-[72px] flex items-center justify-between gap-6`}
      >
        {/* The menu and Pricing sit by the logo, like the infra sites' bars; the actions stay right. */}
        <div className="flex items-center gap-4 sm:gap-10 text-sm">
          <Link aria-label="Boundary home" className="tap shrink-0" href="/">
            <BoundaryLogo className="hidden sm:block h-12 w-auto text-white" />
            <span className="sm:hidden text-white">
              <BoundaryMark className="h-10 w-auto" />
              <span className="sr-only">Boundary</span>
            </span>
          </Link>
          <div className="flex items-center gap-6">
            <PlatformMenu />
            <Link
              className="tap hidden sm:block whitespace-nowrap text-body hover:text-ink transition-colors"
              href="/pricing"
            >
              Pricing
            </Link>
          </div>
        </div>

        {/* Below sm the gaps and the button's padding tighten so the bar fits a 360px phone. */}
        <div className="flex items-center gap-3 sm:gap-6 text-sm">
          <Socials
            counts={{ discord: MEMBERS, github: STARS, youtube: SUBSCRIBERS }}
          />
          <a
            className="whitespace-nowrap px-3 sm:px-4 py-2.5 rounded-[7px] bg-accent text-on-accent font-medium hover:bg-accent-hover transition"
            href={BOOK}
          >
            Get early access
          </a>
        </div>
      </nav>
    </header>
  );
}

// The counts beside the marks, rounded. Update them by hand as they grow: the repository's GitHub
// stars, the Discord server's members, and the YouTube channel's subscribers.
export const STARS = '9.5k';
const MEMBERS = '2.7k';
const SUBSCRIBERS = '9.1k';
