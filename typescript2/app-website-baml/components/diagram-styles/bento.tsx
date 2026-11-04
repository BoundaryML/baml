'use client';

import { useEffect, useState } from 'react';
import { Card, Frame, RED } from './kit';

// What BAML records, laid out as a dashboard: calls over time, latency, the labels classify returned, and
// errors grouped by cause. Everything ticks along slowly as if live. The numbers are example data.
const POINTS = 40;
// Rounded so the server and the browser draw the same starting chart.
const round = (v: number) => Math.round(v * 1000) / 1000;
const seed = (i: number) =>
  round(0.45 + 0.25 * Math.sin(i / 3.2) + 0.1 * Math.sin(i * 1.7));

type State = {
  bars: number[];
  calls: number;
  errors: number[];
  labels: number[];
  p95: number;
  series: number[];
};

const START: State = {
  bars: Array.from({ length: 24 }, (_, i) =>
    round(0.3 + 0.5 * Math.abs(Math.sin(i * 0.9))),
  ),
  calls: 12480,
  errors: [412, 31, 9],
  labels: [41, 33, 26],
  p95: 1.21,
  series: Array.from({ length: POINTS }, (_, i) => seed(i)),
};

function step(s: State, n: number): State {
  const nudge = (v: number) =>
    Math.min(0.95, Math.max(0.12, v + (Math.random() - 0.5) * 0.18));
  // billing moves a point at a time between 36 and 48, trading with bug.
  const billing = Math.min(
    48,
    Math.max(36, s.labels[0] + Math.round((Math.random() - 0.5) * 2)),
  );
  const shift = billing - s.labels[0];
  return {
    bars: [...s.bars.slice(1), nudge(s.bars.at(-1) ?? 0.5)],
    calls: s.calls + 4 + Math.floor(Math.random() * 9),
    errors: s.errors.map(
      (e, i) => e + (Math.random() < [0.5, 0.15, 0.05][i] ? 1 : 0),
    ),
    labels: [billing, s.labels[1] - shift, s.labels[2]],
    p95: Math.round((1.21 + Math.sin(n / 4) * 0.05) * 100) / 100,
    series: [...s.series.slice(1), seed(POINTS + n)],
  };
}

const LABELS = [
  { color: 'var(--ds-accent)', name: 'billing' },
  { color: 'var(--ds-ok)', name: 'bug' },
  { color: 'rgb(from var(--ds-ink) r g b / 0.35)', name: 'other' },
];
const ERRORS = [
  'Verdict.confidence missing',
  'Timeout on gpt-4o',
  'Label not in Label',
];

function Tile({
  children,
  className = '',
  title,
  value,
}: {
  children: React.ReactNode;
  className?: string;
  title: string;
  value?: React.ReactNode;
}) {
  return (
    <Card className={`p-4 @min-[640px]:p-5 ${className}`}>
      <p className="flex items-baseline justify-between gap-4 text-[12.5px] text-(--ds-ink)/45">
        {title}
        {value}
      </p>
      {children}
    </Card>
  );
}

export default function Bento() {
  const [s, setS] = useState(START);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    let n = 0;
    const id = setInterval(() => setS((prev) => step(prev, ++n)), 900);
    return () => clearInterval(id);
  }, []);

  const line = s.series
    .map(
      (v, i) => `${i ? 'L' : 'M'}${(i / (POINTS - 1)) * 100} ${(1 - v) * 40}`,
    )
    .join('');
  const total = s.labels.reduce((a, b) => a + b, 0);

  return (
    <Frame label="A dashboard of what BAML records: calls in the last hour with a chart, p95 latency with a histogram, the share of each label classify returned, and errors grouped by cause.">
      <div className="grid @min-[768px]:grid-cols-3 gap-3 p-3 @min-[640px]:p-6 @min-[768px]:p-8">
        <Tile
          className="@min-[768px]:col-span-2"
          title="Calls, last hour"
          value={<span className="font-mono text-(--ds-ink)/35">classify</span>}
        >
          <p className="mt-2 font-display font-medium text-[32px] @min-[768px]:text-[40px] tracking-[-0.04em] tabular-nums">
            {s.calls.toLocaleString('en-US')}
          </p>
          <svg
            aria-hidden="true"
            className="mt-3 w-full h-28"
            preserveAspectRatio="none"
            viewBox="0 0 100 40"
          >
            <path
              d={`${line}L100 40L0 40Z`}
              fill="rgb(from var(--ds-accent) r g b / 0.1)"
            />
            <path
              d={line}
              fill="none"
              stroke="var(--ds-accent)"
              strokeWidth="1.5"
              vectorEffect="non-scaling-stroke"
            />
          </svg>
        </Tile>

        <Tile title="p95 latency">
          <p className="mt-2 font-display font-medium text-[32px] @min-[768px]:text-[40px] tracking-[-0.04em] tabular-nums">
            {s.p95.toFixed(2)}
            <span className="text-[18px] text-(--ds-ink)/40">s</span>
          </p>
          <div className="flex items-end gap-[3px] h-28 mt-3">
            {s.bars.map((b, i) => (
              <span
                className="flex-1 rounded-[2px] bg-(--ds-ink)/20 transition-[height] duration-700"
                // biome-ignore lint/suspicious/noArrayIndexKey: a bar is a slot in the chart
                key={i}
                style={{ height: `${b * 100}%` }}
              />
            ))}
          </div>
        </Tile>

        <Tile title="Labels returned">
          <div className="flex h-2.5 mt-5 rounded-full overflow-hidden gap-[2px]">
            {LABELS.map((l, i) => (
              <span
                className="transition-[width] duration-700"
                key={l.name}
                style={{
                  background: l.color,
                  width: `${(s.labels[i] / total) * 100}%`,
                }}
              />
            ))}
          </div>
          <ul className="mt-5 flex flex-col gap-2.5 text-[13px]">
            {LABELS.map((l, i) => (
              <li className="flex items-center gap-2.5" key={l.name}>
                <span
                  className="size-2 rounded-full"
                  style={{ background: l.color }}
                />
                <span className="font-mono text-(--ds-ink)/75">{l.name}</span>
                <span className="ml-auto tabular-nums text-(--ds-ink)/50">
                  {Math.round((s.labels[i] / total) * 100)}%
                </span>
              </li>
            ))}
          </ul>
        </Tile>

        <Tile
          className="@min-[768px]:col-span-2"
          title="Errors, grouped by cause"
          value={<span className="text-(--ds-ink)/35">since v42</span>}
        >
          <ul className="mt-3 divide-y divide-(--ds-ink)/[0.06]">
            {ERRORS.map((e, i) => (
              <li className="flex items-center gap-3 h-12 text-[13px]" key={e}>
                <span
                  className="size-2 rounded-full shrink-0"
                  style={{
                    background: i ? 'rgb(from var(--ds-ink) r g b / 0.3)' : RED,
                  }}
                />
                <span className="font-mono text-(--ds-ink)/80 truncate">
                  {e}
                </span>
                <span className="hidden @min-[640px]:block flex-1 mx-2 h-1 rounded-full bg-(--ds-ink)/[0.05] overflow-hidden">
                  <span
                    className="block h-full rounded-full bg-(--ds-ink)/25 transition-[width] duration-700"
                    style={{ width: `${(s.errors[i] / s.errors[0]) * 100}%` }}
                  />
                </span>
                <span className="ml-auto @min-[640px]:ml-0 w-12 text-right tabular-nums text-(--ds-ink)/60">
                  {s.errors[i]}
                </span>
              </li>
            ))}
          </ul>
        </Tile>
      </div>
    </Frame>
  );
}
