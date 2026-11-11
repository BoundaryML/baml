import { section } from './analytics';
import { BoundaryLogo } from './boundary-logo';
import { BOOK } from './links';
import { LANES, LEFT, LINKS, WRAP } from './ui';

// The footer on every page. Its columns follow the nav's Platform menu, plus the community links.
// Its links are full paths, so they work from any page.
const COLUMNS: { title: string; links: [string, string][] }[] = [
  {
    links: [
      ['Overview', '/'],
      ['Adopt', '/adopt'],
      ['How BAML works', '/how-baml-works'],
      ['Pricing', '/pricing'],
      ['Get early access', BOOK],
    ],
    title: 'Platform',
  },
  {
    links: [
      ['Engineering blog', '/blog'],
      ['Podcast: ai that works', '/podcast'],
      ['Company', '/who-are-we'],
    ],
    title: 'Resources',
  },
  {
    links: [
      ['GitHub', LINKS.github],
      ['Discord', LINKS.discord],
      ['YouTube', LINKS.youtube],
    ],
    title: 'Community',
  },
];

/** The site footer: link columns and the Boundary wordmark. The page is always dark, so the footer
 *  carries the white Boundary wordmark. */
export default function Footer() {
  return (
    <footer {...section('footer')} className="bg-footer text-footer-ink">
      <div className={`${WRAP} pt-20 pb-10 flex flex-col gap-16 lg:gap-[72px]`}>
        <div className={LANES}>
          <div className={`${LEFT} flex flex-col gap-[18px]`}>
            <BoundaryLogo className="self-start h-[62px] w-auto text-white" />
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
                    // External links open in a new tab, so the reader keeps the site.
                    {...(href.startsWith('http')
                      ? { rel: 'noopener', target: '_blank' }
                      : {})}
                  >
                    {label}
                  </a>
                ))}
              </div>
            ))}
          </nav>
        </div>
        <div className="flex flex-col sm:flex-row justify-between gap-3 pt-[22px] border-t border-footer-rule text-[13px] text-footer-muted">
          <span>© 2026 Boundary</span>
          <span className="text-[15px] font-medium leading-[18px] tracking-[-0.01em] text-footer-accent">
            The unified software stack for agents.
          </span>
        </div>
      </div>
    </footer>
  );
}
