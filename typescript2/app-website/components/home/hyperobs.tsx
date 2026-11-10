'use client';

import { type CSSProperties, type ReactNode, useEffect, useState } from 'react';
import { SiClaude, SiGithub } from 'react-icons/si';
import { section } from '@/components/home/analytics';
import { Highlight } from '@/components/home/code';
import { LAMB_PATHS } from '@/components/home/lamb';
import { Panel } from '@/components/home/phase';
import { Hyper, Pinned, sweep } from '@/components/home/pinned';
import { ramp } from '@/components/home/scroll';

// ---------- the code and the traces ----------

type Kind = 'logic' | 'span' | 'metric' | 'hist' | 'log';
// `at` is the order the tracing piles on. `d` is the final indent, `d0` the indent before any tracing.
type Line = { at: number; d: number; d0?: number; k: Kind; s: string };

const TS: Line[] = [
  { at: 1, d: 0, k: 'span', s: 'const tracer = trace.getTracer("support");' },
  {
    at: 3,
    d: 0,
    k: 'metric',
    s: 'const replies = meter.createCounter("replies");',
  },
  {
    at: 3,
    d: 0,
    k: 'hist',
    s: 'const latency = meter.createHistogram("reply_ms");',
  },
  { at: 0, d: 0, k: 'logic', s: 'async function draftReply(ticket) {' },
  {
    at: 1,
    d: 1,
    k: 'span',
    s: 'return tracer.startActiveSpan("draftReply", async (span) => {',
  },
  { at: 3, d: 2, k: 'hist', s: 'const start = performance.now();' },
  { at: 2, d: 2, k: 'span', s: 'try {' },
  { at: 1, d: 3, k: 'span', s: 'span.setAttribute("ticket.id", ticket.id);' },
  {
    at: 0,
    d: 3,
    d0: 1,
    k: 'logic',
    s: 'const label = await classify(ticket);',
  },
  { at: 0, d: 3, d0: 1, k: 'logic', s: 'if (label === "Spam") return "";' },
  {
    at: 0,
    d: 3,
    d0: 1,
    k: 'logic',
    s: 'const reply = await writeReply(ticket, label);',
  },
  { at: 3, d: 3, k: 'metric', s: 'replies.add(1, { status: "ok" });' },
  {
    at: 4,
    d: 3,
    k: 'log',
    s: 'logger.info("drafted", { traceId: span.spanContext().traceId });',
  },
  { at: 0, d: 3, d0: 1, k: 'logic', s: 'return reply;' },
  { at: 2, d: 2, k: 'span', s: '} catch (err) {' },
  { at: 2, d: 3, k: 'span', s: 'span.recordException(err);' },
  {
    at: 2,
    d: 3,
    k: 'span',
    s: 'span.setStatus({ code: SpanStatusCode.ERROR });',
  },
  { at: 3, d: 3, k: 'metric', s: 'replies.add(1, { status: "error" });' },
  { at: 2, d: 3, k: 'span', s: 'throw err;' },
  { at: 2, d: 2, k: 'span', s: '} finally {' },
  { at: 3, d: 3, k: 'hist', s: 'latency.record(performance.now() - start);' },
  { at: 1, d: 3, k: 'span', s: 'span.end();' },
  { at: 2, d: 2, k: 'span', s: '}' },
  { at: 1, d: 1, k: 'span', s: '});' },
  { at: 0, d: 0, k: 'logic', s: '}' },
];

const BAML = [
  'function draft_reply(ticket: Ticket) -> string {',
  '    let label = classify(ticket);',
  '    match (label) {',
  '        Label.Spam => "",',
  '        _ => write_reply(ticket, label),',
  '    }',
  '}',
];

type Span = {
  depth: number;
  gc?: boolean;
  ms: number;
  name: string;
  start: number;
  auto?: boolean;
  // Instead of a duration, for something that isn't a span.
  kind?: string;
  // The step of the scroll build that adds it.
  step?: number;
};

const TOTAL = 1842;

// The BAML side's trace, as tall as the left side. Only what the runtime records: a timing for every
// function call, including both AI functions it calls and their HTTP requests with their events, and
// garbage collector runs with their stats. `classify` and `write_reply` are AI functions whose source
// isn't shown. The timings are illustrative.
const DEEP: Span[] = [
  { depth: 0, ms: 1842, name: 'draft_reply', start: 0 },
  { depth: 1, ms: 612.3, name: 'classify', start: 0.4 },
  { depth: 2, ms: 0.3, name: 'classify.render_prompt', start: 0.5 },
  { depth: 2, ms: 0.1, name: 'classify.build_request', start: 0.8 },
  { depth: 2, ms: 611.2, name: 'ai.Client.invoke', start: 0.9 },
  {
    depth: 3,
    ms: 610.8,
    name: 'POST api.openai.com/v1/chat/completions',
    start: 1.2,
  },
  { depth: 4, kind: '200', ms: 0, name: 'connection', start: 402 },
  { depth: 4, kind: 'event', ms: 0, name: 'data', start: 598 },
  { depth: 4, kind: 'event', ms: 0, name: 'end', start: 611 },
  { depth: 2, ms: 0.4, name: 'classify.parse', start: 612.1 },
  { depth: 1, ms: 1228.6, name: 'write_reply', start: 613.1 },
  { depth: 2, ms: 0.4, name: 'write_reply.render_prompt', start: 613.2 },
  { depth: 2, ms: 0.1, name: 'write_reply.build_request', start: 613.6 },
  { depth: 2, ms: 1227.4, name: 'ai.Client.invoke', start: 613.7 },
  {
    depth: 3,
    ms: 1227,
    name: 'POST api.openai.com/v1/chat/completions',
    start: 614,
  },
  { depth: 4, kind: '200', ms: 0, name: 'connection', start: 980 },
  { depth: 4, kind: 'event', ms: 0, name: 'data', start: 1100 },
  { depth: 4, kind: 'event', ms: 0, name: 'data', start: 1500 },
  { depth: 4, kind: 'event', ms: 0, name: 'end', start: 1840 },
  { depth: 2, ms: 0.5, name: 'write_reply.parse', start: 1841 },
  { depth: 0, gc: true, ms: 6.1, name: 'garbage collector', start: 1834 },
  {
    depth: 1,
    gc: true,
    kind: 'major',
    ms: 0,
    name: 'collection_level',
    start: 1834,
  },
  {
    depth: 1,
    gc: true,
    kind: '1,204',
    ms: 0,
    name: 'reclaimed_slots',
    start: 1834,
  },
  { depth: 1, gc: true, kind: '3,817', ms: 0, name: 'live_slots', start: 1834 },
  {
    depth: 1,
    gc: true,
    kind: '96 KB',
    ms: 0,
    name: 'live_payload_bytes',
    start: 1834,
  },
];

// Which code line each part of the BAML trace comes from, so hovering one lights up the other: the
// function's own span, and each AI call with everything under it. The garbage collector and the
// match have no line of their own.
const DEEP_GROUPS = (() => {
  let group: string | null = null;
  return DEEP.map((s) => {
    if (s.gc) return null;
    if (s.depth === 0) {
      group = null;
      return s.name;
    }
    if (s.depth === 1) group = s.name;
    return group;
  });
})();
const BAML_GROUPS = [
  'draft_reply',
  'classify',
  null,
  null,
  'write_reply',
  null,
  'draft_reply',
];

// What the TypeScript records, built up as you scroll: each row arrives with the step that adds it.
// The HTTP span is there from the start, from OpenTelemetry's auto-instrumentation.
const BUILT: Span[] = [
  { depth: 0, ms: 1842, name: 'draftReply', start: 0, step: 1 },
  { auto: true, depth: 1, ms: 611.4, name: 'http.post', start: 0.9, step: 0 },
  {
    auto: true,
    depth: 1,
    ms: 1227.2,
    name: 'http.post',
    start: 613.8,
    step: 0,
  },
  { depth: 1, kind: 'error', ms: 0, name: 'exception', start: 1841, step: 2 },
  { depth: 1, kind: 'counter', ms: 0, name: 'replies', start: 1841, step: 3 },
  {
    depth: 1,
    kind: 'histogram',
    ms: 0,
    name: 'reply_ms',
    start: 1842,
    step: 3,
  },
  { depth: 1, kind: 'log', ms: 0, name: 'drafted', start: 1841, step: 4 },
];

// Every row is this tall, sized by the figure so the whole thing fits in the window.
const ROW = 'var(--r)';

const TS_TOKENS =
  /("[^"]*"|\b(?:const|async|function|return|await|try|catch|finally|throw|if)\b|\b\w+(?=\())/;

function TsText({ line }: { line: string }) {
  return (
    <>
      {line.split(TS_TOKENS).map((part, i) => {
        const key = `${i}-${part}`;
        if (i % 2 === 0) return <span key={key}>{part}</span>;
        const tone = part.startsWith('"')
          ? 'text-(--color-pasture-200)'
          : /^(const|async|function|return|await|try|catch|finally|throw|if)$/.test(
                part,
              )
            ? 'text-clover'
            : 'text-white';
        return (
          <span className={tone} key={key}>
            {part}
          </span>
        );
      })}
    </>
  );
}

/** The TypeScript listing. `level` is how much tracing has piled on, from 0 to 4; fractions ease a
 *  line in. The original lines keep a clover tint, so they stay findable, and the tracing is muted. */
function TsCode({ level }: { level: number }) {
  return (
    <div className="py-3 font-mono text-[12.5px] text-[#BDBDC4]">
      {TS.map((l, i) => {
        const shown = l.at === 0 ? 1 : ramp(level, l.at - 1, l.at);
        const indent = level > 0.5 ? l.d : (l.d0 ?? l.d);
        return (
          <div
            className={`flex items-center overflow-hidden whitespace-pre px-4 transition-[padding] duration-300 ${l.k === 'logic' ? 'bg-clover/14 text-white' : 'text-white/35'}`}
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed listing
            key={i}
            style={{
              height: `calc(${ROW} * ${shown})`,
              opacity: shown,
              paddingLeft: `calc(1rem + ${indent * 2}ch)`,
            }}
          >
            {l.k === 'logic' ? <TsText line={l.s} /> : l.s}
          </div>
        );
      })}
    </div>
  );
}

/** A group being hovered, and how to set it, shared by the BAML code and its trace. */
type Hover = { hot: string | null; onHot: (group: string | null) => void };

/** How much a row dims while another group is hovered. */
const dim = (hot: string | null, group: string | null) =>
  hot !== null && group !== hot ? 0.35 : 1;

function BamlCode({ hot, onHot }: Hover) {
  return (
    <div className="py-3 font-mono text-[12.5px] text-[#BDBDC4]">
      {BAML.map((l, i) => {
        const group = BAML_GROUPS[i];
        return (
          <div
            className={`flex items-center whitespace-pre px-4 text-white transition-[background-color,opacity] duration-150 ${hot !== null && group === hot ? 'bg-clover/30' : 'bg-clover/14'}`}
            key={l}
            onMouseEnter={() => onHot(group)}
            onMouseLeave={() => onHot(null)}
            style={{ height: ROW, opacity: dim(hot, group) }}
          >
            <span>
              <Highlight line={l} />
            </span>
          </div>
        );
      })}
    </div>
  );
}

/** One trace as a waterfall. Rows take up no room until they arrive, so the panel expands as they do:
 *  `steps` reveals the rows tagged with a step, `shown` reveals the rest from the top. */
function Waterfall({
  groups,
  hover,
  rows,
  shown = rows.length,
  steps,
}: {
  /** The code line each row comes from, for the hover highlight. */
  groups?: (string | null)[];
  hover?: Hover;
  rows: Span[];
  shown?: number;
  steps?: number;
}) {
  return (
    <div className="py-3 px-4 font-mono text-[12px]">
      {rows.map((s, i) => {
        const on =
          steps !== undefined && s.step !== undefined
            ? ramp(steps, s.step - 1, s.step)
            : Math.min(1, Math.max(0, shown - i));
        const group = groups?.[i] ?? null;
        const hot = hover?.hot ?? null;
        return (
          <div
            className={`-mx-2 grid grid-cols-[minmax(0,52%)_minmax(0,1fr)_64px] gap-x-3 items-center overflow-hidden rounded-[3px] px-2 transition-[background-color,opacity] duration-150 ${hot !== null && group === hot ? 'bg-white/8' : ''}`}
            key={`${s.name}-${s.start}`}
            onMouseEnter={hover && (() => hover.onHot(group))}
            onMouseLeave={hover && (() => hover.onHot(null))}
            style={{
              height: `calc(${ROW} * ${on})`,
              opacity: on * dim(hot, group),
            }}
          >
            <span
              className={`truncate ${s.gc ? 'text-white/45' : 'text-white/80'}`}
              style={{ paddingLeft: `${s.depth}ch` }}
            >
              {s.name}
              {s.auto && <span className="text-white/35"> auto</span>}
            </span>
            <span className="relative h-full">
              <span
                className={`absolute top-[5px] bottom-[5px] rounded-[2px] ${s.gc ? 'bg-white/25' : s.depth === 0 ? 'bg-clover' : 'bg-clover/60'}`}
                style={{
                  left: `${(s.start / TOTAL) * 100}%`,
                  minWidth: 3,
                  width: `${(s.ms / TOTAL) * 100}%`,
                }}
              />
            </span>
            <span className="text-right tabular-nums text-white/45">
              {s.kind ?? `${s.ms.toFixed(1)}ms`}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function Stat({
  label,
  tone,
  value,
}: {
  label: string;
  tone?: string;
  value: string;
}) {
  return (
    <p className="flex items-baseline gap-2">
      <span
        className={`font-display font-medium text-[28px] tabular-nums tracking-[-0.02em] ${tone ?? 'text-head'}`}
      >
        {value}
      </span>
      <span className="text-[14px] text-muted">{label}</span>
    </p>
  );
}

// ---------- the code, side by side, the left side building as you scroll ----------

const STEP_GAP = 0.28;
const STEP_LEN = 0.18;

/** How many of the four tracing steps are done at scroll progress `p`, holding still between them.
 *  The first, a span around the function, is in from the start; scrolling adds the other three. */
function stepsDone(p: number) {
  return [0, 1, 2].reduce(
    (sum, i) =>
      sum + ramp(p, 0.1 + i * STEP_GAP, 0.1 + i * STEP_GAP + STEP_LEN),
    1,
  );
}

/** A code panel whose bar says which language it is, with the line count, the number that matters,
 *  out on the right. */
function FilePanel({
  children,
  file,
  lines,
  logo,
}: {
  children: ReactNode;
  file: string;
  lines: number;
  logo: ReactNode;
}) {
  return (
    <figure className="rounded-xl overflow-hidden border border-rule bg-raised">
      <figcaption className="v4-caption h-10 flex items-center gap-2.5 px-4 border-b border-rule text-ink-2">
        {logo}
        {file}
        <span className="ml-auto tabular-nums text-head">{lines} lines</span>
      </figcaption>
      {children}
    </figure>
  );
}

// Rows on each side once everything is in: the TypeScript and what it records on the left, the BAML
// and its trace on the right. The two have to match, or the sides end at different heights.
const SIDE_ROWS = TS.length + BUILT.length;
if (BAML.length + DEEP.length !== SIDE_ROWS) {
  throw new Error(
    'The two sides of the scroll build must have the same number of rows.',
  );
}

// Every tracing step the TypeScript needs, all in from the start.
/** The BAML side: its code and its trace, every span in from the start. Hovering a line lights up the
 *  spans it records, and hovering a span lights up its line. */
function HyperSide() {
  const [hot, setHot] = useState<string | null>(null);
  return (
    <div className="min-w-0">
      <p className="v4-h3">
        <Hyper>Hyperobservability</Hyper>
      </p>
      <p className="mt-1 mb-4 text-[20px] text-ink-2">No work, more data.</p>
      <FilePanel
        file="support.baml"
        lines={BAML.length}
        logo={
          <svg
            aria-hidden="true"
            className="h-[20px] w-auto text-clover"
            fill="currentColor"
            viewBox="0 0 356 422"
          >
            {LAMB_PATHS.map((d) => (
              <path d={d} key={d} />
            ))}
          </svg>
        }
      >
        <BamlCode hot={hot} onHot={setHot} />
      </FilePanel>
      <Panel className="mt-3" title="what you get">
        <Waterfall
          groups={DEEP_GROUPS}
          hover={{ hot, onHot: setHot }}
          rows={DEEP}
        />
      </Panel>
    </div>
  );
}

function SideBySide() {
  return (
    <Pinned length="240vh">
      {(p) => {
        const steps = stepsDone(p);
        // The TypeScript starts with a span around its logic and piles on more tracing as you scroll; the BAML
        // side has every span from the start.
        const lines = Math.round(
          TS.reduce(
            (sum, l) => sum + (l.at === 0 ? 1 : ramp(steps, l.at - 1, l.at)),
            0,
          ),
        );
        return (
          <div
            className="grid md:grid-cols-2 gap-6 items-start"
            // SIDE_ROWS rows a side, sized so the whole figure fits in the window.
            style={
              {
                '--r': `clamp(13px, calc((100vh - 335px) / ${SIDE_ROWS}), 18px)`,
              } as CSSProperties
            }
          >
            <div className="min-w-0">
              <p className="v4-h3 text-head">Observability</p>
              <p className="mt-1 mb-4 text-[20px] text-ink-2">
                More work, less data.
              </p>
              <FilePanel
                file="support.ts"
                lines={lines}
                logo={
                  // biome-ignore lint/performance/noImgElement: a tiny static svg
                  <img
                    alt=""
                    className="size-[18px]"
                    height={18}
                    src="/logos/typescript.svg"
                    width={18}
                  />
                }
              >
                <TsCode level={steps} />
              </FilePanel>
              <Panel className="mt-3" title="what you get">
                <Waterfall rows={BUILT} steps={steps} />
              </Panel>
            </div>
            <HyperSide />
          </div>
        );
      }}
    </Pinned>
  );
}

// ---------- slow vs fast decisions, as two team chats ----------

function SideName({ lit }: { lit?: boolean }) {
  return lit ? (
    <p className="v4-h3">
      <Hyper>Hyperobservability</Hyper>
    </p>
  ) : (
    <p className="v4-h3 text-head">Observability</p>
  );
}

/** How many days the question has been open: counting while it waits, then the day it was decided. */
function Days({ day, lit }: { day: number; lit: boolean }) {
  const value = lit ? 1 : dayNumber(day);
  return (
    <Stat
      label={value === 1 ? 'day' : 'days'}
      tone={lit ? 'text-clover' : 'text-head'}
      value={String(value)}
    />
  );
}

// Illustrative days in November 2026, which starts on a Monday the 2nd.
const FIRST_DATE = 2;

type Msg = [
  from: string,
  time: string,
  text: string,
  opts?: { decides?: boolean; pr?: PullRequest; react?: string },
];

/** A pull request a message links to, shown the way chat apps unfurl a GitHub link. */
type PullRequest = {
  additions: number;
  deletions: number;
  files: number;
  number: number;
  repo: string;
  title: string;
};

// Claude's pull request on the left: the metrics nobody recorded before the question came up.
const METRICS_PR: PullRequest = {
  additions: 48,
  deletions: 2,
  files: 3,
  number: 1287,
  repo: 'acme/support-agent',
  title: "Trace the agent's tool calls and retries",
};

/** One calendar day of both teams' chats. */
type ThreadDay = { day: number; hyper: Msg[]; obs: Msg[] };

const DECIDES = { decides: true };

// Both teams ask Claude the same question on Monday evening: the support agent is slow, but only for
// some tickets. On the right, every call the agent made was recorded, so Claude finds the slow ones at
// once. On the left, the averages look fine and nothing traced the agent's tool calls, so Claude adds
// tracing and then has to wait for a slow ticket to come in, which it finally catches on the ninth day.
const ASK = '@claude the support agent is slow on some tickets, not all. Why?';
const THREAD: ThreadDay[] = [
  {
    day: 0,
    hyper: [
      ['Vaibhav', '6:12 PM', ASK],
      [
        'Claude',
        '6:12 PM',
        'Only PDF tickets. The parse tool times out, then retries.',
      ],
      ['Vaibhav', '6:14 PM', 'Cap the retries 👍', DECIDES],
    ],
    obs: [
      ['Vaibhav', '6:12 PM', ASK],
      [
        'Claude',
        '6:13 PM',
        "Averages look fine, and we don't trace tool calls. Adding it.",
      ],
    ],
  },
  {
    day: 1,
    hyper: [],
    obs: [
      [
        'Claude',
        '9:21 AM',
        'Opened a PR with the new metrics.',
        { pr: METRICS_PR },
      ],
      ['Claude', '10:40 AM', 'Merged. Waiting for a slow ticket ⏳'],
    ],
  },
  {
    day: 4,
    hyper: [],
    obs: [
      ['Vaibhav', '4:30 PM', '@claude any news?'],
      ['Claude', '4:31 PM', "It's rare. Haven't caught a slow one yet."],
    ],
  },
  {
    day: 8,
    hyper: [],
    obs: [
      [
        'Claude',
        '9:40 AM',
        'Caught one. PDF tickets: the parse tool times out, then retries.',
      ],
      ['Vaibhav', '9:52 AM', 'Cap the retries 👍', DECIDES],
    ],
  },
];

// A little past the left team's decision on day 8, so its star burst finishes.
const THREAD_DAYS = 9.2;
// Where the right team's chat sits: just past its decision on day 0.
const HYPER_DAY = 1.2;

/** The day number a reader sees, counting the day of the question as day 1. */
const dayNumber = (day: number) =>
  Math.min(9, Math.max(1, Math.floor(day - 0.3) + 1));

type Skin = 'slack' | 'teams';

const FULL_DAYS = ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday'];

type ChatItem = { when: number } & (
  | { kind: 'date'; label: string }
  | {
      decides: boolean;
      from: string;
      kind: 'msg';
      long: boolean;
      pr?: PullRequest;
      react?: string;
      text: string;
      time: string;
    }
);

/** One side's chat in order, with a date only on days something was said. */
function chatItems(lit: boolean): ChatItem[] {
  return THREAD.flatMap((r): ChatItem[] => {
    const msgs = lit ? r.hyper : r.obs;
    if (msgs.length === 0) return [];
    const date: ChatItem = {
      kind: 'date',
      label: `${FULL_DAYS[r.day % 7]}, November ${FIRST_DATE + r.day}`,
      when: r.day,
    };
    return [
      date,
      ...msgs.map(
        ([from, time, text, opts], i): ChatItem => ({
          decides: opts?.decides === true,
          from,
          kind: 'msg',
          // Too long for one line on a phone, so it gets a second line everywhere.
          long: text.length > 32,
          pr: opts?.pr,
          react: opts?.react,
          text,
          time,
          // A busy day fits its messages into the same day.
          when: r.day + 0.15 + (i * 0.75) / msgs.length,
        }),
      ),
    ];
  });
}

const CHATS = { hyper: chatItems(true), obs: chatItems(false) };

const PEOPLE: Record<string, { photo: string; x: string }> = {
  Vaibhav: { photo: '/profile-vbv.jpeg', x: 'https://x.com/vaibcode' },
};

/** Anyone not in `PEOPLE` is an app in the channel, like Claude. */
const isApp = (from: string) => !PEOPLE[from];

/** A person's photo, which quietly links to their profile on X, or an app's mark. */
function Avatar({ from, round }: { from: string; round: boolean }) {
  if (isApp(from)) {
    return (
      <span
        aria-hidden
        className={`grid size-[36px] shrink-0 place-items-center bg-[#D97757] text-[20px] text-white ${round ? 'rounded-full' : 'rounded-[7px]'}`}
      >
        <SiClaude />
      </span>
    );
  }
  return (
    <a
      aria-label={`${from} on X`}
      className="shrink-0"
      href={PEOPLE[from].x}
      rel="noopener noreferrer"
      target="_blank"
    >
      {/* biome-ignore lint/performance/noImgElement: a small static avatar */}
      <img
        alt=""
        className={`size-[36px] object-cover object-[50%_30%] ${round ? 'rounded-full' : 'rounded-[7px]'}`}
        height={36}
        src={PEOPLE[from].photo}
        width={36}
      />
    </a>
  );
}

const GOLD = '#F5C542';

function Star({ size = 15 }: { size?: number }) {
  return (
    <svg aria-hidden="true" height={size} viewBox="0 0 24 24" width={size}>
      <path
        d="M12 2.5l2.9 6.2 6.800.8-5 4.700 1.300 6.800-6-3.400-6 3.400 1.300-6.800-5-4.700 6.800-.8z"
        fill={GOLD}
      />
    </svg>
  );
}

const SPARKS = [-60, -25, 15, 55, 100, 140, 200, 250, 300];

/** The celebration: once the message has arrived, the tag pops in and small stars burst out of it.
 *  CSS animations, so they run on the compositor at their own pace, not the scroll's. */
function DecisionTag({ on }: { on: boolean }) {
  return (
    <span
      className={`relative shrink-0 inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[13px] font-semibold text-white ${on ? 'decision-pop' : 'scale-[0.4] opacity-0'}`}
      style={{
        background: 'rgb(245 197 66 / 0.16)',
        borderColor: 'rgb(245 197 66 / 0.6)',
      }}
    >
      <Star />
      Decision
      {on &&
        SPARKS.map((deg) => {
          const a = (deg * Math.PI) / 180;
          return (
            <span
              className="decision-spark absolute left-[14px] top-1/2 -mt-[5px] -ml-[5px]"
              key={deg}
              style={
                {
                  '--dx': `${Math.cos(a) * 48}px`,
                  '--dy': `${Math.sin(a) * 48}px`,
                } as CSSProperties
              }
            >
              <Star size={10} />
            </span>
          );
        })}
    </span>
  );
}

/** An emoji reaction under the team's fun. */
function Reaction({ react }: { react: string }) {
  return (
    <span className="shrink-0 rounded-full border border-white/15 bg-white/8 px-2 text-[12px] leading-[20px] text-white/80">
      {react}
    </span>
  );
}

/** A message's text with @claude set like a chat app sets a mention. */
function Mentions({ text }: { text: string }) {
  return (
    <>
      {text.split(/(@claude)/).map((part, i) =>
        part === '@claude' ? (
          <span
            className="rounded-[3px] bg-[#1d9bd1]/20 px-0.5 font-medium text-[#4fb6e8]"
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed split of one string
            key={i}
          >
            {part}
          </span>
        ) : (
          part
        ),
      )}
    </>
  );
}

/** A GitHub pull request unfurled under a message: repo, number and title, and its diff size. */
function PrCard({ pr }: { pr: PullRequest }) {
  return (
    <span className="mt-1.5 flex max-w-[22rem] gap-2.5 rounded-[4px] border-l-[3px] border-white/20 bg-white/[0.04] py-1.5 pr-3 pl-2.5 text-left">
      <SiGithub
        aria-hidden="true"
        className="mt-0.5 shrink-0 text-[15px] text-white/80"
      />
      <span className="min-w-0 leading-[1.35]">
        <span className="block truncate text-[12px] text-white/50">
          {pr.repo}
        </span>
        <span className="block truncate text-[13px] font-semibold text-[#4fb6e8]">
          #{pr.number} {pr.title}
        </span>
        <span className="mt-0.5 flex items-center gap-2 text-[12px]">
          <span className="rounded-full bg-[#238636] px-1.5 font-medium text-white">
            Open
          </span>
          <span className="font-mono text-[#3fb950]">+{pr.additions}</span>
          <span className="font-mono text-[#f85149]">−{pr.deletions}</span>
          <span className="text-white/50">{pr.files} files</span>
        </span>
      </span>
    </span>
  );
}

const PR_H = 70;
// A message's second line.
const LINE_H = 20;
const MSG_H = 56;
const REACTION_H = 28;
const DATE_H = 40;

function ChatMessage({
  item,
  on,
  skin,
}: {
  item: Extract<ChatItem, { kind: 'msg' }>;
  on: boolean;
  skin: Skin;
}) {
  const { decides, from, pr, react, text, time } = item;
  if (skin === 'slack') {
    return (
      <div
        className={`flex items-start gap-3 h-full px-4 pt-[7px] border-l-[3px] ${decides ? 'border-clover bg-clover/20' : 'border-transparent'}`}
      >
        <Avatar from={from} round={false} />
        <div className="min-w-0 leading-[1.35]">
          <p className="text-[15px]">
            {isApp(from) ? (
              <>
                <b className="font-bold text-white">{from}</b>
                <span className="ml-1.5 rounded-[3px] bg-white/10 px-1 py-px align-[1px] text-[10px] font-semibold text-white/60">
                  APP
                </span>
              </>
            ) : (
              <a
                className="font-bold text-white hover:underline"
                href={PEOPLE[from].x}
                rel="noopener noreferrer"
                target="_blank"
              >
                {from}
              </a>
            )}
            <span className="ml-2 text-[12px] text-white/40">{time}</span>
          </p>
          <p
            className={`line-clamp-2 text-[13px] sm:text-[15px] ${decides ? 'font-medium text-white' : 'text-[#d1d2d3]'}`}
          >
            <Mentions text={text} />
          </p>
          {pr && <PrCard pr={pr} />}
          {react && (
            <p className="mt-1.5">
              <Reaction react={react} />
            </p>
          )}
        </div>
        {decides && (
          <span className="ml-auto self-center -mt-[7px]">
            <DecisionTag on={on} />
          </span>
        )}
      </div>
    );
  }
  const sam = from === 'Vaibhav';
  return (
    <div
      className={`flex items-center gap-2.5 h-full px-4 ${sam ? 'justify-end' : ''}`}
    >
      {!sam && <Avatar from={from} round />}
      {decides && sam && <DecisionTag on={on} />}
      <div
        className={`min-w-0 flex flex-col gap-1 ${sam ? 'items-end' : 'items-start'}`}
      >
        <div
          className={`min-w-0 rounded-md px-3 py-1 leading-[1.35] ${decides ? 'bg-clover text-black' : sam ? 'bg-[#444791] text-white/90' : 'bg-[#333] text-white/90'}`}
        >
          <p
            className={`text-[12px] ${decides ? 'text-black/60' : 'text-white/50'}`}
          >
            {from} {time}
          </p>
          <p
            className={`line-clamp-2 text-[13px] sm:text-[15px] ${decides ? 'font-medium' : ''}`}
          >
            <Mentions text={text} />
          </p>
        </div>
        {pr && <PrCard pr={pr} />}
        {react && <Reaction react={react} />}
      </div>
      {decides && !sam && <DecisionTag on={on} />}
      {sam && <Avatar from={from} round />}
    </div>
  );
}

/** A chat item's height once it has arrived. */
function fullHeight(item: ChatItem) {
  return item.kind === 'date'
    ? DATE_H
    : MSG_H +
        (item.long ? LINE_H : 0) +
        (item.react ? REACTION_H : 0) +
        (item.pr ? PR_H : 0);
}

/** Whether a chat item has arrived by `day`. Scrolling only decides when; the motion itself is a
 *  CSS transition, so it plays at the same smooth pace however the reader scrolls. */
const arrived = (day: number, item: ChatItem) => day >= item.when + 0.05;

/** A chat window. New messages take up room at the bottom, pushing older ones up and out of view. */
function ChatWindow({
  day,
  items,
  skin,
}: {
  day: number;
  items: ChatItem[];
  skin: Skin;
}) {
  const slack = skin === 'slack';
  return (
    <div
      className={`flex overflow-hidden rounded-lg border border-white/10 ${slack ? 'bg-[#1a1d21]' : 'bg-[#1f1f1f]'}`}
    >
      <div
        className={`w-[36px] shrink-0 flex justify-center py-3 ${slack ? 'bg-[#3f0e40]' : 'bg-[#141414]'}`}
      >
        <span
          className={`size-[20px] rounded-[5px] ${slack ? 'bg-white/30' : 'bg-[#7f85f5]'}`}
        />
      </div>
      <div className="min-w-0 flex-1 flex flex-col">
        <div
          className={`h-11 shrink-0 flex items-center gap-3 px-4 border-b border-white/10 ${slack ? '' : 'bg-[#292929]'}`}
        >
          <b className="text-[15px] font-bold text-white">
            {slack ? '# support' : 'Support'}
          </b>
          <span className="ml-auto font-mono text-[13px] tabular-nums text-white/55">
            day {dayNumber(day)}
          </span>
        </div>
        {/* Every message keeps its full height and the whole list slides up as one, on the GPU. A
            message that hasn't arrived yet sits below the bottom edge; as it arrives the list moves
            up by its height, pushing older messages out of view at the top. Nothing is laid out
            again while it moves, so it stays smooth. */}
        <div className="relative h-[clamp(330px,calc(100vh-300px),580px)] overflow-hidden">
          <div
            className="absolute inset-x-0 bottom-0 will-change-transform transition-transform duration-500 ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none"
            style={{
              transform: `translate3d(0, ${items.reduce((sum, item) => sum + (arrived(day, item) ? 0 : fullHeight(item)), 0)}px, 0)`,
            }}
          >
            {items.map((item) => {
              const on = arrived(day, item);
              return (
                <div
                  className="will-change-[opacity,transform] transition-[opacity,transform] delay-100 duration-400 ease-out motion-reduce:transition-none"
                  // Not arrived yet: out of the tab order and the accessibility tree, so keyboard
                  // focus can't scroll it into view early and screen readers don't read ahead.
                  inert={!on}
                  key={`${item.when}-${item.kind}`}
                  style={{
                    height: fullHeight(item),
                    opacity: on ? 1 : 0,
                    transform: `translate3d(0, ${on ? 0 : 10}px, 0)`,
                  }}
                >
                  {item.kind === 'date' ? (
                    <p className="flex h-full items-center justify-center text-[12px] font-medium text-white/50">
                      {item.label}
                    </p>
                  ) : (
                    <ChatMessage item={item} on={on} skin={skin} />
                  )}
                </div>
              );
            })}
          </div>
        </div>
        <div className="p-2.5">
          <p
            className={`flex h-[34px] items-center px-3 text-[14px] text-white/40 ${slack ? 'rounded-lg border border-white/20' : 'rounded-sm bg-[#333] border-b-2 border-[#7f85f5]'}`}
          >
            {slack ? 'Message #support' : 'Type a message'}
          </p>
        </div>
      </div>
    </div>
  );
}

function Threads() {
  const [skin, setSkin] = useState<Skin>('slack');
  useEffect(() => {
    // The easter egg: Windows gets Teams, and everyone else gets Slack.
    if (/Windows/i.test(navigator.userAgent)) setSkin('teams');
  }, []);
  return (
    <Pinned length="300vh">
      {(p) => {
        // One clock for both chats, played one after the other: the right decides by day 1, and
        // only then does the left start its nine days.
        const scrolled = sweep(p) * (HYPER_DAY + THREAD_DAYS);
        return (
          <div className="grid md:grid-cols-2 gap-x-6 gap-y-8">
            {[false, true].map((lit) => {
              const day = lit
                ? Math.min(HYPER_DAY, scrolled)
                : Math.max(0, scrolled - HYPER_DAY);
              return (
                <div className="min-w-0" key={String(lit)}>
                  <div className="mb-3 flex items-baseline justify-between gap-4">
                    <SideName lit={lit} />
                    <Days day={day} lit={lit} />
                  </div>
                  <ChatWindow
                    day={day}
                    items={lit ? CHATS.hyper : CHATS.obs}
                    skin={skin}
                  />
                </div>
              );
            })}
          </div>
        );
      }}
    </Pinned>
  );
}

// ---------- the heading ----------

// What observability slows down. All plural, so the second sentence's "them" fits every one.
const SLOWED = ['decisions', 'incidents', 'product calls', 'fixes'];
const ROTATE_MS = 2600;

/** "Observability makes X slow. Hyperobservability makes them fast.", with X rotating, one sentence
 *  a line. Under reduced motion it stays on the first. */
function Rotating() {
  const [n, setN] = useState(0);
  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const id = setInterval(() => setN((k) => k + 1), ROTATE_MS);
    return () => clearInterval(id);
  }, []);
  return (
    <>
      {/* Every option, for screen readers, which would otherwise hear it change. */}
      <span className="sr-only">
        Observability makes {SLOWED.join(', ')} slow. Hyperobservability makes
        them fast.
      </span>
      <span aria-hidden>
        <span className="block sm:whitespace-nowrap">
          Observability makes{' '}
          <span className="heading-swap" key={n}>
            {SLOWED[n % SLOWED.length]}
          </span>{' '}
          slow.
        </span>
        <span className="block sm:whitespace-nowrap">
          <Hyper>Hyperobservability</Hyper> makes them fast.
        </span>
      </span>
    </>
  );
}

// ---------- the section ----------

export default function Hyperobs() {
  return (
    <section
      {...section('hyperobservability')}
      aria-labelledby="hyperobservability"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 text-head max-w-[24em]" id="hyperobservability">
          <Hyper>Hyperobservability</Hyper> is observability that is always on,
          by default, for everything.
        </h2>

        <div className="mt-10 md:mt-12">
          <SideBySide />
        </div>

        <div className="mt-20 md:mt-28">
          <h3 className="v4-h2 text-head max-w-[24em]">
            <Rotating />
          </h3>
          <p className="mt-4 max-w-[40rem] v4-body text-ink-2">
            Observability only answers the questions you thought of before you
            shipped. <Hyper>Hyperobservability</Hyper> answers all of them.
          </p>
        </div>
        <div className="mt-10 md:mt-12">
          <Threads />
        </div>
      </div>
    </section>
  );
}
