import Link from 'next/link';
import { getEapEvents } from '@/lib/luma';
import { section } from './analytics';
import { BoundaryLogo, BoundaryMark } from './boundary-logo';
import { EarlyAccessBanner } from './early-access-banner';
import { PlatformMenu } from './nav-menu';
import { Socials } from './nav-socials';
import { WRAP } from './ui';

export default async function Nav(_: { current?: 'blog' }) {
  const sessions = (await getEapEvents()).map(
    ({ id, start_at, end_at, timezone }) => ({
      id,
      start_at,
      end_at,
      timezone,
    }),
  );
  return (
    <header
      {...section('nav')}
      className="sticky top-0 z-30 border-b border-rule bg-ground"
    >
      <EarlyAccessBanner sessions={sessions} />
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
            <BoundaryMark className="h-8 w-auto sm:hidden" />
          </Link>
          <div className="flex items-center gap-4 sm:gap-6">
            <PlatformMenu />
            <Link
              className="tap whitespace-nowrap text-body hover:text-ink transition-colors"
              href="/adopt"
            >
              Adopt
            </Link>
            <Link
              className="tap whitespace-nowrap text-body hover:text-ink transition-colors"
              href="/pricing"
            >
              Pricing
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
