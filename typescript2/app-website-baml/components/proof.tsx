import Image from 'next/image';
import { section } from './analytics';
import { Arrow, LINKS } from './ui';

/* Proof sits early, with a hard number backing the claim. The logos above it say who uses BAML; this
   is the proof a reader can check in a tab — every figure links to the place it came from.
   Read 14 Oct 2026: stars from the GitHub API; downloads are npm (@boundaryml/baml, 1.08M) plus
   PyPI (baml-py, 593k) over the preceding 30 days. Rounded down, so they stay true as they grow.
   Metrics are set in mono, this site's voice for numbers and code. */
const STATS = [
  { href: LINKS.github, label: 'stars on GitHub', metric: '9,400+' },
  {
    href: 'https://www.npmjs.com/package/@boundaryml/baml',
    label: 'SDK downloads a month',
    metric: '1.6M',
  },
  {
    href: '/#toolchain-title',
    label: 'languages baml generate targets',
    metric: '7',
  },
  {
    href: LINKS.pricing,
    label: 'open source, free to run locally',
    metric: 'Apache 2.0',
  },
];

// Logos keep their own brand colours. Rust's mark is solid, so it ships in both polarities — orange
// on the dark ground, black on the light one.
const LANGS: {
  lead: string;
  name: string;
  logo: string;
  light?: string;
  w: number;
}[] = [
  {
    lead: 'the syntax and readability of',
    logo: '/langs/typescript.svg',
    name: 'TypeScript',
    w: 32,
  },
  {
    lead: 'the correctness and tooling of',
    light: '/langs/rust-black.svg',
    logo: '/langs/rust-orange.svg',
    name: 'Rust',
    w: 32,
  },
  {
    lead: 'the compile times and concurrency of',
    logo: '/langs/go.svg',
    name: 'Go',
    w: 86,
  },
  {
    lead: 'the dynamism, and nothing else, of',
    logo: '/langs/python.png',
    name: 'Python',
    w: 32,
  },
];

const LOGO = 'self-start h-8 w-auto object-contain';

/* The numbers and the "feels like" strip are one 4 × 2 grid (2 across below lg, so 4 rows): two
   lists on the same column template, so the column lines are shared exactly. The grid is not a box
   inside the frame but part of it: the section drops the content inset (padding is --frame-gutter
   only), so the outer cells run rail to rail and the rails are the grid's left and right edges.
   Lines are gaps, not borders: the wrapper is filled with --grid-line and every cell paints the
   ground over it, so only the 1px gaps between cells show — no outer box, no doubled edges. A 1px
   top pad draws the top line rail to rail (where a section divider would be); there is no bottom
   pad, because Why's divider sits on the last row's bottom edge and is that line. The outer cells
   pad by --frame-inset on their outer side (.proof-cells in globals.css), so their text starts on
   the page's content column; inner sides keep the cell padding. Below 640 there are no rails and
   the gutter is 0, so the grid runs edge to edge with the phone's 20px at the screen edges.
   On a phone the inner padding drops to 12px: 'Apache 2.0' needs 136px at 360. */
const CELL = 'bg-ground';
const PAD = 'proof-pad pt-6 pb-7 lg:pt-7 lg:pb-8';
const COLS = 'proof-cells grid grid-cols-2 lg:grid-cols-4 gap-px';

export default function Proof() {
  return (
    <section
      {...section('proof')}
      aria-label="BAML in numbers"
      className="w-full max-w-[1440px] mx-auto px-(--frame-gutter) pt-2"
    >
      <div className="flex flex-col gap-px pt-px bg-grid-line">
        <ul className={COLS}>
          {STATS.map((s) => (
            <li className={CELL} key={s.label}>
              <a
                className={`group flex h-full flex-col gap-2.5 ${PAD}`}
                href={s.href}
              >
                {/* 'Apache 2.0' is ten monospaced characters, so below 640 it is sized off the
                    viewport and lands on one line at every width. */}
                <span className="font-mono font-medium text-[clamp(26px,2.9vw,38px)] max-sm:text-[clamp(22px,6.2vw,26px)] leading-none tracking-[-0.03em] text-head">
                  {s.metric}
                </span>
                <span className="text-sm leading-5 text-muted text-pretty transition-colors group-hover:text-body">
                  {s.label} <Arrow />
                </span>
              </a>
            </li>
          ))}
        </ul>
        <ul aria-label="What BAML feels like" className={COLS}>
          {LANGS.map((l) => (
            <li className={`${CELL} ${PAD} flex flex-col gap-3.5`} key={l.name}>
              {l.light ? (
                <>
                  <Image
                    alt=""
                    className={`${LOGO} dark-only`}
                    height={32}
                    src={l.logo}
                    width={l.w}
                  />
                  <Image
                    alt=""
                    className={`${LOGO} light-only`}
                    height={32}
                    src={l.light}
                    width={l.w}
                  />
                </>
              ) : (
                <Image
                  alt=""
                  className={LOGO}
                  height={32}
                  src={l.logo}
                  width={l.w}
                />
              )}
              <p className="flex flex-col gap-1.5">
                {/* Two lines reserved, so the four names sit on one line across the row even if a
                    lead wraps differently at some width in between. */}
                <span className="block min-h-10 lg:min-h-0 text-sm leading-5 text-muted">
                  {l.lead}
                </span>
                <span className="font-display font-medium text-[clamp(24px,2.1vw,30px)] leading-9 tracking-[-0.03em]">
                  {l.name}
                </span>
              </p>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
