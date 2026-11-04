import bamlWhite from '@boundaryml/brand/logo/svg/baml-primary-white.svg';
import Image from 'next/image';
import { section } from '../analytics';
import { LANES, LEFT, LINKS, WRAP } from '../ui';
import { EXPLORE } from './links';

const COLUMNS: { title: string; links: [string, string][] }[] = [
  {
    links: [
      ['Quickstart', LINKS.quickstart],
      ['Explore BAML', EXPLORE],
    ],
    title: 'Language',
  },
  {
    links: [
      ['Source code', LINKS.github],
      ['Demo projects', LINKS.demos],
      ['Discord', LINKS.discord],
    ],
    title: 'Open source',
  },
  { links: [['boundaryml.com ↗', LINKS.boundary]], title: 'Boundary' },
];

// Dark in both themes — deep aubergine under the light ground — so the white wordmark carries over.
export default function Footer() {
  return (
    <footer {...section('footer')} className="bg-footer text-footer-ink">
      <div className={`${WRAP} pt-20 pb-10 flex flex-col gap-16 lg:gap-[72px]`}>
        <div className={LANES}>
          <div className={`${LEFT} flex flex-col gap-[18px]`}>
            <Image alt="baml" height={60} src={bamlWhite} width={156} />
            <p className="text-sm text-footer-muted">
              The programming language for agents.
            </p>
          </div>
          {/* Three nowrap columns at 72px gaps need more room than the 528px copy lane leaves
              them at 1024, and the overflow put a horizontal scrollbar on the whole page. They go
              in a row from xl, where there is width for it. */}
          <nav
            aria-label="Footer"
            className="grid grid-cols-2 xl:flex gap-x-10 xl:gap-x-[72px] gap-y-10 text-sm"
          >
            {COLUMNS.map((c) => (
              <div
                className="tap-list flex flex-col gap-3 min-w-[120px] whitespace-nowrap"
                key={c.title}
              >
                <p className="text-footer-muted">{c.title}</p>
                {c.links.map(([label, href]) => (
                  <a
                    className="tap-row hover:text-footer-accent transition-colors"
                    href={href}
                    key={label}
                  >
                    {label}
                  </a>
                ))}
              </div>
            ))}
          </nav>
        </div>
        <div className="flex flex-col sm:flex-row justify-between gap-3 pt-[22px] border-t border-footer-rule text-[13px] text-footer-muted">
          <span>© 2026 Boundary ML</span>
          <span className="text-[15px] font-medium leading-[18px] tracking-[-0.01em] text-footer-accent">
            Build with BAML. Grow with Boundary.
          </span>
        </div>
      </div>
    </footer>
  );
}
