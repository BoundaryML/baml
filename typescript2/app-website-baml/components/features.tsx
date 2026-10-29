import { section } from './analytics';
import Reveal from './reveal';
import { LANES, LEFT, PAD_Y, WRAP } from './ui';

// The eight features listed on boundaryml.com. Commands are set in mono.
const FEATURES: { title: string; body: string; mono?: boolean }[] = [
  {
    body: 'Unions, generics and interfaces, in a syntax TypeScript developers already read.',
    title: 'TypeScript-like syntax',
  },
  {
    body: 'Types stay meaningful when the code runs. No any, no unchecked casts.',
    title: 'Runtime types',
  },
  {
    body: 'Explicit error handling and exhaustive pattern matching.',
    title: 'Typed errors',
  },
  { body: 'Concurrency with spawn and await.', title: 'Green threads' },
  {
    body: 'LLM functions, tests, testsets and evaluations are part of the language.',
    title: 'Native LLM functions',
  },
  {
    body: 'AST-aware code discovery, so an agent finds what it needs without guessing.',
    mono: true,
    title: 'baml describe',
  },
  {
    body: 'Run any function from the command line.',
    mono: true,
    title: 'baml run',
  },
  {
    body: 'Generated SDKs let you call BAML from the code you already have.',
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
          Familiar syntax, honest types, and a toolchain an agent can drive from
          the command line.
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
