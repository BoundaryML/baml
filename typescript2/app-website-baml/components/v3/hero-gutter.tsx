'use client';

import { type ReactNode, useEffect, useState } from 'react';
import { Code } from './code';
import { FIXES, RED } from './story';

// An editor with a real BAML file, and a gutter on the right where production shows up next to the code
// it ran: calls and latency beside each function, how often each prompt rule matters, how the labels split.
// Nothing types and nothing chats. The numbers tick up now and then, the latency line moves, and a dot
// beside a function lights when it's called. The source compiles with baml check; every number is example
// data.
const LINES = [
  'type Label = "billing" | "bug" | "other";',
  '',
  'class Verdict {',
  '  label: Label,',
  '  confidence: float,',
  '}',
  '',
  'function classify(text: string) -> Verdict {',
  '  client: "openai/gpt-6-luna"',
  '  prompt: `',
  '    Classify this support ticket.',
  ...FIXES.map((f) => `    ${f.rule}`),
  '    ${text}',
  '  `',
  '}',
  '',
  'function triage(tickets: string[]) -> string[] {',
  '  let bugs: string[] = [];',
  '  for (let ticket in tickets) {',
  '    let verdict = classify(ticket);',
  '    if (verdict.label == "bug") {',
  '      bugs.push(ticket);',
  '    }',
  '  }',
  '  return bugs;',
  '}',
];
const PROSE = [10, 11, 12, 13, 14];
const CLASSIFY = 7;
const TRIAGE = 18;

// How often each prompt rule changed the answer, in percent of calls.
const RULE_RATES = [6.1, 2.4, 3.8];

const TICK_MS = 500;
const SPARK = 28;

type Live = {
  classify: number;
  errors: number;
  lastClassify: number;
  lastTriage: number;
  now: number;
  rules: number[];
  spark: number[];
  ticks: number;
  triage: number;
};

const START: Live = {
  classify: 12408,
  errors: 0.8,
  lastClassify: 0,
  lastTriage: 0,
  now: 0,
  rules: RULE_RATES,
  spark: Array.from(
    { length: SPARK },
    (_, i) => 1380 + Math.sin(i * 0.9) * 120 + ((i * 53) % 90),
  ),
  ticks: 0,
  triage: 3102,
};

function next(s: Live, now: number): Live {
  const called = Math.random() < 0.45;
  const batch = called && Math.random() < 0.3;
  const ticks = s.ticks + 1;
  return {
    classify: s.classify + (called ? 1 + Math.floor(Math.random() * 3) : 0),
    errors:
      ticks % 14 === 0
        ? Math.max(0.6, Math.min(1.1, s.errors + (Math.random() - 0.5) * 0.2))
        : s.errors,
    lastClassify: called ? now : s.lastClassify,
    lastTriage: batch ? now : s.lastTriage,
    now,
    rules:
      ticks % 9 === 0
        ? s.rules.map((r) => Math.max(0.5, r + (Math.random() - 0.5) * 0.2))
        : s.rules,
    spark:
      ticks % 5 === 0
        ? [...s.spark.slice(1), 1250 + Math.random() * 380]
        : s.spark,
    ticks,
    triage: s.triage + (batch ? 1 : 0),
  };
}

const count = (n: number) => n.toLocaleString('en-US');

function ago(now: number, then: number) {
  if (!then) return '';
  const s = Math.floor((now - then) / 1000);
  return s < 1 ? 'just now' : `${s}s ago`;
}

function Sparkline({ points }: { points: number[] }) {
  const lo = Math.min(...points);
  const hi = Math.max(...points);
  const d = points
    .map(
      (p, i) =>
        `${(i / (points.length - 1)) * 64},${14 - ((p - lo) / (hi - lo || 1)) * 12}`,
    )
    .join(' ');
  const last = d.split(' ').at(-1)?.split(',') ?? ['0', '0'];
  return (
    <svg
      aria-hidden="true"
      className="shrink-0"
      height="16"
      viewBox="0 0 66 16"
      width="66"
    >
      <polyline
        fill="none"
        points={d}
        stroke="rgb(255 255 255 / 0.45)"
        strokeLinejoin="round"
        strokeWidth="1"
      />
      <circle
        cx={last[0]}
        cy={last[1]}
        fill="var(--color-clover-core)"
        r="1.75"
      />
    </svg>
  );
}

/** A function's dot: bright for a moment when it's called, faint otherwise. */
function Activity({ now, then }: { now: number; then: number }) {
  const fresh = then > 0 && now - then < TICK_MS * 1.5;
  return (
    <span
      className="size-1.5 shrink-0 rounded-full bg-clover transition-opacity duration-500"
      style={{ opacity: fresh ? 1 : 0.3 }}
    />
  );
}

function Rate({ max, value }: { max: number; value: number }) {
  return (
    <>
      <span className="relative w-10 h-[3px] shrink-0 bg-white/10">
        <span
          className="absolute inset-y-0 left-0 bg-white/50 transition-[width] duration-700"
          style={{ width: `${(value / max) * 100}%` }}
        />
      </span>
      <span>
        <span className="hidden md:inline">changes </span>
        {value.toFixed(1)}%
      </span>
    </>
  );
}

function gutter(line: number, s: Live): ReactNode {
  switch (line) {
    case 0:
      return (
        <>
          <span className="flex w-12 h-[3px] shrink-0 gap-px">
            <span className="bg-white/70" style={{ width: '41%' }} />
            <span className="bg-white/40" style={{ width: '33%' }} />
            <span className="bg-white/20" style={{ width: '26%' }} />
          </span>
          <span className="hidden md:inline">billing 41, bug 33, other 26</span>
        </>
      );
    case 4:
      return <span>mean 0.87</span>;
    case CLASSIFY:
      return (
        <>
          <Activity now={s.now} then={s.lastClassify} />
          <span className="text-white/85 tabular-nums">
            {count(s.classify)}
          </span>
          <span className="hidden md:inline">calls</span>
          <span className="hidden md:inline text-white/35">
            {ago(s.now, s.lastClassify)}
          </span>
        </>
      );
    case CLASSIFY + 1:
      return (
        <>
          <span className="tabular-nums">p50 1.42s</span>
          <span className="hidden md:inline-flex">
            <Sparkline points={s.spark} />
          </span>
        </>
      );
    case CLASSIFY + 2:
      return (
        <span className="tabular-nums" style={{ color: '#FF9A9E' }}>
          {s.errors.toFixed(1)}% errors
        </span>
      );
    case 11:
    case 12:
    case 13:
      return <Rate max={8} value={s.rules[line - 11]} />;
    case 14:
      return <span>212 tokens avg</span>;
    case TRIAGE:
      return (
        <>
          <Activity now={s.now} then={s.lastTriage} />
          <span className="text-white/85 tabular-nums">{count(s.triage)}</span>
          <span className="hidden md:inline">calls</span>
          <span className="hidden md:inline text-white/35">
            {ago(s.now, s.lastTriage)}
          </span>
        </>
      );
    case TRIAGE + 1:
      return <span className="tabular-nums">p50 4.8s</span>;
    case 21:
      return <span>4.0 per run</span>;
    case 22:
      return (
        <>
          <span className="relative w-10 h-[3px] shrink-0 bg-white/10">
            <span className="absolute inset-y-0 left-0 w-1/3 bg-white/50" />
          </span>
          <span>true 33%</span>
        </>
      );
    case 26:
      return <span>1.3 avg</span>;
    default:
      return null;
  }
}

export default function HeroGutter() {
  const [live, setLive] = useState(START);

  useEffect(() => {
    const now = Date.now();
    setLive((s) => ({
      ...s,
      lastClassify: now - 2000,
      lastTriage: now - 9000,
      now,
    }));
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const id = setInterval(() => setLive((s) => next(s, Date.now())), TICK_MS);
    return () => clearInterval(id);
  }, []);

  return (
    <figure
      aria-label="An editor with a BAML file. Beside the code, a gutter shows how it behaves in production: calls, latency, and errors for each function, how often each prompt rule changes the answer, and how the labels split."
      className="relative pb-16"
    >
      <div className="overflow-hidden rounded-lg border border-white/12 bg-[#0b0b0d]">
        <div className="flex items-stretch h-10 border-b border-white/10 text-[13px]">
          <span className="flex items-center px-4 border-r border-white/10 bg-white/[0.04] text-white/85">
            triage.baml
          </span>
          <span className="flex-1" />
          <span className="hidden sm:flex items-center gap-2 px-4 border-l border-white/10 font-mono text-[12px] text-white/50">
            Production, last 24 hours
          </span>
        </div>

        <div className="flex">
          <div className="flex-1 min-w-0 overflow-x-auto py-4">
            <Code lines={LINES} prose={PROSE} />
          </div>
          <div className="shrink-0 w-[118px] md:w-[320px] py-4 border-l border-white/10 bg-white/[0.02]">
            {LINES.map((_, i) => (
              <div
                className="flex items-center gap-2.5 h-[22px] md:h-6 px-3 md:px-4 overflow-hidden whitespace-nowrap font-mono text-[11px] md:text-[12px] text-white/50"
                // biome-ignore lint/suspicious/noArrayIndexKey: one gutter row per source line
                key={i}
              >
                {gutter(i, live)}
              </div>
            ))}
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-x-5 gap-y-1 px-4 py-2 border-t border-white/10 font-mono text-[11.5px] md:text-[12px] text-white/45">
          <span className="flex items-center gap-2">
            <span
              className="size-1.5 rounded-full"
              style={{ background: 'var(--color-clover-core)' }}
            />
            Connected to production
          </span>
          <span>v42, deployed 3h ago</span>
          <span className="hidden md:inline">
            baml check: <span className="text-white/70">0 errors</span>
          </span>
          <span className="hidden md:inline">
            baml test: <span className="text-white/70">9 passed</span>
          </span>
          <span className="ml-auto hidden sm:inline" style={{ color: RED }}>
            {live.errors > 1 ? 'Errors above 1%' : ''}
          </span>
        </div>
      </div>
    </figure>
  );
}
