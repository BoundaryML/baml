import type { ReactNode } from 'react';

// Destinations. Quickstart is this site's own page, and "Explore BAML" is the
// homepage's feature list. boundaryml.com is the Boundary site, linked only from the Boundary band
// and the footer.
export const LINKS = {
  boundary: 'https://boundaryml.com',
  demos: 'https://github.com/boundaryml/baml-demos',
  discord: 'https://discord.gg/yzaTpQ3tdT',
  explore: '/#features-title',
  github: 'https://github.com/boundaryml/baml',
  quickstart: '/quickstart',
} as const;

// Grid: 1440 max, two lanes of 528px + 96px gap + the rest. The side padding is the page frame:
// --frame-gutter out to the grid rails, then --frame-inset inside them (both in globals.css), so
// every section's content starts at one inset.
export const WRAP = 'w-full max-w-[1440px] mx-auto px-(--frame-pad)';

/* The page's vertical rhythm. The phone step is deliberately shorter than a scaled-down desktop
   one: 96px of air is 7% of the 1440 grid but a quarter of a 390px column, which reads as an
   absence rather than a break. 64px still separates two sections cleanly. */
export const PAD_Y = 'py-16 md:py-24 lg:py-32';
export const LANES = 'flex flex-col lg:flex-row gap-7 md:gap-10 lg:gap-24';
export const LEFT = 'lg:w-[528px] shrink-0';

export function Arrow() {
  return (
    <span
      aria-hidden
      className="inline-block transition-transform duration-200 group-hover:-translate-y-0.5 group-hover:translate-x-0.5"
    >
      ↗
    </span>
  );
}

type Variant = 'primary' | 'secondary' | 'onPainting';

const VARIANTS: Record<Variant, string> = {
  onPainting: 'border border-white/40 text-white hover:border-white',
  primary: 'bg-accent text-on-accent hover:bg-accent-hover',
  secondary: 'border border-accent/35 text-accent hover:border-accent/70',
};

/** The site's button styles. The accent fill is reserved for the one primary action in view. */
export function Button({
  href,
  variant = 'primary',
  children,
}: {
  href: string;
  variant?: Variant;
  children: ReactNode;
}) {
  return (
    <a
      className={`inline-flex items-center px-5 h-[46px] rounded-lg text-[15px] font-medium active:translate-y-px transition ${VARIANTS[variant]}`}
      href={href}
    >
      {children}
    </a>
  );
}

/* Syntax colours, shared by the hero editor and the walkthrough so one example reads the same
   wherever it appears: clover keywords, white types, periwinkle strings, muted comments. These only
   ever sit inside .on-paint panels, which are dark in both themes. */
export const K = ({ children }: { children: ReactNode }) => (
  <span className="text-clover">{children}</span>
);
export const T = ({ children }: { children: ReactNode }) => (
  <span className="text-white">{children}</span>
);
export const S = ({ children }: { children: ReactNode }) => (
  <span className="text-periwinkle">{children}</span>
);
export const C = ({ children }: { children: ReactNode }) => (
  <span className="text-muted">{children}</span>
);

/** Inline code inside prose. */
export function Cmd({ children }: { children: ReactNode }) {
  return <code className="font-mono text-[0.94em] text-ink-2">{children}</code>;
}
