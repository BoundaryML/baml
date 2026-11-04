'use client';

import { section } from '../analytics';
import { Code, proseLines } from '../v3/code';
import { FIRST, source } from '../v3/hero-video';

const LINES = source(FIRST);

const FEATURES = [
  {
    body: 'Write unions, generics, and classes in a syntax TypeScript developers already read.',
    title: 'TypeScript-like syntax.',
  },
  {
    body: "The errors a function can throw are part of its type, and baml check reports any it doesn't handle.",
    title: 'Rust-like correctness.',
  },
  {
    body: 'baml check type-checks 60,000 lines in about 1.4 seconds, and green threads run work concurrently with spawn and await.',
    title: 'Fast checks and Go-like concurrency.',
  },
  {
    body: 'Model replies are parsed into your types, and the runtime records every call with its typed arguments and result, with no tracing SDK.',
    title: 'Types that hold at runtime.',
  },
];

/** Section 4: what the language is like, with the sample the rest of the page uses. */
export default function Language() {
  return (
    <section
      {...section('language')}
      aria-labelledby="language"
      className="border-t border-white/[0.08]"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 max-w-[22em] text-balance" id="language">
          The syntax of TypeScript, the correctness of Rust, the concurrency of
          Go, and reflection as good as Python's.
        </h2>
        <p className="mt-6 max-w-[38rem] text-lg leading-[1.55] text-white/70">
          BAML is the language the whole stack reads. Every tool in it
          understands your types, your call graph, and which version of the code
          ran.
        </p>
        <div className="mt-10 md:mt-14 grid lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] gap-x-12 gap-y-10 items-start">
          <figure className="rounded-xl overflow-hidden border border-white/12 bg-[#0b0b0c]">
            <figcaption className="h-9 flex items-center px-4 border-b border-white/[0.08] bg-[#111113] font-mono text-[12px] text-white/50">
              triage.baml
            </figcaption>
            <div className="py-4 overflow-x-auto">
              <Code lines={LINES} prose={[...proseLines(LINES)]} />
            </div>
          </figure>
          <dl className="grid sm:grid-cols-2 lg:grid-cols-1 gap-x-8 gap-y-7">
            {FEATURES.map((f) => (
              <div key={f.title}>
                <dt className="font-display font-medium text-[19px] tracking-[-0.015em] text-white">
                  {f.title}
                </dt>
                <dd className="mt-1.5 text-[16px] leading-[1.55] text-white/65">
                  {f.body}
                </dd>
              </div>
            ))}
          </dl>
        </div>
      </div>
    </section>
  );
}
