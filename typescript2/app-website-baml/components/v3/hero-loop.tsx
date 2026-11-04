'use client';

import { useState } from 'react';
import LoopLine from './loop-line';

// The whole idea in four words and one line. Each stage is one command from the same toolchain, and the
// line back from Monitor to Build is the part other toolchains leave to you.
const STAGES = [
  {
    command: 'baml check',
    detail: 'Types are checked before anything runs.',
    label: 'Build',
  },
  {
    command: 'baml test',
    detail: 'Tests run from the file next to your function.',
    label: 'Test',
  },
  {
    command: 'baml pack',
    detail: 'Your program ships as one standalone executable.',
    label: 'Deploy',
  },
  {
    command: 'baml query',
    detail: 'Every call is recorded, and you can query it with SQL.',
    label: 'Monitor',
  },
];
const RETURN = 'Your agent reads what happened and starts the next build.';
const STATIONS = [0.125, 0.375, 0.625, 0.875];

export default function HeroLoop() {
  const [active, setActive] = useState(-1);
  const back = active === STAGES.length;

  return (
    <figure
      aria-label="Build with baml check, test with baml test, deploy with baml pack, and monitor with baml query. A line runs from monitor back to build, because what you learn in production goes into your next build."
      className="relative pb-24"
    >
      <div
        aria-hidden
        className="v3-glow left-[-10%] right-[-10%] -top-24 h-[720px]"
      />
      <div className="relative grid grid-cols-4">
        {STAGES.map((s, i) => (
          <div className="min-w-0 text-center" key={s.label}>
            <p
              className={`v3-stage font-display font-medium text-[clamp(20px,6vw,88px)] leading-none tracking-[-0.05em] ${active === i ? 'v3-stage-on' : ''}`}
            >
              {s.label}
            </p>
            <p
              className={`mt-2 md:mt-4 font-mono text-[10.5px] md:text-[17px] transition-colors duration-300 ${active === i ? 'text-clover' : 'text-white/40'}`}
            >
              {s.command}
            </p>
          </div>
        ))}
      </div>

      <div className="relative h-[220px] md:h-[300px] mt-4 md:mt-8">
        <LoopLine
          active={active}
          inset={22}
          onStage={setActive}
          stations={STATIONS}
          top={16}
        />
        <div className="absolute inset-x-[16%] top-12 bottom-14 flex items-center justify-center text-center">
          <p
            className="v3-enter font-display text-[clamp(17px,2.5vw,36px)] leading-[1.2] tracking-[-0.03em] text-white/90 text-balance"
            key={active}
          >
            {back ? RETURN : (STAGES[active]?.detail ?? STAGES[0].detail)}
          </p>
        </div>
        <p
          className={`absolute left-1/2 bottom-[22px] -translate-x-1/2 translate-y-1/2 px-3 md:px-5 bg-black whitespace-nowrap text-[12.5px] md:text-lg transition-colors duration-300 ${back ? 'text-clover' : 'text-white/60'}`}
        >
          What you learn in production goes into your next build
        </p>
      </div>
    </figure>
  );
}
