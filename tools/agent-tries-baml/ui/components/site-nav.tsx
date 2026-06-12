import Link from 'next/link';

const LINKS: Array<[string, string]> = [
  ['/', 'agents'],
  ['/pipeline', 'pipeline'],
  ['/#runs', 'runs'],
  ['/cohorts', 'arenas'],
  ['/#issues', 'issues'],
  ['/db/tasks', 'db'],
  ['/changelog', 'changelog'],
];

/**
 * Persistent site navigation: one hairline-ruled mono strip at the top of every
 * page, so any view is one click from any other (previously each page only had
 * an ad-hoc back link). Server component — no state, no JS.
 */
export function SiteNav() {
  return (
    <nav
      aria-label="site"
      className="mb-8 flex items-baseline gap-5 overflow-x-auto border-b border-border pb-2.5 font-mono text-[12px] tracking-[0.04em] max-[640px]:mb-5 max-[640px]:gap-4"
    >
      <Link
        href="/"
        className="font-semibold text-foreground no-underline hover:text-link"
      >
        agent-tries-baml
      </Link>
      <span className="flex-1" />
      {LINKS.map(([href, label]) => (
        <Link
          key={href}
          href={href}
          className="whitespace-nowrap text-muted-foreground no-underline hover:text-foreground"
        >
          {label}
        </Link>
      ))}
    </nav>
  );
}
