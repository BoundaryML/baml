'use client';

import { type ReactNode, useEffect, useState } from 'react';
import { FIXES } from './story';

// An agent going around the loop on its own, sped up. Every lap checks, tests, ships, reads production,
// and fixes what it found, and each lap runs faster than the last until the counter is a blur. The output
// is example data.
const LABELS = ['Build', 'Test', 'Deploy', 'Monitor'];
const LAPS = 12;
const SHOWN = 22;

type Line = { stage: number; text: ReactNode };

const Ok = ({ children }: { children: ReactNode }) => (
  <span className="text-clover">{children}</span>
);
const Cmd = ({ children }: { children: string }) => (
  <span className="text-white">
    <span className="text-clover">$ </span>
    {children}
  </span>
);

function lapLines(lap: number): Line[] {
  const tests = 6 + lap;
  const found = Math.max(14 - lap * 3, 2);
  const fix = FIXES[lap % FIXES.length];
  return [
    { stage: 0, text: <Cmd>baml check</Cmd> },
    {
      stage: 0,
      text: (
        <>
          <Ok>Finished</Ok> checked 4 files
        </>
      ),
    },
    { stage: 1, text: <Cmd>baml test classify</Cmd> },
    {
      stage: 1,
      text: (
        <>
          {tests} tests, <Ok>{tests} passed</Ok>
        </>
      ),
    },
    { stage: 2, text: <Cmd>baml pack triage</Cmd> },
    { stage: 2, text: <>Packed triage v{42 + lap}</> },
    {
      stage: 3,
      text: (
        <Cmd>
          {'baml query "SELECT input FROM calls WHERE label = \'other\'"'}
        </Cmd>
      ),
    },
    {
      stage: 3,
      text: (
        <span className="text-[#FF9A9E]">
          {found} {fix.failure}
        </span>
      ),
    },
    {
      stage: 0,
      text: (
        <span className="text-white/50">Added to the prompt: {fix.rule}</span>
      ),
    },
    {
      stage: 0,
      text: (
        <span className="text-white/50">
          Added test {fix.test}_{lap + 1}
        </span>
      ),
    },
  ];
}

const SCRIPT = Array.from({ length: LAPS }, (_, lap) =>
  lapLines(lap).map((line) => ({ ...line, lap })),
).flat();

export default function HeroTerminal() {
  const [shown, setShown] = useState(1);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setShown(SCRIPT.length);
      return;
    }
    let id = 0;
    const step = (n: number) => {
      const lap = SCRIPT[Math.min(n, SCRIPT.length - 1)].lap;
      // Slow enough to read on the first lap, a blur by the last, then a pause before it starts over.
      const delay = n >= SCRIPT.length ? 3200 : Math.max(55, 380 * 0.72 ** lap);
      id = window.setTimeout(() => {
        const next = n >= SCRIPT.length ? 1 : n + 1;
        setShown(next);
        step(next);
      }, delay);
    };
    step(1);
    return () => clearTimeout(id);
  }, []);

  const visible = SCRIPT.slice(Math.max(0, shown - SHOWN), shown);
  const current = SCRIPT[shown - 1];
  const finished = shown >= SCRIPT.length;
  const laps = finished ? LAPS : current.lap;

  return (
    <figure
      aria-label="An agent's terminal, sped up. Lap after lap, the agent runs baml check, baml test, baml pack, and baml query, then fixes what it found in production, faster each time."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className="v3-glow left-[-10%] right-[-10%] -top-24 h-[720px]"
      />
      <div className="relative grid lg:grid-cols-[minmax(0,1fr)_260px] gap-8 lg:gap-12 items-center">
        <div className="v3-window overflow-hidden">
          <div className="flex items-center h-11 px-4 border-b border-white/8 text-[13px] text-white/55">
            Your agent, in triage/
          </div>
          <div className="flex flex-col justify-end h-[420px] md:h-[520px] px-4 md:px-5 pb-4 overflow-hidden font-mono text-[12px] md:text-[14px] leading-[21px] md:leading-[23px] text-[#BDBDC4]">
            {visible.map((line, i) => (
              <p
                className="truncate"
                // biome-ignore lint/suspicious/noArrayIndexKey: lines only ever append
                key={shown - visible.length + i}
              >
                {line.text}
              </p>
            ))}
          </div>
        </div>

        <div className="flex lg:flex-col items-end lg:items-start justify-between gap-6">
          <div>
            <p className="text-[14px] text-white/45">Laps</p>
            <p className="font-display font-medium text-[64px] md:text-[96px] leading-none tracking-[-0.05em] tabular-nums">
              {laps}
            </p>
          </div>
          <ul className="flex lg:flex-col gap-4 lg:gap-2.5">
            {LABELS.map((label, i) => (
              <li
                className={`v3-stage font-display font-medium text-[16px] md:text-[26px] tracking-[-0.03em] ${!finished && current.stage === i ? 'v3-stage-on' : ''}`}
                key={label}
              >
                {label}
              </li>
            ))}
          </ul>
          <p
            className={`hidden lg:block text-[15px] transition-colors duration-300 ${finished ? 'text-clover' : 'text-white/50'}`}
          >
            {finished
              ? 'Twelve laps. Nobody had to step in.'
              : 'Check, test, ship, read production, fix, and again'}
          </p>
        </div>
      </div>
    </figure>
  );
}
