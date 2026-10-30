import type { ReactNode } from 'react';
import { section } from '../analytics';

// A small picture of each feature on a hairline grid. Commands are set in mono.
const Mono = ({ children }: { children: ReactNode }) => (
  <div className="font-mono text-[12.5px] leading-[1.7] text-white/80">
    {children}
  </div>
);
const K = ({ children }: { children: ReactNode }) => (
  <span className="text-clover">{children}</span>
);
const S = ({ children }: { children: ReactNode }) => (
  <span className="text-periwinkle">{children}</span>
);

const THREADS = [78, 46, 92];

const FEATURES: {
  title: string;
  body: string;
  mono?: boolean;
  picture: ReactNode;
}[] = [
  {
    body: 'Write unions, generics, and classes in a syntax TypeScript developers already read.',
    picture: (
      <Mono>
        <K>type</K> Label = <S>"a"</S> | <S>"b"</S>
      </Mono>
    ),
    title: 'TypeScript-like syntax',
  },
  {
    body: 'Model replies are parsed into your types, and the recorded data keeps those types.',
    picture: (
      <Mono>
        reply <K>→</K> Verdict <span className="text-pasture-200">✓</span>
      </Mono>
    ),
    title: 'Types that hold in production',
  },
  {
    body: 'BAML records every call, error, call path, and code version by default, along with the user and metadata you attach. One line above a function changes how much it keeps.',
    picture: (
      <svg
        aria-hidden="true"
        className="w-full h-5"
        preserveAspectRatio="none"
        viewBox="0 0 1000 20"
      >
        <path className="v2-wire" d="M0 10H1000" pathLength={1000} />
        <path className="v2-pulse" d="M0 10H1000" pathLength={1000} />
      </svg>
    ),
    title: 'Recording in the runtime',
  },
  {
    body: 'Query recorded calls with SQL, on your laptop or in Boundary.',
    mono: true,
    picture: (
      <Mono>
        <K>SELECT</K> * <K>FROM</K> spans
      </Mono>
    ),
    title: 'baml query',
  },
  {
    body: "Find functions, types, and tests by their place in the syntax tree, so an agent doesn't have to search text.",
    mono: true,
    picture: <Mono>extract.baml:7 · 2 tests</Mono>,
    title: 'baml describe',
  },
  {
    body: 'Run any function or test from the command line.',
    mono: true,
    picture: (
      <Mono>
        2 tests · <span className="text-pasture-200">2 passed</span>
      </Mono>
    ),
    title: 'baml test and baml run',
  },
  {
    body: 'Handle every error case explicitly, and run work concurrently with spawn and await.',
    picture: (
      <div className="flex flex-col gap-1.5">
        {THREADS.map((w) => (
          <div
            className="h-1 rounded-full bg-clover/70"
            key={w}
            style={{ width: `${w}%` }}
          />
        ))}
      </div>
    ),
    title: 'Typed errors and green threads',
  },
  {
    body: 'Generate a typed client for Python, TypeScript, Go, Rust, Java, Swift, or C++, and keep the rest of your app as it is.',
    picture: <Mono>.py .ts .go .rs .java .swift .cpp</Mono>,
    title: 'Incremental adoption',
  },
];

export default function Features() {
  return (
    <section
      {...section('features')}
      aria-labelledby="features-title"
      className="max-w-[1440px] mx-auto px-5 md:px-12 pt-20 md:pt-36 scroll-mt-16"
    >
      <h2 className="v2-h2" id="features-title">
        What&apos;s in the language.
      </h2>
      <p className="max-w-160 mt-6 text-xl leading-[1.5] text-white/70">
        BAML combines a familiar syntax, types that hold at runtime, and a
        command-line toolchain that people and coding agents both use.
      </p>
      <ul className="grid sm:grid-cols-2 lg:grid-cols-4 gap-px mt-12 rounded-2xl overflow-hidden bg-white/12 border border-white/12">
        {FEATURES.map((f) => (
          <li
            className="flex flex-col gap-5 p-6 md:p-7 bg-black transition-colors hover:bg-clover-900/40"
            key={f.title}
          >
            <div className="flex items-center min-h-12">{f.picture}</div>
            <div>
              <h3
                className={
                  f.mono
                    ? 'font-mono font-medium text-[18px] leading-[27px]'
                    : 'font-display font-medium text-[20px] leading-[27px] tracking-[-0.03em]'
                }
              >
                {f.title}
              </h3>
              <p className="mt-2 text-[15px] leading-[23px] text-white/65">
                {f.body}
              </p>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
