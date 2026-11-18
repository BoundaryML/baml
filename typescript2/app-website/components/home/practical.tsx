import type { ReactNode } from 'react';
import { section } from './analytics';
import { Cheap } from './stack';

// What recording everything costs, in one section. Cheap comes first and runs the full width: one
// comparison with one slider, in ./stack.tsx beside the storage math it uses. Fast and Multimodal
// sit under it as two cards, carrying the numbers of the larger speed chart and storage-engine
// figure in ./stack.tsx, which the homepage no longer mounts. The speed numbers were measured on the
// machine named under them.

/** Overhead of observing one function, in nanoseconds. */
// `times` is how many times slower than Boundary; Boundary's own row has none.
const OVERHEAD: { lit?: boolean; name: string; ns: number; times?: string }[] =
  [
    { lit: true, name: 'Boundary', ns: 32 },
    { name: 'OTel + Rust', ns: 186, times: '5.8×' },
    { name: 'OTel + TypeScript', ns: 702, times: '22×' },
    { name: 'OTel + Python', ns: 9288, times: '290×' },
  ];
// Bars are drawn to scale against the TypeScript row. Python is 13 times that, so its bar runs the
// full width with a break in it instead.
const SCALE = 702;

const KINDS: { says: string; title: string }[] = [
  {
    says: 'Images, audio, video, PDFs, large strings and bytes.',
    title: 'Every kind of data',
  },
  {
    says: 'Runtime and cloud collaborate to minimize duplicated data.',
    title: 'Recursive deduping',
  },
  {
    says: 'Classes and enums stored with their definitions.',
    title: 'Keeps your types',
  },
  {
    says: 'Keep PII data private. Withhold a value, and the trace records why.',
    title: 'Redaction',
  },
];

/** One of the three cards: the claim as a word, its number, and the figure that backs it. */
function Card({
  children,
  figure,
  unit,
  word,
}: {
  children: ReactNode;
  figure: string;
  unit: string;
  word: string;
}) {
  return (
    <div className="flex flex-col rounded-xl border border-rule bg-raised p-5 md:p-6">
      <h3 className="v4-caption text-clover">{word}</h3>
      <p className="mt-4 flex flex-wrap items-baseline gap-x-2.5">
        <span className="font-display text-[36px] font-medium leading-none tracking-[-0.03em] text-head">
          {figure}
        </span>
        <span className="text-[16px] text-ink-2">{unit}</span>
      </p>
      <div className="mt-6 flex flex-1 flex-col">{children}</div>
    </div>
  );
}

/** A horizontal bar on a faint track. `broken` draws a gap near the end, for a value off the scale. */
function Bar({
  broken,
  lit,
  share,
}: {
  broken?: boolean;
  lit?: boolean;
  share: number;
}) {
  return (
    <span className="relative block h-2 rounded-full bg-white/[0.07]">
      <span
        className={`absolute inset-y-0 left-0 rounded-full ${lit ? 'bg-clover' : 'bg-white/45'}`}
        style={{ width: `${Math.max(2, Math.min(1, share) * 100)}%` }}
      />
      {broken && (
        <span className="absolute inset-y-[-2px] right-[14%] w-1.5 -skew-x-12 bg-raised" />
      )}
    </span>
  );
}

/** Is recording everything practical? Cheap as a box of its own, then Fast and Multimodal. */
export default function Practical() {
  return (
    <section
      {...section('practical')}
      aria-labelledby="practical"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 text-head max-w-[22em]" id="practical">
          We made observability cheap, fast, and multimodal.
        </h2>
        <p className="mt-4 max-w-[40rem] v4-body text-ink-2">
          Because we own the language and the BAML runtime, we're able to make
          optimizations to telemetry that no other language can.
        </p>

        <div className="mt-10 md:mt-12">
          <Cheap />
        </div>
        <div className="mt-4 grid gap-4 md:grid-cols-2">
          <Card figure="32 ns" unit="to observe a function" word="Fast">
            <ul className="flex flex-col gap-3.5">
              {OVERHEAD.map((r) => (
                <li key={r.name}>
                  <p className="flex items-baseline justify-between gap-3 text-[14px]">
                    <span
                      className={`whitespace-nowrap ${r.lit ? 'text-head' : 'text-ink-2'}`}
                    >
                      {r.name}
                    </span>
                    {/* The narrowest phones drop the word "slower", so the name and its number
                        stay on one line. */}
                    <span className="whitespace-nowrap font-mono text-[12px] text-muted">
                      {r.ns.toLocaleString('en-US')} ns ·{' '}
                      {r.times ? (
                        <>
                          {r.times}
                          <span className="max-[359px]:hidden"> slower</span>
                        </>
                      ) : (
                        'baseline'
                      )}
                    </span>
                  </p>
                  <span className="mt-1.5 block">
                    <Bar
                      broken={r.ns > SCALE}
                      lit={r.lit}
                      share={r.ns / SCALE}
                    />
                  </span>
                </li>
              ))}
            </ul>
            <p className="mt-auto pt-5 text-[13px] text-white/45">
              Measured on Apple M2 Max, 64 GB.
            </p>
          </Card>

          <Card
            figure="One store"
            unit="for traces and files"
            word="Multimodal"
          >
            <ul className="flex flex-col gap-3.5">
              {KINDS.map((k) => (
                <li key={k.title}>
                  <p className="text-[14px] text-head">{k.title}</p>
                  <p className="mt-0.5 text-[14px] leading-snug text-ink-2">
                    {k.says}
                  </p>
                </li>
              ))}
            </ul>
            <p className="mt-auto pt-5 text-[16px] leading-snug text-ink-2">
              Runs locally or in the cloud.
            </p>
          </Card>
        </div>
      </div>
    </section>
  );
}
