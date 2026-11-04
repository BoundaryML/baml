'use client';

import { useEffect, useState } from 'react';

// The usual stack for an AI feature, scattered and wired together by hand, folds into one toolchain that
// covers the whole loop. The tools are kinds of tools rather than products.
const TOOLS = [
  { label: 'Python', x: 12, y: 16 },
  { label: 'Prompt templates', x: 40, y: 8 },
  { label: 'Test runner', x: 70, y: 14 },
  { label: 'Tracing SDK', x: 89, y: 36 },
  { label: 'Log search', x: 76, y: 70 },
  { label: 'Notebooks', x: 47, y: 88 },
  { label: 'Deploy scripts', x: 15, y: 76 },
  { label: 'Eval spreadsheet', x: 28, y: 46 },
];
const WIRES = [
  [0, 1],
  [1, 2],
  [2, 3],
  [3, 4],
  [4, 5],
  [5, 6],
  [6, 0],
  [0, 7],
  [7, 2],
  [7, 4],
  [1, 5],
  [6, 3],
];
const STAGES = [
  ['Build', 'baml check'],
  ['Test', 'baml test'],
  ['Deploy', 'baml pack'],
  ['Monitor', 'baml query'],
];
const TICK_MS = 100;
const MERGE = 30;
const ONE = 38;
const END = 90;

export default function HeroTools() {
  const [t, setT] = useState(0);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setT(ONE + 10);
      return;
    }
    const id = setInterval(() => setT((n) => (n + 1) % END), TICK_MS);
    return () => clearInterval(id);
  }, []);

  const merged = t >= MERGE;
  const one = t >= ONE;
  const active = one ? Math.floor((t - ONE) / 7) % (STAGES.length + 1) : -1;

  return (
    <figure
      aria-label="Eight separate tools for building an AI feature, wired together by hand, fold into BAML: one toolchain that builds, tests, deploys, and monitors, and feeds production back into the next build."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className={`v3-glow left-[5%] right-[5%] -top-10 h-[600px] transition-opacity duration-700 ${one ? 'opacity-100' : 'opacity-40'}`}
      />
      <p
        aria-live="polite"
        className="v3-enter relative mb-6 text-center text-[15px] md:text-xl text-white/75 text-balance"
        key={String(one)}
      >
        {one
          ? 'One toolchain. Your agent drives all of it.'
          : 'Eight tools, wired together by hand. Your agent can reach a few of them.'}
      </p>
      <div className="relative h-[460px] md:h-[480px]">
        <svg
          aria-hidden="true"
          className={`absolute inset-0 size-full transition-opacity duration-500 ${merged ? 'opacity-0' : 'opacity-100'}`}
          preserveAspectRatio="none"
          viewBox="0 0 100 100"
        >
          {WIRES.map(([a, b]) => {
            const p = TOOLS[a];
            const q = TOOLS[b];
            return (
              <path
                d={`M${p.x} ${p.y}C${p.x + 12} ${p.y + 18} ${q.x - 12} ${q.y - 18} ${q.x} ${q.y}`}
                fill="none"
                key={`${a}-${b}`}
                stroke="rgb(255 255 255 / 0.24)"
                strokeDasharray="3 5"
                strokeWidth={1.25}
                vectorEffect="non-scaling-stroke"
              />
            );
          })}
        </svg>

        {TOOLS.map((tool, i) => (
          <div
            className="absolute -translate-x-1/2 -translate-y-1/2 transition-[left,top,opacity,scale] duration-700 ease-[cubic-bezier(0.65,0,0.35,1)]"
            key={tool.label}
            style={{
              left: merged ? '50%' : `${tool.x}%`,
              opacity: merged ? 0 : 1,
              scale: merged ? '0.6' : '1',
              top: merged ? '50%' : `${tool.y}%`,
              transitionDelay: `${merged ? i * 35 : 0}ms`,
            }}
          >
            <div
              className="v3-float px-3 md:px-4 py-2 rounded-lg border border-white/14 bg-[#121215] text-[13px] md:text-[15px] text-white/75 whitespace-nowrap shadow-[0_20px_50px_-10px_#000]"
              style={{ '--i': i } as React.CSSProperties}
            >
              {tool.label}
            </div>
          </div>
        ))}

        {one && (
          <div className="v3-land absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-[min(500px,100%)]">
            <div className="v3-card v3-card-on relative py-3 pl-14 pr-5 md:pl-16">
              <p className="v2-baml-mark mb-2 pt-2 text-[26px] md:text-[32px]">
                BAML
              </p>
              <ul>
                {STAGES.map(([label, command], i) => (
                  <li
                    className={`flex items-center justify-between gap-4 h-12 md:h-14 border-t border-white/8 transition-colors duration-300 ${active === i ? 'text-white' : 'text-white/45'}`}
                    key={label}
                  >
                    <span className="font-display font-medium text-[20px] md:text-[24px] tracking-[-0.03em]">
                      {label}
                    </span>
                    <span
                      className={`font-mono text-[13px] md:text-[15px] transition-colors duration-300 ${active === i ? 'text-clover' : ''}`}
                    >
                      {command}
                    </span>
                  </li>
                ))}
              </ul>
              {/* Monitor runs back up into Build. */}
              <svg
                aria-hidden="true"
                className="absolute left-5 md:left-6 bottom-6 md:bottom-7 w-4 h-[calc(100%-118px)] md:h-[calc(100%-134px)] overflow-visible"
                preserveAspectRatio="none"
                viewBox="0 0 16 100"
              >
                <path
                  d="M16 100H4V0H16"
                  fill="none"
                  stroke={
                    active === STAGES.length
                      ? 'var(--color-clover-core)'
                      : 'rgb(255 255 255 / 0.3)'
                  }
                  strokeWidth={1.5}
                  style={{ transition: 'stroke 300ms' }}
                  vectorEffect="non-scaling-stroke"
                />
              </svg>
            </div>
            <p
              className={`mt-4 text-center text-[14px] md:text-base transition-colors duration-300 ${active === STAGES.length ? 'text-clover' : 'text-white/50'}`}
            >
              What happens in production goes into the next build
            </p>
          </div>
        )}
      </div>
    </figure>
  );
}
