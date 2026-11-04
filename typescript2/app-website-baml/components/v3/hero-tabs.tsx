'use client';

import { Fragment, type ReactNode, useEffect, useRef, useState } from 'react';
import { Button, LINKS } from '../ui';
import { Highlight, Prose, proseLines } from './code';
import { Alarm } from './hero-video';
import { HIGHLIGHT, RED } from './story';

// The whole hero in one version: the headline and buttons on the left, and on the right one BAML file
// with a terminal over its corner. The tabs are what happens once the code is in production: a live
// dashboard, one call's trace, a query, and the alerts. Each tab runs its command in the terminal, and
// as each row of output appears, the line of code it is about lights up. The file itself never changes.
// The tabs advance on their own, can be clicked, and motion can be paused.

// triage.baml as baml fmt writes it.
const FILE = [
  'type Label = "billing" | "bug" | "other";',
  '',
  'function classify(ticket: string) -> Label {',
  '    client: "openai/gpt-6-luna"',
  '    prompt: `',
  '        Classify this support ticket.',
  // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, shown as text
  '        ${ticket}',
  '    `',
  '}',
  '',
  'function route(ticket: string) -> string {',
  '    let label = classify(ticket);',
  '    if (label == "bug") {',
  '        return "engineering";',
  '    }',
  '    return "support";',
  '}',
];
const PROSE = proseLines(FILE);

/**
 * A highlight on part of a line. `row` is the output row that lights it up; without one it is lit for
 * the whole tab. `note` is a small count drawn after the highlight.
 */
export type Mark = {
  bad?: boolean;
  line: number;
  note?: string;
  row?: number;
  text: string;
};

type Output = 'alerts' | 'dashboard' | 'labels' | 'trace';

const TABS: {
  /** Alarm icons drawn at the end of a function's first line, lit by an output row. */
  alarms?: { firing: boolean; line: number; row: number }[];
  command: string;
  marks: Mark[];
  name: string;
  out: Output;
}[] = [
  {
    command: `baml query --watch "SELECT strftime('%H:%M', start_time) AS minute, count(*) AS calls, median(duration) AS p50 FROM spans WHERE span_name = 'route' GROUP BY minute"`,
    marks: [{ line: 10, text: 'route' }],
    name: 'Dashboard',
    out: 'dashboard',
  },
  {
    command: `baml query "SELECT span_name, start_time, duration, output_value FROM spans WHERE process_id = 'p_7c1f'"`,
    marks: [
      { line: 10, row: 1, text: 'route' },
      { line: 11, row: 2, text: 'classify(ticket)' },
      { line: 3, row: 3, text: '"openai/gpt-6-luna"' },
      { line: 13, row: 4, text: '"engineering"' },
    ],
    name: 'Traces',
    out: 'trace',
  },
  {
    command: `baml query "SELECT output_value AS label, count(*) AS calls FROM spans WHERE span_name = 'classify' GROUP BY label"`,
    marks: [
      { line: 2, text: 'classify' },
      { line: 0, note: '412', row: 1, text: '"billing"' },
      { line: 0, note: '318', row: 2, text: '"bug"' },
      { bad: true, line: 0, note: '31', row: 3, text: '"other"' },
    ],
    name: 'Query',
    out: 'labels',
  },
  {
    alarms: [
      { firing: true, line: 2, row: 1 },
      { firing: false, line: 10, row: 2 },
    ],
    command: 'baml alerts',
    marks: [{ bad: true, line: 0, row: 1, text: '"other"' }],
    name: 'Alerts',
    out: 'alerts',
  },
];

const TAB_MS = 6000;
const TYPE_AT = 250;
const TYPE_MS = 650;
const OUT_AT = 1100;
const ROW_MS = 260;
/** How often the dashboard's watch refreshes, in the demo's time. */
const WATCH_MS = 2000;

/** Output rows shown so far. */
function rowsAt(elapsed: number) {
  return Math.max(0, Math.floor((elapsed - OUT_AT) / ROW_MS) + 1);
}

export function Line({
  line,
  marks,
  prose,
}: {
  line: string;
  marks: Mark[];
  prose: boolean;
}) {
  const Text = prose ? Prose : Highlight;
  const found = marks
    .map((m) => ({ ...m, index: line.indexOf(m.text) }))
    .filter((m) => m.index >= 0)
    .sort((a, b) => a.index - b.index);
  const parts: ReactNode[] = [];
  let at = 0;
  for (const m of found) {
    parts.push(<Text key={`t${m.index}`} line={line.slice(at, m.index)} />);
    const color = m.bad ? RED : HIGHLIGHT;
    parts.push(
      <Fragment key={`m${m.index}`}>
        <span
          className="v3-enter inline-block rounded-[3px]"
          style={{
            background: `color-mix(in srgb, ${color} 20%, transparent)`,
            boxShadow: `inset 0 -2px 0 ${color}`,
          }}
        >
          <Text line={m.text} />
        </span>
        {m.note && (
          <span
            className="v3-enter hidden sm:inline ml-1 text-[10.5px] tabular-nums"
            style={{ color }}
          >
            {m.note}
          </span>
        )}
      </Fragment>,
    );
    at = m.index + m.text.length;
  }
  parts.push(<Text key="end" line={line.slice(at)} />);
  return <>{parts}</>;
}

function Source({ elapsed, tab }: { elapsed: number; tab: number }) {
  const { alarms = [], marks } = TABS[tab];
  const rows = rowsAt(elapsed);
  const lit = marks.filter((m) => m.row === undefined || rows > m.row);
  return (
    <div className="rounded-xl border border-white/10 bg-[#0b0b0d] overflow-hidden">
      <p className="h-9 flex items-center px-4 border-b border-white/[0.08] font-mono text-[12px] text-white/50">
        triage.baml
      </p>
      <div className="py-3 overflow-hidden font-mono text-[10.5px] leading-[18px] sm:text-[12.5px] sm:leading-[20px] text-[#BDBDC4]">
        {FILE.map((line, i) => {
          const alarm = alarms.find((a) => a.line === i && rows > a.row);
          return (
            <div
              className="flex items-center pr-4"
              // biome-ignore lint/suspicious/noArrayIndexKey: the file's lines keep their positions
              key={i}
            >
              <span className="shrink-0 w-8 sm:w-11 pr-3 sm:pr-4 text-right text-white/20 select-none">
                {i + 1}
              </span>
              <span className="whitespace-pre min-w-0" key={tab}>
                <Line
                  line={line}
                  marks={lit.filter((m) => m.line === i)}
                  prose={PROSE.has(i)}
                />
              </span>
              {alarm && (
                <span className="v3-enter ml-2 shrink-0">
                  <Alarm firing={alarm.firing} />
                </span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// Calls per minute to route, oldest first. The dashboard shows 28 at a time and moves one minute along
// with each refresh.
const CALLS = [
  24, 27, 22, 30, 33, 29, 35, 31, 28, 34, 38, 36, 32, 39, 41, 37, 33, 36, 40,
  44, 39, 35, 38, 42, 46, 41, 37, 43, 45, 40,
];
const P50 = [1.3, 1.4, 1.3];
const SHOWN = 28;

function Dashboard({ elapsed, rows }: { elapsed: number; rows: number }) {
  const ticks = Math.min(
    CALLS.length - SHOWN,
    Math.max(0, Math.floor((elapsed - OUT_AT) / WATCH_MS)),
  );
  const series = CALLS.slice(ticks, ticks + SHOWN);
  const now = series.at(-1) ?? 0;
  const max = Math.max(...CALLS);
  const tiles = [
    { label: 'Calls', value: String(now) },
    { label: 'p50', value: `${P50[ticks]}s` },
    { label: 'Failed', value: '0' },
  ];
  return (
    <div className="mt-2">
      {rows > 0 && (
        <div className="v3-enter grid grid-cols-3 gap-2">
          {tiles.map((t) => (
            <div
              className="rounded-md border border-white/10 px-2.5 py-1.5"
              key={t.label}
            >
              <p className="truncate text-[10px] sm:text-[11px] leading-[14px] text-white/40">
                {t.label}
              </p>
              <p className="text-[14px] sm:text-[16px] leading-[22px] tabular-nums text-white">
                {t.value}
              </p>
            </div>
          ))}
        </div>
      )}
      {rows > 1 && (
        <div className="v3-enter mt-2.5 h-[44px] sm:h-[52px] flex items-end gap-[2px]">
          {series.map((calls, i) => (
            <span
              className="flex-1 rounded-[1px] transition-[height] duration-300"
              // biome-ignore lint/suspicious/noArrayIndexKey: bars are positions on the time axis
              key={i}
              style={{
                background: 'var(--color-clover)',
                height: `${((calls - 12) / (max - 12)) * 100}%`,
                opacity: i === series.length - 1 ? 1 : 0.45,
              }}
            />
          ))}
        </div>
      )}
      {rows > 2 && (
        <p className="v3-enter mt-1.5 flex items-center gap-2 text-white/40">
          <span className="size-1.5 rounded-full bg-(--color-pasture-200) animate-pulse" />
          Watching. Refreshed 14:32:{String(4 + ticks * 2).padStart(2, '0')}
        </p>
      )}
    </div>
  );
}

// One call to route: the function, the classify call inside it, and the model request inside that.
// Start and length are in ms from the start of route.
const TRACE = [
  { depth: 0, ms: 1342, name: 'route', start: 0 },
  { depth: 1, ms: 1318, name: 'classify', start: 9 },
  { depth: 2, ms: 1296, name: 'openai/gpt-6-luna', start: 15 },
];

function Trace({ rows }: { rows: number }) {
  const total = TRACE[0].ms;
  return (
    <div className="mt-1.5">
      {rows > 0 && (
        <p className="v3-enter truncate text-white/40">
          ticket{' '}
          <span className="text-white/70">
            "Export to CSV has been broken since Monday."
          </span>
        </p>
      )}
      <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] gap-x-3 items-center">
        {TRACE.slice(0, Math.max(0, rows - 1)).map((s) => (
          <Fragment key={s.name}>
            <span className="text-white/80 whitespace-pre">
              {'  '.repeat(s.depth)}
              {s.name}
            </span>
            <span className="relative h-2">
              <span
                className="v3-enter absolute inset-y-0 rounded-[2px] bg-clover/70"
                style={{
                  left: `${(s.start / total) * 100}%`,
                  width: `${(s.ms / total) * 100}%`,
                }}
              />
            </span>
            <span className="text-right tabular-nums text-white/50">
              {s.ms.toLocaleString('en-US')}ms
            </span>
          </Fragment>
        ))}
      </div>
      {rows > 4 && (
        <p className="v3-enter truncate text-white/40">
          returned{' '}
          <span className="text-(--color-pasture-200)">"engineering"</span>
        </p>
      )}
    </div>
  );
}

const LABELS = [
  { calls: 412, label: 'billing' },
  { calls: 318, label: 'bug' },
  { calls: 31, label: 'other' },
];

function Labels({ rows }: { rows: number }) {
  if (rows === 0) return null;
  return (
    <div className="mt-1.5 grid grid-cols-[auto_auto_minmax(0,1fr)] gap-x-4 items-center">
      <span className="text-white/35">label</span>
      <span className="text-white/35 text-right">calls</span>
      <span />
      {LABELS.slice(0, rows - 1).map((row) => {
        const color = row.label === 'other' ? RED : undefined;
        return (
          <Fragment key={row.label}>
            <span className="text-white/75" style={{ color }}>
              {row.label}
            </span>
            <span
              className="text-right tabular-nums text-white/75"
              style={{ color }}
            >
              {row.calls}
            </span>
            <span
              className="v3-enter h-2 rounded-[2px]"
              style={{
                background: color ?? 'var(--color-clover)',
                opacity: color ? 1 : 0.6,
                width: `${Math.max(2, (row.calls / 412) * 100)}%`,
              }}
            />
          </Fragment>
        );
      })}
    </div>
  );
}

const ALERTS = [
  {
    firing: true,
    fn: 'classify',
    name: 'other_labels',
    now: '4.1%',
    rule: 'other above 2%',
  },
  {
    firing: false,
    fn: 'route',
    name: 'slow_route',
    now: '1.3s',
    rule: 'p95 above 3s',
  },
];

function Alerts({ rows }: { rows: number }) {
  return (
    <div className="mt-1.5">
      <div className="grid grid-cols-[auto_auto_minmax(0,1fr)_auto] gap-x-3 sm:gap-x-4">
        {rows > 0 && (
          <>
            <span className="text-white/35">alert</span>
            <span className="text-white/35">function</span>
            <span className="text-white/35 truncate">rule</span>
            <span className="text-white/35 text-right">now</span>
          </>
        )}
        {ALERTS.slice(0, Math.max(0, rows - 1)).map((a) => {
          const color = a.firing ? RED : 'var(--color-pasture-200)';
          return (
            <Fragment key={a.name}>
              <span className="flex items-center gap-1.5" style={{ color }}>
                <span
                  className="size-1.5 rounded-full"
                  style={{ background: color }}
                />
                {a.name}
              </span>
              <span className="text-white/75">{a.fn}</span>
              <span className="text-white/60 truncate">{a.rule}</span>
              <span className="text-right tabular-nums" style={{ color }}>
                {a.now}
              </span>
            </Fragment>
          );
        })}
      </div>
      {rows > 3 && (
        <p className="v3-enter mt-2 text-white/60">
          <span style={{ color: RED }}>other_labels is firing.</span> 31 of 761
          calls in the last hour were labelled other.
        </p>
      )}
    </div>
  );
}

function Terminal({ elapsed, tab }: { elapsed: number; tab: number }) {
  const { command, out } = TABS[tab];
  const typed = Math.round(
    Math.min(1, Math.max(0, (elapsed - TYPE_AT) / TYPE_MS)) * command.length,
  );
  const rows = rowsAt(elapsed);
  const done = typed === command.length;
  return (
    <div
      className="rounded-xl border border-white/15 bg-[#0e0e10] overflow-hidden"
      style={{ boxShadow: '0 24px 60px -20px rgb(0 0 0 / 0.9)' }}
    >
      <div className="relative h-9 flex items-center px-3.5 border-b border-white/[0.08]">
        <span className="flex gap-1.5">
          {[0, 1, 2].map((d) => (
            <span className="size-2.5 rounded-full bg-white/15" key={d} />
          ))}
        </span>
        <span className="absolute inset-x-0 text-center font-mono text-[12px] text-white/45 pointer-events-none">
          ~/triage
        </span>
      </div>
      <div className="h-[184px] sm:h-[204px] px-4 py-3 overflow-hidden font-mono text-[11px] leading-[19px] sm:text-[12.5px] sm:leading-[21px]">
        <p className="truncate text-white/90">
          <span className="text-clover">$</span> {command.slice(0, typed)}
          {!done && (
            <span className="inline-block w-[7px] h-[13px] ml-px align-[-2px] bg-white/80" />
          )}
        </p>
        {out === 'dashboard' && <Dashboard elapsed={elapsed} rows={rows} />}
        {out === 'trace' && <Trace rows={rows} />}
        {out === 'labels' && <Labels rows={rows} />}
        {out === 'alerts' && <Alerts rows={rows} />}
      </div>
    </div>
  );
}

function Demo() {
  const [tab, setTab] = useState(0);
  const [elapsed, setElapsed] = useState(0);
  const [paused, setPaused] = useState(false);
  const start = useRef(0);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setPaused(true);
    }
  }, []);

  useEffect(() => {
    if (paused) return;
    start.current = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      let e = now - start.current;
      if (e >= TAB_MS) {
        start.current = now;
        e = 0;
        setTab((t) => (t + 1) % TABS.length);
      }
      setElapsed(e);
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [paused]);

  // Paused, each tab shows its finished output.
  const shown = paused ? TAB_MS : elapsed;

  return (
    <div className="min-w-0">
      <div
        aria-label="Production views"
        className="flex gap-1 mb-3 font-mono text-[12.5px] sm:text-[13.5px]"
        role="tablist"
      >
        {TABS.map((t, i) => (
          <button
            aria-selected={i === tab}
            className={`focus-ring relative h-9 px-3 rounded-md cursor-pointer transition-colors ${i === tab ? 'text-white' : 'text-white/45 hover:text-white/80'}`}
            key={t.name}
            onClick={() => {
              setTab(i);
              setElapsed(0);
              start.current = performance.now();
            }}
            role="tab"
            type="button"
          >
            {t.name}
            {i === tab && (
              <span className="absolute left-3 right-3 bottom-0 h-[2px] bg-white/10 overflow-hidden">
                <span
                  className="block h-full bg-clover"
                  style={{ width: `${(shown / TAB_MS) * 100}%` }}
                />
              </span>
            )}
          </button>
        ))}
      </div>
      <div className="w-full sm:w-[88%]">
        <Source elapsed={shown} tab={tab} />
      </div>
      <div className="relative -mt-4 sm:-mt-[72px] ml-6 sm:ml-auto sm:w-[68%]">
        <Terminal elapsed={shown} tab={tab} />
      </div>
      <button
        className="focus-ring mt-4 h-8 px-3 rounded-full border border-white/12 text-[12.5px] text-white/60 hover:text-white cursor-pointer transition-colors"
        onClick={() => setPaused((p) => !p)}
        type="button"
      >
        {paused ? 'Play motion' : 'Pause motion'}
      </button>
    </div>
  );
}

export default function HeroTabs() {
  return (
    // On a wide screen the hero fills the screen below the nav (72px tall), pulled up over the
    // section's top padding, and both columns sit in its middle.
    <div className="grid lg:grid-cols-[minmax(0,1fr)_minmax(0,1.1fr)] gap-x-16 gap-y-12 items-center pb-24 lg:-mt-24 lg:py-10 lg:min-h-[calc(100svh-72px)]">
      <div>
        <h1
          className="v3-h1 text-balance"
          // .v3-h1 is sized for a full-width headline; this one shares the row with the demo.
          id="hero-title"
          style={{ fontSize: 'clamp(32px, 3.2vw, 48px)' }}
        >
          The programming language for agents
        </h1>
        <p className="max-w-[30rem] mt-5 md:mt-6 text-base md:text-lg leading-[1.45] tracking-[-0.01em] text-white/70">
          BAML records every call your code makes in production. You and your
          agent can watch it live, read a trace, query the calls, and set
          alerts, all from the terminal.
        </p>
        <div className="flex flex-wrap gap-2.5 mt-8">
          <Button href={LINKS.quickstart}>Install BAML</Button>
          <Button href={`${LINKS.quickstart}#agent`} variant="secondary">
            Build it with your agent
          </Button>
        </div>
      </div>
      <Demo />
    </div>
  );
}
