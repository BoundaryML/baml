import type { CSSProperties, ReactNode } from 'react';

// Placeholder customer logos until Boundary sends the real wall.
const G = 'currentColor'; // marks take the wordmark colour (text-logo), which flips in dark mode
type Logo = { name: string; mark?: ReactNode; style: CSSProperties };

const LOGOS: Logo[] = [
  {
    mark: (
      <svg aria-hidden="true" height="16" viewBox="0 0 18 16" width="18">
        <rect fill={G} height="4" rx="1" width="18" y="2" />
        <rect fill={G} height="4" rx="1" width="11" y="10" />
      </svg>
    ),
    name: 'Ledgerline',
    style: {
      fontFamily: 'var(--font-sora)',
      fontSize: 19,
      fontWeight: 600,
      letterSpacing: '-0.02em',
    },
  },
  {
    mark: (
      <svg aria-hidden="true" height="16" viewBox="0 0 16 16" width="16">
        <rect fill={G} height="7" width="7" />
        <rect fill={G} height="7" width="7" x="9" />
        <rect fill={G} height="7" width="7" y="9" />
        <rect fill={G} height="7" rx="3.5" width="7" x="9" y="9" />
      </svg>
    ),
    name: 'mossgrid',
    style: {
      fontFamily: 'var(--font-space-grotesk)',
      fontSize: 20,
      fontWeight: 700,
      letterSpacing: '-0.03em',
    },
  },
  {
    mark: (
      <svg aria-hidden="true" height="18" viewBox="0 0 16 18" width="16">
        <path d="M8 0 L16 8 L8 18 L0 8 Z" fill={G} />
      </svg>
    ),
    name: 'Paperkite',
    style: {
      fontFamily: 'var(--font-fraunces)',
      fontSize: 21,
      fontWeight: 600,
      letterSpacing: '-0.01em',
    },
  },
  {
    name: 'HALDEN',
    style: {
      fontFamily: 'var(--font-archivo-black)',
      fontSize: 18,
      letterSpacing: '0.12em',
    },
  },
  {
    mark: (
      <svg aria-hidden="true" height="18" viewBox="0 0 18 18" width="18">
        <circle
          cx="9"
          cy="9"
          fill="none"
          r="7.5"
          stroke={G}
          strokeWidth="2.2"
        />
        <circle cx="9" cy="9" fill={G} r="2.6" />
      </svg>
    ),
    name: 'veritrace',
    style: {
      fontFamily: 'var(--font-manrope)',
      fontSize: 20,
      fontWeight: 600,
      letterSpacing: '-0.02em',
    },
  },
  {
    name: 'Brightfold',
    style: {
      fontFamily: 'var(--font-dm-serif)',
      fontSize: 23,
      letterSpacing: '-0.01em',
    },
  },
  {
    mark: (
      <svg aria-hidden="true" height="16" viewBox="0 0 18 16" width="18">
        <path d="M9 0 L18 16 L0 16 Z" fill={G} />
      </svg>
    ),
    name: 'Kestrel Labs',
    style: {
      fontFamily: 'var(--font-plex)',
      fontSize: 18,
      fontWeight: 600,
      letterSpacing: '-0.01em',
    },
  },
  {
    name: 'oakum',
    style: {
      fontFamily: 'var(--font-syne)',
      fontSize: 23,
      fontWeight: 800,
      letterSpacing: '-0.02em',
    },
  },
  {
    name: 'Tallow & Pine',
    style: { fontFamily: 'var(--font-fraunces)', fontSize: 20 },
  },
  {
    mark: (
      <svg aria-hidden="true" height="18" viewBox="0 0 18 18" width="18">
        <path
          d="M16 9 A7 7 0 1 1 9 2"
          fill="none"
          stroke={G}
          strokeLinecap="round"
          strokeWidth="3"
        />
      </svg>
    ),
    name: 'Cindral',
    style: {
      fontFamily: 'var(--font-space-grotesk)',
      fontSize: 20,
      fontWeight: 500,
      letterSpacing: '-0.02em',
    },
  },
  {
    name: 'FERROW',
    style: {
      fontFamily: 'var(--font-sora)',
      fontSize: 17,
      fontWeight: 300,
      letterSpacing: '0.28em',
    },
  },
  {
    mark: (
      <svg aria-hidden="true" height="12" viewBox="0 0 20 12" width="20">
        <path d="M0 12 A10 10 0 0 1 20 12 Z" fill={G} />
      </svg>
    ),
    name: 'Sundial',
    style: {
      fontFamily: 'var(--font-manrope)',
      fontSize: 20,
      fontWeight: 800,
      letterSpacing: '-0.03em',
    },
  },
];

export default function LogoWall() {
  return (
    <section
      className="w-full max-w-[1440px] mx-auto flex flex-col lg:flex-row items-start gap-8 lg:gap-12 px-5 md:px-12 pt-8 md:pt-18 pb-12 md:pb-16"
      id="customers"
    >
      <p className="lg:w-62 shrink-0 text-[15px] leading-[22px] text-ink-2">
        Used by teams doing lorem ipsum in the ipsum at lorem.
      </p>
      <div className="flex-1 w-full grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 border-l border-t border-hair">
        {LOGOS.map((l, i) => (
          <div
            className="h-28 flex items-center justify-center border-r border-b border-hair"
            key={l.name}
          >
            <span
              className="logo-sweep flex items-center gap-2"
              style={{ '--i': i } as CSSProperties}
            >
              {l.mark && (
                <span className="shrink-0 flex text-logo">{l.mark}</span>
              )}
              <span
                className="leading-6 text-logo whitespace-nowrap"
                style={l.style}
              >
                {l.name}
              </span>
            </span>
          </div>
        ))}
      </div>
    </section>
  );
}
