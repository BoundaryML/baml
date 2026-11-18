'use client';

import {
  ArrowRight,
  Bug,
  Coins,
  type LucideIcon,
  Repeat2,
  ShieldAlert,
  Timer,
  TrendingUp,
} from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { section } from './analytics';
import { RED } from './story';
import { Cmd } from './ui';

// What you do with the data: a board of saved queries. Each one is something a team wants flagged
// (spend, a security issue, slow calls, errors, drifting output, a side effect that ran twice). The
// list on the left reads like an inbox, with each query's number beside it; the panel shows that
// number large, a chart of what the query found, and the SQL that found it. The queries use the
// columns `baml local recordings schema spans` lists. The results are example data.

const CLOVER = 'var(--color-clover)';
const LIGHT = 'var(--color-clover-100)';
const AMBER = '#f5c451';
const GREY = 'rgb(255 255 255 / 0.22)';

type Tone = 'alert' | 'report' | 'watch';

const TONES: Record<Tone, { color: string; label: string }> = {
  alert: { color: RED, label: 'Alert firing' },
  report: { color: CLOVER, label: 'Report' },
  watch: { color: AMBER, label: 'Over threshold' },
};

/** Noise in [0, 1) for point i, so the charts draw the same way on every render. */
function noise(i: number, seed = 1) {
  const x = Math.sin(i * 12.9898 + seed * 78.233) * 43758.5453;
  return x - Math.floor(x);
}

/** Rounded to a hundredth. The server and the browser can disagree in the last digits of a sine,
 *  and a number drawn straight from one would differ between the two renders. */
const round = (v: number) => Math.round(v * 100) / 100;

const W = 360;
const H = 132;

/** Bars across the chart, one per day. `hot` days are drawn in the tone's color. */
function Days({
  color,
  days,
  labels,
}: {
  color: string;
  // Each day as [the part in grey, the part in color], as shares of the chart's height.
  days: [base: number, hot: number][];
  labels: [left: string, right: string];
}) {
  const gap = 6;
  const bw = (W - gap * (days.length - 1)) / days.length;
  const top = H - 18;
  return (
    <svg aria-hidden="true" className="h-auto w-full" viewBox={`0 0 ${W} ${H}`}>
      {days.map(([base, hot], i) => {
        const x = i * (bw + gap);
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed example days
          <g key={i}>
            <rect
              fill={GREY}
              height={round(base * top)}
              rx={2}
              width={round(bw)}
              x={round(x)}
              y={round(top - base * top)}
            />
            {hot > 0 && (
              <rect
                fill={color}
                height={round(hot * top)}
                rx={2}
                width={round(bw)}
                x={round(x)}
                y={round(top - (base + hot) * top)}
              />
            )}
          </g>
        );
      })}
      <text fill="white" fillOpacity={0.45} fontSize={10} x={0} y={H - 3}>
        {labels[0]}
      </text>
      <text
        fill="white"
        fillOpacity={0.45}
        fontSize={10}
        textAnchor="end"
        x={W}
        y={H - 3}
      >
        {labels[1]}
      </text>
    </svg>
  );
}

// Two weeks of input tokens. write_reply's share is the colored part, and it grows this week.
const TOKEN_DAYS: [number, number][] = Array.from({ length: 14 }, (_, i) => [
  round(0.2 + 0.05 * noise(i, 5)),
  round(i < 7 ? 0.12 + 0.03 * noise(i, 6) : 0.2 + 0.085 * (i - 7)),
]);

// Ten days of errors. issue_refund's are the colored part, from Monday on.
const ERROR_DAYS: [number, number][] = Array.from({ length: 10 }, (_, i) => [
  round(0.1 + 0.05 * noise(i, 7)),
  round(i < 6 ? 0 : 0.35 + 0.14 * (i - 6)),
]);

/** Small labelled values under a chart. */
function Legend({ items }: { items: [name: string, value: string][] }) {
  return (
    <ul className="mt-3 flex flex-wrap gap-x-5 gap-y-1 font-mono text-[12px]">
      {items.map(([name, value], i) => (
        <li className="flex items-center gap-1.5" key={name}>
          <span className={i === 0 ? 'text-head' : 'text-ink-2'}>{name}</span>
          <span className="text-muted">{value}</span>
        </li>
      ))}
    </ul>
  );
}

/** The path a call took: the agent, the tool it called, and the host that isn't on the list. */
function Flow() {
  const node =
    'flex min-w-0 items-center gap-2 rounded-lg border px-3 py-2 font-mono text-[12.5px]';
  return (
    <div>
      <div className="flex flex-wrap items-center gap-2">
        <span className={`${node} border-rule text-ink-2`}>support_agent</span>
        {/* Each arrow stays with the step it points to, so a wrapped row never ends on one. */}
        <span className="flex min-w-0 items-center gap-2">
          <ArrowRight aria-hidden className="size-4 shrink-0 text-muted" />
          <span className={`${node} border-rule text-ink-2`}>http.get</span>
        </span>
        <span className="flex min-w-0 items-center gap-2">
          <ArrowRight
            aria-hidden
            className="size-4 shrink-0"
            style={{ color: RED }}
          />
          <span
            className={node}
            style={{
              background: 'rgb(229 72 77 / 0.1)',
              borderColor: 'rgb(229 72 77 / 0.6)',
              color: RED,
            }}
          >
            <ShieldAlert aria-hidden className="size-4 shrink-0" />
            pastebin.com
          </span>
        </span>
      </div>
      <div className="mt-4 grid grid-cols-[auto_auto_minmax(0,1fr)] gap-x-5 gap-y-1.5 font-mono text-[12px]">
        {['span_id', 'start_time', 'url'].map((c) => (
          <span className="text-muted" key={c}>
            {c}
          </span>
        ))}
        {[
          ['7f3a91c2', 'Mon 6:13 PM', 'pastebin.com/raw/9xk2'],
          ['b21c04de', 'Mon 6:14 PM', 'pastebin.com/raw/9xk2'],
        ].flatMap((r) =>
          r.map((cell, j) => (
            <span
              className={`truncate ${j === 0 ? 'text-head' : 'text-ink-2'}`}
              key={`${r[0]}-${cell}-${j === 0 ? 'id' : j}`}
            >
              {cell}
            </span>
          )),
        )}
      </div>
    </div>
  );
}

const SLOWEST: [name: string, seconds: number, input: string][] = [
  ['issue_refund', 31.2, '{ order: 8841, amount: 182 }'],
  ['write_reply', 12.9, '{ ticket: "Charged twice and…" }'],
  ['classify', 9.4, '{ ticket: "Wrong plan on invoice" }'],
  ['route', 7.7, '{ label: Billing }'],
  ['lookup_customer', 6.1, '{ email: "j@acme.com" }'],
];

/** The slowest calls as bars to one scale, each with the input that caused it. */
function Slowest() {
  return (
    <ul className="grid grid-cols-1 gap-2.5 font-mono text-[12px]">
      {SLOWEST.map(([name, seconds, input], i) => (
        <li className="min-w-0" key={name}>
          <p className="flex items-baseline justify-between gap-3">
            <span
              className={`shrink-0 ${i === 0 ? 'text-head' : 'text-ink-2'}`}
            >
              {name}
            </span>
            {/* The input is the part that gives way: it shortens to an ellipsis on a narrow
                phone, where the row would otherwise push the page wider than the screen. */}
            <span className="min-w-0 truncate text-muted">{input}</span>
          </p>
          <p className="mt-1 flex items-center gap-2.5">
            <span className="h-2 flex-1 rounded-full bg-white/[0.07]">
              <span
                className="block h-full rounded-full"
                style={{
                  background: i === 0 ? CLOVER : GREY,
                  width: `${(seconds / SLOWEST[0][1]) * 100}%`,
                }}
              />
            </span>
            <span className="w-11 shrink-0 text-right text-head">
              {seconds}s
            </span>
          </p>
        </li>
      ))}
    </ul>
  );
}

// The share labelled other, hour by hour, against a 2% threshold. The y axis runs to 8%.
const SHARE = Array.from({ length: 16 }, (_, i) =>
  i < 11
    ? (0.012 + 0.006 * noise(i, 2)) / 0.08
    : (0.03 + 0.012 * (i - 11)) / 0.08,
);
const THRESHOLD = 0.02 / 0.08;
const toPath = (ys: number[], from = 0, to = ys.length - 1) =>
  ys
    .slice(from, to + 1)
    .map(
      (y, k) =>
        `${k ? 'L' : 'M'}${(((from + k) / (ys.length - 1)) * W).toFixed(1)},${(H - y * H).toFixed(1)}`,
    )
    .join('');

/** The share of tickets labelled other, crossing its threshold in the last few hours. */
function Drift() {
  const ty = H - THRESHOLD * H;
  const cross = SHARE.findIndex((s) => s > THRESHOLD);
  const cx = (cross / (SHARE.length - 1)) * W;
  const before = toPath(SHARE, 0, cross - 1);
  return (
    <svg aria-hidden="true" className="h-auto w-full" viewBox={`0 0 ${W} ${H}`}>
      <rect fill={RED} height={H} opacity={0.08} width={W - cx} x={cx} y={0} />
      <path
        d={`${before}L${cx.toFixed(1)},${H}L0,${H}Z`}
        fill={LIGHT}
        opacity={0.07}
      />
      <line
        stroke="white"
        strokeDasharray="4 4"
        strokeOpacity={0.45}
        x1={0}
        x2={W}
        y1={ty}
        y2={ty}
      />
      <text fill="white" fillOpacity={0.5} fontSize={10} x={4} y={ty - 6}>
        2% threshold
      </text>
      <path d={before} fill="none" stroke={LIGHT} strokeWidth={1.75} />
      <path
        d={toPath(SHARE, cross - 1)}
        fill="none"
        stroke={RED}
        strokeWidth={1.75}
      />
    </svg>
  );
}

// One agent run as a trace: where each span starts and how long it runs, as shares of the run.
const RUN: [name: string, from: number, width: number, second?: boolean][] = [
  ['support_agent', 0, 1],
  ['classify', 0.02, 0.14],
  ['issue_refund', 0.18, 0.62],
  ['issue_refund', 0.76, 0.2, true],
  ['write_reply', 0.82, 0.16],
];

/** One of the runs the query found: issue_refund starts again while the first call is still open. */
function Twice() {
  return (
    <ul className="grid gap-2 font-mono text-[12px]">
      {RUN.map(([name, from, width, second], i) => (
        <li
          className="grid grid-cols-[7.5rem_minmax(0,1fr)] items-center gap-3"
          // biome-ignore lint/suspicious/noArrayIndexKey: issue_refund appears twice, in order
          key={i}
        >
          <span style={{ color: second ? RED : undefined }}>
            <span
              className={second ? '' : i === 0 ? 'text-head' : 'text-ink-2'}
            >
              {name}
            </span>
          </span>
          <span className="relative h-2.5">
            <span
              className="absolute inset-y-0 rounded-[3px]"
              style={{
                background: second ? RED : i === 0 ? CLOVER : GREY,
                left: `${from * 100}%`,
                width: `${width * 100}%`,
              }}
            />
            {/* The label sits in the row, ending where the bar starts: the bar is near the right
                edge, so text after or above it ran into the query beside the chart. */}
            {second && (
              <span
                className="absolute inset-y-0 flex items-center whitespace-nowrap pr-2 text-[11px] leading-none"
                style={{ color: RED, right: `${(1 - from) * 100}%` }}
              >
                2nd call, 30s in
              </span>
            )}
          </span>
        </li>
      ))}
    </ul>
  );
}

const QUERIES: {
  chart: ReactNode;
  // What the number is, said after it.
  counts: string;
  finding: string;
  Icon: LucideIcon;
  // The one number the query comes down to.
  number: string;
  sql: string;
  title: string;
  tone: Tone;
}[] = [
  {
    chart: (
      <>
        <Days
          color={AMBER}
          days={TOKEN_DAYS}
          labels={['last week', 'this week']}
        />
        <Legend
          items={[
            ['write_reply', '41.2M'],
            ['classify', '12.8M'],
            ['route', '3.1M'],
          ]}
        />
      </>
    ),
    counts: 'input tokens this week',
    finding: 'write_reply used 3.2× the tokens it used last week.',
    Icon: Coins,
    number: '41.2M',
    sql: `SELECT span_name,
       sum(total_usage[0]['input_tokens']) AS tokens
FROM spans
GROUP BY span_name
ORDER BY tokens DESC`,
    title: 'High token usage',
    tone: 'watch',
  },
  {
    chart: <Flow />,
    counts: 'calls to an unlisted host',
    finding: 'An agent tool sent data to a host that is not on your list.',
    Icon: ShieldAlert,
    number: '2',
    sql: `SELECT span_id, start_time, input_args['url'] AS url
FROM spans
WHERE span_name = 'http.get'
  AND input_args['url'] NOT LIKE '%acme.com%'`,
    title: 'Security issue detected',
    tone: 'alert',
  },
  {
    chart: <Slowest />,
    counts: 'slowest call this week',
    finding: 'The five slowest calls, each with the input that caused it.',
    Icon: Timer,
    number: '31.2s',
    sql: `SELECT span_name, duration, input_args
FROM spans
ORDER BY duration DESC
LIMIT 5`,
    title: 'Worst latencies',
    tone: 'report',
  },
  {
    chart: (
      <>
        <Days
          color={AMBER}
          days={ERROR_DAYS}
          labels={['10 days ago', 'today']}
        />
        <Legend
          items={[
            ['issue_refund', '214'],
            ['lookup_customer', '37'],
            ['write_reply', '9'],
          ]}
        />
      </>
    ),
    counts: 'errors since Monday',
    finding: 'Nearly all of them are in issue_refund.',
    Icon: Bug,
    number: '214',
    sql: `SELECT span_name, count(*) AS errors
FROM spans
WHERE status = 'error'
GROUP BY span_name
ORDER BY errors DESC`,
    title: 'Errors by function',
    tone: 'watch',
  },
  {
    chart: <Drift />,
    counts: 'of tickets labelled "other"',
    finding: 'classify crossed its 2% threshold five hours ago.',
    Icon: TrendingUp,
    number: '6.2%',
    sql: `SELECT avg(output_value = 'other')
FROM spans
WHERE span_name = 'classify'
GROUP BY hour`,
    title: 'Model output drifting',
    tone: 'alert',
  },
  {
    chart: <Twice />,
    counts: 'agent runs',
    finding: 'issue_refund ran twice inside one run. This is one of them.',
    Icon: Repeat2,
    number: '23',
    sql: `SELECT parent_span_id, count(*) AS calls
FROM spans
WHERE span_name = 'issue_refund'
GROUP BY parent_span_id
HAVING count(*) > 1`,
    title: 'Tool called twice',
    tone: 'alert',
  },
];

type Query = (typeof QUERIES)[number];

/** The query's icon on a tile tinted with its tone. */
function Mark({ q, small }: { q: Query; small?: boolean }) {
  const color = TONES[q.tone].color;
  return (
    <span
      className={`flex shrink-0 items-center justify-center rounded-lg ${small ? 'size-8' : 'size-10'}`}
      style={{
        background: `color-mix(in srgb, ${color} 14%, transparent)`,
        color,
      }}
    >
      <q.Icon aria-hidden className={small ? 'size-4' : 'size-5'} />
    </span>
  );
}

/** One query in full: its number, what it found, the chart, and the SQL. */
function Detail({ q }: { q: Query }) {
  const tone = TONES[q.tone];
  return (
    <>
      <div className="flex items-center gap-3">
        <Mark q={q} />
        {/* On a phone the status sits under the title: side by side, the title broke onto three
            lines and the status ran past the card's padding. */}
        <div className="flex min-w-0 flex-1 flex-col gap-1 sm:flex-row sm:items-center sm:gap-3">
          <h3 className="font-display text-[20px] font-medium leading-snug tracking-[-0.02em] text-head">
            {q.title}
          </h3>
          <span
            className="inline-flex items-center gap-1.5 whitespace-nowrap font-mono text-[11px] uppercase tracking-wider sm:ml-auto"
            style={{ color: tone.color }}
          >
            <span className="size-1.5 rounded-full bg-current" />
            {tone.label}
          </span>
        </div>
      </div>
      <p className="mt-5 flex flex-wrap items-baseline gap-x-2.5">
        <span className="font-display text-[36px] font-medium leading-none tracking-[-0.03em] text-head">
          {q.number}
        </span>
        <span className="text-[16px] text-ink-2">{q.counts}</span>
      </p>
      <p className="mt-2 text-[16px] leading-snug text-ink-2">{q.finding}</p>
      <div className="mt-6 grid grid-cols-1 gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(0,0.95fr)]">
        <div className="min-w-0 self-center">{q.chart}</div>
        <pre className="code-pane min-w-0 self-start rounded-lg border border-rule bg-black/40 p-4 font-mono text-[12px] leading-[1.7] text-clover/90">
          {q.sql}
        </pre>
      </div>
    </>
  );
}

// How many queries a phone shows in full. Six stacked ran to three thousand pixels.
const PHONE = 3;

/** The board: the six queries listed on the left and the picked one in full on the right. On a
 *  phone there is no room for both, so the first three are shown in full, one under the other, and
 *  the rest are named. */
function Board() {
  const [at, setAt] = useState(0);
  return (
    <>
      <div className="hidden overflow-hidden rounded-xl border border-rule bg-raised md:grid md:grid-cols-[minmax(0,19rem)_minmax(0,1fr)]">
        <div
          aria-label="Saved queries"
          className="flex flex-col gap-0.5 border-r border-rule p-2"
          role="tablist"
        >
          {QUERIES.map((q, i) => (
            <button
              aria-controls="query-panel"
              aria-selected={i === at}
              className={`focus-ring flex cursor-pointer items-center gap-3 rounded-lg px-3 py-2.5 text-left transition-colors ${i === at ? 'bg-white/[0.07]' : 'hover:bg-white/[0.03]'}`}
              key={q.title}
              onClick={() => setAt(i)}
              role="tab"
              type="button"
            >
              <Mark q={q} small />
              <span
                className={`min-w-0 flex-1 text-[15px] ${i === at ? 'text-head' : 'text-ink-2'}`}
              >
                {q.title}
              </span>
              <span
                className="font-mono text-[12px]"
                style={{ color: TONES[q.tone].color }}
              >
                {q.number}
              </span>
            </button>
          ))}
        </div>
        <div
          className="min-w-0 p-6 md:min-h-[37.5rem] lg:min-h-[28.75rem] lg:p-8"
          id="query-panel"
          role="tabpanel"
        >
          {/* Keyed by the query, so the panel's entrance plays again each time one is picked. */}
          <div className="query-in" key={at}>
            <Detail q={QUERIES[at]} />
          </div>
        </div>
      </div>

      <ul className="grid gap-3 md:hidden">
        {QUERIES.slice(0, PHONE).map((q) => (
          <li
            className="min-w-0 rounded-xl border border-rule bg-raised p-5"
            key={q.title}
          >
            <Detail q={q} />
          </li>
        ))}
        <li className="text-[14px] leading-snug text-ink-2">
          Also on the board:{' '}
          {QUERIES.slice(PHONE)
            .map((q) => q.title)
            .join(', ')}
          .
        </li>
      </ul>
    </>
  );
}

export default function Observe() {
  return (
    <section
      {...section('observe')}
      aria-labelledby="observe"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 text-head max-w-[22em]" id="observe">
          Turn your traces to a queryable{' '}
          <span className="whitespace-nowrap">data-lake</span>, with zero setup.
        </h2>
        <p className="mt-4 max-w-[40rem] v4-body text-ink-2">
          Query your program traces and profiles to generate any custom
          dashboard using <Cmd>baml query</Cmd>, or create alerts with{' '}
          <Cmd>baml alerts</Cmd> to keep track of critical metrics.
        </p>
        <div className="mt-10 md:mt-12">
          <Board />
        </div>
      </div>
    </section>
  );
}
