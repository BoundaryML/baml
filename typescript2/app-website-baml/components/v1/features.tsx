import { section } from '../analytics';
import Reveal from '../reveal';
import { LANES, LEFT, PAD_Y, WRAP } from '../ui';

// Commands are set in mono.
const FEATURES: { title: string; body: string; mono?: boolean }[] = [
  {
    body: 'Write unions, generics, and classes in a syntax TypeScript developers already read.',
    title: 'TypeScript-like syntax',
  },
  {
    body: 'Model replies are parsed into your types, and the recorded data keeps those types.',
    title: 'Types that hold in production',
  },
  {
    body: 'BAML records every call, error, call path, and code version by default, along with the user and metadata you attach. One line above a function changes how much it keeps.',
    title: 'Recording in the runtime',
  },
  {
    body: 'Query recorded calls with SQL, on your laptop or in Boundary.',
    mono: true,
    title: 'baml query',
  },
  {
    body: "Find functions, types, and tests by their place in the syntax tree, so an agent doesn't have to search text.",
    mono: true,
    title: 'baml describe',
  },
  {
    body: 'Run any function or test from the command line.',
    mono: true,
    title: 'baml test and baml run',
  },
  {
    body: 'Handle every error case explicitly, and run work concurrently with spawn and await.',
    title: 'Typed errors and green threads',
  },
  {
    body: 'Generate a typed client for Python, TypeScript, Go, Rust, Java, Swift, or C++, and keep the rest of your app as it is.',
    title: 'Incremental adoption',
  },
];

export default function Features() {
  return (
    <section
      {...section('features')}
      aria-labelledby="features-title"
      className={`${WRAP} ${PAD_Y}`}
    >
      <div className={`${LANES} mb-10 md:mb-14 lg:mb-16`}>
        <div className={`${LEFT} flex flex-col gap-4`}>
          <h2 className="h2" id="features-title">
            What&apos;s in the language.
          </h2>
        </div>
        <p className="max-w-[520px] lg:pt-2.5 text-[17px] leading-[27px] text-body">
          BAML combines a familiar syntax, types that hold at runtime, and a
          command-line toolchain that people and coding agents both use.
        </p>
      </div>
      {/* One column on a phone, where each card is already fenced by its own rule, so the row gap
          only has to be a beat — not the two-row separation it is at four across. */}
      <ul className="grid sm:grid-cols-2 lg:grid-cols-4 gap-x-10 gap-y-9 sm:gap-y-12 lg:gap-y-14">
        {FEATURES.map((f, i) => (
          <li key={f.title}>
            <Reveal
              className={`h-full flex flex-col gap-2.5 pt-5 border-t ${i === 0 ? 'border-rule-accent' : 'border-rule'}`}
              delay={(i % 4) * 60}
            >
              <h3
                className={
                  f.mono
                    ? 'font-mono font-medium text-[19px] leading-[27px]'
                    : 'font-display font-medium text-[21px] leading-[27px] tracking-[-0.03em]'
                }
              >
                {f.title}
              </h3>
              <p className="text-[15px] leading-[23px] text-body">{f.body}</p>
            </Reveal>
          </li>
        ))}
      </ul>
    </section>
  );
}
