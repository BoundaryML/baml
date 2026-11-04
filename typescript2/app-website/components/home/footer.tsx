import boundaryWhite from '@boundaryml/brand/logo/svg/boundary-primary-white.svg';
import Image from 'next/image';
import { section } from './analytics';
import { BOOK } from './links';
import { LANES, LEFT, LINKS, WRAP } from './ui';

// The footer on the homepage and the blog. Its links are full paths, so they work from either page.
const COLUMNS: { title: string; links: [string, string][] }[] = [
  {
    links: [
      ['Get early access', BOOK],
      ['Build', '/#build'],
    ],
    title: 'BAML',
  },
  {
    links: [
      ['Source code', LINKS.github],
      ['Demo projects', LINKS.demos],
      ['Discord', LINKS.discord],
    ],
    title: 'Open source',
  },
  {
    links: [['Blog', '/blog']],
    title: 'Boundary',
  },
];

// The page is always dark, so the footer carries the white Boundary wordmark.
export default function Footer() {
  return (
    <footer {...section('footer')} className="bg-footer text-footer-ink">
      <div className={`${WRAP} pt-20 pb-10 flex flex-col gap-16 lg:gap-[72px]`}>
        <div className={LANES}>
          <div className={`${LEFT} flex flex-col gap-[18px]`}>
            <Image
              alt="Boundary"
              className="self-start h-10 w-auto"
              height={40}
              src={boundaryWhite}
              width={234}
            />
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
            The unified software stack for agents.
          </span>
        </div>
      </div>
    </footer>
  );
}
