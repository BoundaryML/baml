import ground from '@boundaryml/brand/imagery/baml/violet-dusk.jpg';
import Image from 'next/image';
import { section } from './analytics';

// Adoption, right after the language section: three ways into Boundary, each with the same codebase
// drawn small and filled as far as that way takes it: all of it in BAML, some of it, or none
// (sending OpenTelemetry instead). Only incremental adoption is recommended, so only its card shows
// Learn more without a hover, and on a phone it comes first. Each card leads to that route's
// section on /adopt.

// A codebase as a 6 by 4 treemap: [name, column, row, columns spanned, rows spanned, the order it
// moves into BAML]. A file that never moves has no order.
const FILES: [string, number, number, number, number, number?][] = [
  ['server.py', 1, 1, 3, 2],
  ['classify.py', 4, 1, 2, 1, 1],
  ['cfg.yaml', 6, 1, 1, 1],
  ['billing.go', 4, 2, 1, 2],
  ['route.ts', 5, 2, 2, 1, 3],
  ['db.py', 1, 3, 1, 2],
  ['models.py', 2, 3, 2, 1],
  ['extract.py', 5, 3, 2, 2, 2],
  ['notify.ts', 2, 4, 1, 1, 4],
  ['index.ts', 3, 4, 2, 1],
];

type Fill = 'all' | 'some' | 'none';

const WAYS: {
  body: string;
  // The button beside the title.
  cta: string;
  fill: Fill;
  href: string;
  // Only incremental adoption is recommended: its card carries the badge, shows its button without
  // a hover, and comes first on a phone.
  recommended?: boolean;
  title: string;
}[] = [
  {
    body: 'Everything you write gets hyperobservability.',
    cta: 'Start today',
    fill: 'all',
    href: '/adopt#rewrite',
    title: 'Rewrite your stack in BAML',
  },
  {
    body: 'Move one function at a time. BAML generates a typed SDK your existing code calls.',
    cta: 'Start today',
    fill: 'some',
    href: '/adopt#incremental',
    recommended: true,
    title: 'Adopt BAML incrementally',
  },
  {
    body: 'Keep your code. Send the traces you already have.',
    cta: 'Learn more',
    fill: 'none',
    href: '/adopt#otel',
    title: 'Send OTel to Boundary Cloud',
  },
];

/** The codebase, filled as far as this way into BAML takes it. */
function Mini({ fill }: { fill: Fill }) {
  return (
    <div aria-hidden className="grid h-full grid-cols-6 grid-rows-4 gap-1">
      {FILES.map(([name, col, row, w, h, at]) => {
        const lit =
          fill === 'all' || (fill === 'some' && at !== undefined && at <= 2);
        return (
          <span
            className={`rounded-[4px] border ${lit ? 'border-clover/70 bg-clover' : 'border-rule bg-white/[0.02]'}`}
            key={name}
            style={{
              gridColumn: `${col} / span ${w}`,
              gridRow: `${row} / span ${h}`,
            }}
          />
        );
      })}
    </div>
  );
}

/** The three cards. */
function AdoptCards() {
  return (
    <div className="grid gap-3 md:grid-cols-3">
      {WAYS.map((w) => {
        const go = w.recommended === true;
        return (
          <a
            className={`focus-ring group flex flex-col overflow-hidden rounded-2xl border bg-raised transition-[border-color,translate,box-shadow] duration-300 hover:-translate-y-1 focus-visible:-translate-y-1 motion-reduce:transition-none motion-reduce:hover:translate-y-0 ${go ? 'max-md:order-first border-clover/60 hover:border-clover hover:shadow-[0_16px_48px_-16px_rgb(from_var(--color-clover)_r_g_b/0.55)]' : 'border-rule hover:border-white/35 hover:shadow-[0_16px_48px_-16px_rgb(255_255_255/0.18)]'}`}
            href={w.href}
            key={w.title}
          >
            <div className="relative h-36 border-b border-rule p-4">
              <Mini fill={w.fill} />
              {go && (
                <span className="absolute top-3 left-3 inline-flex items-center gap-1.5 rounded-full border border-clover/60 bg-raised px-2.5 py-1 text-[12px] font-medium text-clover">
                  <span className="size-1.5 rounded-full bg-clover" />
                  Recommended
                </span>
              )}
              {w.fill === 'none' && (
                <span className="absolute right-3 bottom-3 rounded-md border border-white/15 bg-raised px-2 py-0.5 font-mono text-[11px] text-ink-2">
                  OTel → Boundary
                </span>
              )}
            </div>
            <div className="flex flex-1 flex-col p-5 md:p-6">
              {/* The title, with the next step to its right: always shown on the recommended card; on
                      the others it slides in on hover or focus, and is always shown on touch screens,
                      which have no hover. */}
              <div className="flex items-start justify-between gap-3">
                <h3 className="v4-h3 text-head">{w.title}</h3>
                <span
                  className={`mt-1 inline-flex shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full px-3 py-1 text-[13px] font-medium transition-[opacity,translate] duration-200 ease-out motion-reduce:transition-none ${go ? 'bg-clover text-black' : '-translate-x-1 border border-white/30 text-head opacity-0 group-hover:translate-x-0 group-hover:opacity-100 group-focus-visible:translate-x-0 group-focus-visible:opacity-100 [@media(hover:none)]:translate-x-0 [@media(hover:none)]:opacity-100'}`}
                >
                  {w.cta}
                  <span className="transition-[translate] duration-200 group-hover:translate-x-0.5 motion-reduce:transition-none">
                    →
                  </span>
                </span>
              </div>
              <p className="mt-3 v4-body text-ink-2">{w.body}</p>
            </div>
          </a>
        );
      })}
    </div>
  );
}

export default function Adopt() {
  return (
    <section
      {...section('adopt')}
      aria-labelledby="adopt"
      className="on-paint relative overflow-hidden border-t border-rule"
    >
      {/* The one painted band between the hero and the closing section, so the place to start
          stands apart from the black sections around it: the brand's violet dusk, dimmed toward
          the cards so their text keeps its contrast. On a phone the cards stack several screens
          tall, so the painting sits behind the heading only and fades into the ground; stretched
          over the whole section it would be enlarged until it blurred. */}
      <div className="absolute inset-x-0 top-0 h-[34rem] md:inset-0 md:h-auto">
        <Image
          alt=""
          className="object-cover object-[50%_35%]"
          fill
          sizes="(max-width: 767px) 250vw, 100vw"
          src={ground}
        />
        <div className="absolute inset-0 bg-[linear-gradient(180deg,rgb(10_10_10/0.35)_0%,rgb(10_10_10/1)_100%)] md:bg-[linear-gradient(180deg,rgb(10_10_10/0.35)_0%,rgb(10_10_10/0.6)_100%)]" />
      </div>
      <div className="relative v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 max-w-[22em]" id="adopt">
          Start where feedback loops matter most, and keep the rest of your
          code.
        </h2>
        <div className="mt-10 md:mt-12">
          <AdoptCards />
        </div>
      </div>
    </section>
  );
}
