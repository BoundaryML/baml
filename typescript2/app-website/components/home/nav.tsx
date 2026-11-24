import Link from 'next/link';
import type { ReactNode } from 'react';
import { getEapEvents } from '@/lib/luma';
import { section } from './analytics';
import { BoundaryLogo, BoundaryMark } from './boundary-logo';
import { EarlyAccessBanner } from './early-access-banner';
import { PlatformMenu } from './nav-menu';
import { Socials } from './nav-socials';
import { WRAP } from './ui';

/** The early access banner and the nav bar. The two stay pinned together by default. The homepage
 *  passes `pinBanner={false}`: its banner scrolls away with the page and only the bar stays pinned,
 *  which leaves more of a long page's window for reading. */
export default async function Nav({
  pinBanner = true,
}: {
  current?: 'blog';
  pinBanner?: boolean;
}) {
  const sessions = (await getEapEvents()).map(
    ({ id, start_at, end_at, timezone }) => ({
      end_at,
      id,
      start_at,
      timezone,
    }),
  );
  const banner = <EarlyAccessBanner sessions={sessions} />;
  return (
    <>
      {/* Outside the pinned bar, the banner keeps the bar's section name, so its clicks are still
          recorded under "nav". */}
      {!pinBanner && <div {...section('nav')}>{banner}</div>}
      <NavBar>{pinBanner && banner}</NavBar>
    </>
  );
}

/** The pinned bar, with anything pinned above it passed as children. */
function NavBar({ children }: { children: ReactNode }) {
  return (
    <header
      {...section('nav')}
      className="sticky top-0 z-30 border-b border-rule bg-ground"
    >
      {children}
      <nav
        aria-label="Main"
        className={`${WRAP} h-12 sm:h-14 flex items-center justify-between gap-3 sm:gap-6`}
      >
        <div className="flex items-center gap-4 sm:gap-6 lg:gap-10 text-sm">
          <Link
            aria-label="Boundary home"
            className="tap shrink-0 text-white"
            href="/"
          >
            <BoundaryLogo className="hidden h-8 w-auto sm:block" />
            <BoundaryMark className="h-6 w-auto sm:hidden" />
          </Link>
          <div className="flex items-center gap-4 sm:gap-6">
            <PlatformMenu />
            {/* Adopt is in the Platform menu. The bar keeps the two links a visitor acts on. */}
            <Link
              className="tap whitespace-nowrap text-body hover:text-ink transition-colors"
              href="/pricing"
            >
              Pricing
            </Link>
            <Link
              className="tap whitespace-nowrap text-body hover:text-ink transition-colors"
              href="/quickstart"
            >
              {/* "BAML" shows where the bar has room for it: not on the narrowest phones, and not
                  between sm and md, where the wordmark and the counts come in beside it. */}
              <span className="hidden min-[360px]:inline sm:hidden md:inline">
                BAML{' '}
              </span>
              Quickstart
            </Link>
          </div>
        </div>

        <div className="hidden min-[400px]:block">
          <Socials
            counts={{ discord: MEMBERS, github: STARS, youtube: SUBSCRIBERS }}
          />
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
