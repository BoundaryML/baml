'use client';

import { type ReactNode, useState } from 'react';
import { Highlight } from './code';
import LoopLine from './loop-line';

// The same loop with each stage showing the real thing: source, test results, production data, and a
// release. One example runs through all four, and its failures travel the line back. The data is example
// data.
function Command({ children }: { children: string }) {
  return (
    <p className="truncate font-mono text-[12.5px] text-white/50">
      <span className="text-clover">$ </span>
      {children}
    </p>
  );
}

function Build() {
  return (
    <>
      <div className="font-mono text-[12.5px] leading-[21px] text-[#BDBDC4] whitespace-pre overflow-hidden">
        {[
          'function classify(',
          '  text: string,',
          ') -> Verdict {',
          '  client: "openai/gpt-6-luna"',
          '  prompt: `Classify ${text}`',
          '}',
        ].map((line) => (
          <div key={line}>
            <Highlight line={line} />
          </div>
        ))}
      </div>
      <Command>baml check</Command>
    </>
  );
}

function Test() {
  return (
    <>
      <ul className="flex flex-col gap-2 text-[14px]">
        {[
          'refund_request',
          'login_bug',
          'plain_question',
          'sarcastic_ticket',
        ].map((name) => (
          <li
            className="flex items-center justify-between gap-3 font-mono text-[12.5px] text-[#BDBDC4]"
            key={name}
          >
            <span className="truncate">{name}</span>
            <span className="text-clover">passed</span>
          </li>
        ))}
      </ul>
      <Command>baml test classify</Command>
    </>
  );
}

function Monitor() {
  return (
    <>
      <div>
        <p className="font-display text-[34px] leading-none tracking-[-0.04em]">
          14
        </p>
        <p className="mt-1.5 text-[14px] leading-5 text-white/60">
          sarcastic tickets labelled "other"
        </p>
        <svg
          aria-hidden="true"
          className="block w-full h-14 mt-3"
          preserveAspectRatio="none"
          viewBox="0 0 240 56"
        >
          <defs>
            <linearGradient id="v3-area" x1="0" x2="0" y1="0" y2="1">
              <stop
                offset="0"
                stopColor="var(--color-clover-core)"
                stopOpacity={0.4}
              />
              <stop
                offset="1"
                stopColor="var(--color-clover-core)"
                stopOpacity={0}
              />
            </linearGradient>
          </defs>
          <path
            d="M0 44C24 42 36 46 60 42S96 38 120 40s36 2 54 -8 28 -28 40 -26 18 8 26 6V56H0Z"
            fill="url(#v3-area)"
          />
          <path
            d="M0 44C24 42 36 46 60 42S96 38 120 40s36 2 54 -8 28 -28 40 -26 18 8 26 6"
            fill="none"
            stroke="var(--color-clover-core)"
            strokeWidth={1.5}
            vectorEffect="non-scaling-stroke"
          />
        </svg>
      </div>
      <Command>baml query "SELECT ..."</Command>
    </>
  );
}

function Deploy() {
  return (
    <>
      <div>
        <div className="flex items-center gap-2 text-[15px]">
          <span className="size-2 rounded-full bg-clover shadow-[0_0_12px_var(--color-clover-core)]" />
          Live
        </div>
        <dl className="mt-3.5 flex flex-col gap-2 font-mono text-[12.5px] text-[#BDBDC4]">
          {[
            ['build', 'triage'],
            ['version', 'a41f9c2'],
            ['recording', 'on'],
          ].map(([key, value]) => (
            <div className="flex justify-between gap-3" key={key}>
              <dt className="text-white/40">{key}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      </div>
      <Command>baml pack triage</Command>
    </>
  );
}

const STAGES: { body: ReactNode; label: string }[] = [
  { body: <Build />, label: 'Build' },
  { body: <Test />, label: 'Test' },
  { body: <Deploy />, label: 'Deploy' },
  { body: <Monitor />, label: 'Monitor' },
];

const STATIONS = [0.12, 0.375, 0.625, 0.88];

export default function HeroCards() {
  const [active, setActive] = useState(-1);
  const back = active === STAGES.length;

  return (
    <figure
      aria-label="Four cards: build with baml check, test with baml test, deploy with baml pack, and monitor with baml query. A line runs from monitor back to build, carrying failed calls from production into new test cases."
      className="relative pb-24"
    >
      <div
        aria-hidden
        className="v3-glow left-[-10%] right-[-10%] -top-24 h-[720px]"
      />
      <div className="relative lg:h-[460px]">
        <div className="hidden lg:block">
          <LoopLine
            active={active}
            inset={28}
            lapMs={9000}
            nodes={false}
            onStage={setActive}
            stations={STATIONS}
            top={150}
          />
        </div>
        <div className="relative grid sm:grid-cols-2 lg:grid-cols-4 gap-3 lg:gap-6">
          {STAGES.map((s, i) => (
            <div
              className={`v3-card flex flex-col min-w-0 h-[300px] p-5 ${active === i ? 'v3-card-on' : ''}`}
              key={s.label}
            >
              <h3 className="font-display font-medium text-[26px] leading-none tracking-[-0.04em]">
                {s.label}
              </h3>
              <div className="flex flex-1 flex-col justify-between gap-3 min-h-0 mt-5">
                {s.body}
              </div>
            </div>
          ))}
        </div>
        <p
          className={`mt-4 lg:mt-0 lg:absolute lg:left-1/2 lg:bottom-7 lg:-translate-x-1/2 lg:translate-y-1/2 lg:px-5 lg:bg-black text-center lg:whitespace-nowrap text-[15px] lg:text-lg transition-colors duration-300 ${back ? 'text-clover' : 'text-white/60'}`}
        >
          14 failed calls become new test cases for your next build
        </p>
      </div>
    </figure>
  );
}
