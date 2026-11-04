'use client';

import {
  Fragment,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { Highlight, Prose, proseLines } from './code';
import { HIGHLIGHT, RED } from './story';

// Product videos, drawn live rather than recorded. Two windows sit side by side and hold still: an
// editor with triage.baml open, and one terminal where you and then your agent run baml commands. A
// script on one clock types into both, an optional caption runs along the bottom, and the chapters
// under the frame show progress and seek. Version 22 walks the loop from writing a function to fixing
// it from production. Version 24 has no captions; it highlights the code each query is about, and
// shows an alert next to its function. Both files pass baml check. All data is example data.

const W = 1200;
const H = 720;

export const clamp = (v: number, lo = 0, hi = 1) =>
  Math.min(hi, Math.max(lo, v));
export const typed = (text: string, t: number, at: number, ms = 500) =>
  text.slice(0, Math.round(clamp((t - at) / ms) * text.length));
const lastAt = <T extends { at: number }>(list: T[], t: number) =>
  list.reduce((found, item) => (item.at <= t ? item : found), list[0]);
export const within = (
  t: number,
  from: number,
  to = Number.POSITIVE_INFINITY,
) => t >= from && t < to;

/** The video's clock: milliseconds into the loop, and a way to jump. Holds at `hold` under reduced motion. */
function useVideoClock(total: number, hold: number) {
  const [t, setT] = useState(0);
  const start = useRef(0);
  const still = useRef(false);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      still.current = true;
      setT(hold);
      return;
    }
    start.current = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      setT(Math.max(0, now - start.current) % total);
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [hold, total]);

  const seek = useCallback((ms: number) => {
    if (still.current) setT(ms);
    else start.current = performance.now() - ms;
  }, []);

  return [t, seek] as const;
}

// ---- What a script describes -----------------------------------------------------------------------

export type Tone = 'bad' | 'focus' | 'ok';

/** Marks on the editor at one moment: tinted lines, underlined code with an optional count after it,
 *  a note at the end of a line, the alarm in the gutter, the typing caret, and lines added since the
 *  file was opened, which keep a diff marker under any other tint. */
export type Deco = {
  added?: number[];
  alarm?: { firing: boolean; line: number };
  caret?: number;
  ends?: { hint: string; line: number; tone?: Tone }[];
  lines?: Record<number, Tone>;
  marks?: { hint?: string; line: number; text: string; tone?: Tone }[];
};

export type Entry =
  | {
      at: number;
      kind: 'agent' | 'cmd' | 'sep' | 'tool' | 'user';
      text: string;
    }
  | {
      at: number;
      kind: 'out';
      right?: string;
      text: string;
      tone?: 'bad' | 'muted' | 'ok' | 'pass';
    }
  | { at: number; kind: 'node'; render: (since: number) => ReactNode };

export type Script = {
  // A line along the bottom of the frame. A script without captions shows none.
  captions?: { at: number; text: string }[];
  chapters: { at: number; name: string }[];
  code: (t: number) => string[];
  deco: (t: number) => Deco;
  // How tall the two windows are. A script that leaves it out gets TALL.
  height?: number;
  hold: number;
  label: string;
  terminal: Entry[];
  total: number;
};

// ---- Windows ---------------------------------------------------------------------------------------

const TOP = 24;
const TALL = 604;

function Window({
  children,
  height,
  title,
  w,
  x,
}: {
  children: ReactNode;
  height: number;
  title: string;
  w: number;
  x: number;
}) {
  return (
    <section
      className="absolute flex flex-col rounded-xl overflow-hidden border border-white/12 bg-[#0b0b0c]"
      style={{ height, left: x, top: TOP, width: w }}
    >
      <div className="relative h-9 shrink-0 flex items-center px-3.5 border-b border-white/[0.08] bg-[#111113]">
        <span className="flex gap-1.5">
          {[0, 1, 2].map((d) => (
            <span className="size-2.5 rounded-full bg-white/15" key={d} />
          ))}
        </span>
        <span className="absolute inset-x-0 text-center font-mono text-[12px] text-white/50 pointer-events-none">
          {title}
        </span>
      </div>
      {children}
    </section>
  );
}

const LINE_TINT: Record<Tone, string> = {
  bad: 'bg-[#E5484D]/12',
  focus: 'bg-[#FFD43B]/[0.09]',
  ok: 'bg-(--color-pasture-200)/[0.18]',
};

// A line added in this session, as a diff shows it: the whole line in green, with a bar along its left
// edge and a plus in the gutter.
const ADDED =
  'bg-(--color-pasture-200)/15 shadow-[inset_3px_0_0_var(--color-pasture-200)]';

const MARK: Record<Tone, string> = {
  bad: 'bg-[#E5484D]/15 shadow-[inset_0_-2px_0_#E5484D]',
  focus: 'bg-[#FFD43B]/20 shadow-[inset_0_-2px_0_#FFD43B]',
  ok: 'bg-(--color-pasture-200)/15 shadow-[inset_0_-2px_0_var(--color-pasture-200)]',
};

function Hint({ text, tone }: { text: string; tone?: Tone }) {
  return (
    <span
      className="v3-enter inline-block ml-1.5 px-1.5 rounded-[4px] bg-white/[0.07] text-[11px] leading-[16px] align-[1px] whitespace-nowrap"
      style={{
        color:
          tone === 'bad'
            ? RED
            : tone === 'ok'
              ? 'var(--color-pasture-200)'
              : HIGHLIGHT,
      }}
    >
      {text}
    </span>
  );
}

export function Alarm({ firing }: { firing: boolean }) {
  return (
    <span className="relative grid place-items-center size-4">
      {firing && (
        <span
          className="absolute inset-0 rounded-full animate-ping"
          style={{ background: RED, opacity: 0.35 }}
        />
      )}
      <svg
        aria-hidden="true"
        fill="none"
        height="14"
        stroke={firing ? RED : 'var(--color-pasture-200)'}
        strokeLinecap="round"
        strokeWidth="1.6"
        viewBox="0 0 16 16"
        width="14"
      >
        <circle cx="8" cy="9" r="5" />
        <path d="M8 6.5V9l1.6 1.2M2.5 3.5 4.5 2M13.5 3.5 11.5 2M5 14l-1 1M11 14l1 1" />
      </svg>
    </span>
  );
}

/** One source line with its marks, each followed by its count when it has one. */
function Marked({
  line,
  marks,
  prose,
}: {
  line: string;
  marks: Deco['marks'];
  prose?: boolean;
}) {
  const Text = prose ? Prose : Highlight;
  const found = (marks ?? [])
    .map((m) => ({ ...m, index: line.indexOf(m.text) }))
    .filter((m) => m.index >= 0)
    .sort((a, b) => a.index - b.index);
  const pieces: ReactNode[] = [];
  let at = 0;
  for (const m of found) {
    pieces.push(
      <Text key={`p${m.index}`} line={line.slice(at, m.index)} />,
      <span
        className={`v3-enter inline-block rounded-[3px] ${MARK[m.tone ?? 'focus']}`}
        key={`m${m.index}`}
      >
        <Text line={m.text} />
      </span>,
    );
    if (m.hint)
      pieces.push(<Hint key={`h${m.index}`} text={m.hint} tone={m.tone} />);
    at = m.index + m.text.length;
  }
  pieces.push(<Text key="rest" line={line.slice(at)} />);
  return <>{pieces}</>;
}

function Editor({ deco, lines }: { deco: Deco; lines: string[] }) {
  const prose = proseLines(lines);
  return (
    <div className="flex-1 min-h-0 overflow-hidden py-3 font-mono text-[12.5px] leading-[22px] text-[#BDBDC4]">
      {lines.map((line, i) => {
        const added = deco.added?.includes(i);
        const tone = deco.lines?.[i];
        const end = deco.ends?.find((e) => e.line === i);
        return (
          <div
            className={`flex items-start pr-4 transition-colors duration-300 ${added ? ADDED : tone ? LINE_TINT[tone] : ''}`}
            // biome-ignore lint/suspicious/noArrayIndexKey: the file's lines keep their positions
            key={i}
          >
            <span className="shrink-0 w-5 h-[22px] pl-1.5 grid items-center">
              {added && deco.alarm?.line !== i && (
                <span className="text-(--color-pasture-200)">+</span>
              )}
              {deco.alarm?.line === i && (
                <Alarm
                  firing={deco.alarm.firing}
                  key={String(deco.alarm.firing)}
                />
              )}
            </span>
            <span className="shrink-0 w-8 pr-3 text-right text-white/20">
              {i + 1}
            </span>
            <span className="min-w-0 whitespace-pre-wrap">
              <Marked
                line={line}
                marks={deco.marks?.filter((m) => m.line === i)}
                prose={prose.has(i)}
              />
              {deco.caret === i && (
                <span className="inline-block w-[2px] h-[15px] align-[-3px] bg-white" />
              )}
            </span>
            {end && (
              <span className="ml-auto pl-3 shrink-0">
                <Hint text={end.hint} tone={end.tone} />
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}

const OUT_TONE = {
  bad: 'text-[#E5484D]',
  muted: 'text-white/40',
  ok: 'text-(--color-pasture-200)',
  pass: 'text-white/70',
};

// The lines start at the top of the window. Once they fill it, the newest line stays at the bottom and the
// oldest scroll off the top: the inner column is at least as tall as the window, and sits at its bottom.
function Terminal({ entries, t }: { entries: Entry[]; t: number }) {
  const shown = entries.filter((e) => e.at <= t);
  return (
    <div className="flex-1 min-h-0 overflow-hidden flex flex-col justify-end px-4 py-3 font-mono text-[12.5px] leading-[21px]">
      <div className="shrink-0 min-h-full flex flex-col [&>*]:shrink-0">
        {shown.length === 0 && (
          <p className="text-white/90">
            <span className="text-clover">$</span>{' '}
            <span className="inline-block w-[7px] h-[14px] align-[-2px] bg-white/80" />
          </p>
        )}
        {shown.map((e, i) => {
          const key = `${e.at}-${i}`;
          switch (e.kind) {
            case 'cmd':
              return (
                <p
                  className="truncate text-white/90 mt-2.5 first:mt-0"
                  key={key}
                >
                  <span className="text-clover">$</span>{' '}
                  {typed(e.text, t, e.at, 550)}
                </p>
              );
            case 'user':
              return (
                <p
                  className="mt-2.5 px-2.5 py-1 rounded-md bg-white/[0.06] text-white"
                  key={key}
                >
                  <span className="text-white/40">&gt;</span>{' '}
                  {typed(e.text, t, e.at, 700)}
                </p>
              );
            case 'tool':
              return (
                <p className="truncate mt-2.5 text-white/60" key={key}>
                  <span className="text-clover">●</span>{' '}
                  {typed(e.text, t, e.at, 450)}
                </p>
              );
            case 'agent':
              return (
                <p className="v3-enter mt-2.5 pl-4 text-white/85" key={key}>
                  {e.text}
                </p>
              );
            case 'sep':
              return (
                <p
                  className="v3-enter flex items-center gap-3 my-3 text-[11.5px] text-white/35"
                  key={key}
                >
                  <span className="flex-1 border-t border-dashed border-white/15" />
                  {e.text}
                  <span className="flex-1 border-t border-dashed border-white/15" />
                </p>
              );
            case 'out':
              return (
                <p
                  className={`flex gap-2 pl-4 ${OUT_TONE[e.tone ?? 'pass']}`}
                  key={key}
                >
                  {e.tone === 'pass' && (
                    <span className="text-(--color-pasture-200)">✓</span>
                  )}
                  <span className="truncate">{e.text}</span>
                  {e.right && (
                    <span className="ml-auto shrink-0 text-white/35">
                      {e.right}
                    </span>
                  )}
                </p>
              );
            default:
              return (
                <div className="v3-enter pl-4 mt-1" key={key}>
                  {e.render(t - e.at)}
                </div>
              );
          }
        })}
      </div>
    </div>
  );
}

// ---- Widgets the terminal prints -------------------------------------------------------------------

type Row = { bad?: boolean; good?: boolean; label: string; value: number };

/** A labelled bar per row, revealed one row at a time. */
export function Bars({
  format = (v) => v.toLocaleString('en-US'),
  rows,
  since,
}: {
  format?: (v: number) => string;
  rows: Row[];
  since: number;
}) {
  const max = Math.max(...rows.map((r) => r.value));
  return (
    <div className="grid grid-cols-[auto_auto_minmax(0,1fr)] gap-x-4 items-center">
      {rows.slice(0, Math.floor(since / 220) + 1).map((r) => {
        const color = r.bad ? RED : undefined;
        return (
          <Fragment key={r.label}>
            <span className="text-white/75" style={{ color }}>
              {r.label}
            </span>
            <span
              className="text-right tabular-nums text-white/75"
              style={{ color }}
            >
              {format(r.value)}
            </span>
            <span
              className="v3-enter h-2 rounded-[2px]"
              style={{
                background:
                  color ??
                  (r.good ? 'var(--color-pasture-200)' : 'var(--color-clover)'),
                opacity: color || r.good ? 1 : 0.6,
                width: `${Math.max(2, (r.value / max) * 100)}%`,
              }}
            />
          </Fragment>
        );
      })}
    </div>
  );
}

const SARCASTIC = [
  'Oh great, charged twice again. Love that.',
  'Wow, the export broke again. Amazing.',
  'Sure, I will just wait another week.',
];

export function Inputs({ since }: { since: number }) {
  return (
    <div>
      {SARCASTIC.slice(0, Math.floor(since / 200) + 1).map((input) => (
        <p className="flex gap-3" key={input}>
          <span className="truncate text-white/70">{input}</span>
          <span className="ml-auto shrink-0" style={{ color: RED }}>
            other
          </span>
        </p>
      ))}
    </div>
  );
}

// ---- The player ------------------------------------------------------------------------------------

/** Plays a script. `below` draws under the frame on the same clock, for anything that follows the video. */
export function Player({
  below,
  script,
}: {
  below?: (t: number) => ReactNode;
  script: Script;
}) {
  const { captions, chapters, total } = script;
  const [t, seek] = useVideoClock(total, script.hold);
  const frame = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState(0);
  const tall = script.height ?? TALL;
  // Without captions there is no strip to keep clear under the windows.
  const height = captions?.length ? H : TOP + tall + TOP;

  useLayoutEffect(() => {
    const el = frame.current;
    if (!el) return;
    const measure = () => setFit(el.clientWidth / W);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const chapter = lastAt(chapters, t);
  const caption = captions?.length ? lastAt(captions, t) : null;
  // Fade in at the start of the loop and out at its end, like a video that repeats.
  const fade = Math.min(clamp(t / 400), clamp((total - t) / 500));

  return (
    <figure aria-label={script.label} className="relative pb-24">
      <div className="rounded-2xl border border-white/12 bg-[#050505] p-1.5 md:p-2">
        <div
          className="relative overflow-hidden rounded-[11px] bg-[#161618]"
          ref={frame}
          style={{ height: height * fit }}
        >
          {fit > 0 && (
            <div
              aria-hidden
              className="absolute left-0 top-0 origin-top-left"
              style={{
                height,
                opacity: fade,
                transform: `scale(${fit})`,
                width: W,
              }}
            >
              <Window height={tall} title="triage.baml" w={600} x={24}>
                <Editor deco={script.deco(t)} lines={script.code(t)} />
              </Window>
              <Window height={tall} title="~/triage" w={536} x={640}>
                <Terminal entries={script.terminal} t={t} />
              </Window>
            </div>
          )}
          {caption && (
            <p
              className="absolute left-1/2 -translate-x-1/2 bottom-1.5 md:bottom-5 w-max max-w-[calc(100%-1rem)] px-2.5 md:px-4 py-1 md:py-2 rounded-md md:rounded-lg bg-black/85 border border-white/10 text-[11px] md:text-[15px] text-white"
              key={caption.at}
            >
              <span className="v3-enter inline-block">{caption.text}</span>
            </p>
          )}
        </div>
      </div>

      {below?.(t)}

      <div
        className="grid gap-2 md:gap-3 mt-3 md:mt-4"
        style={{
          gridTemplateColumns: `repeat(${chapters.length}, minmax(0, 1fr))`,
        }}
      >
        {chapters.map((c, i) => {
          const end = chapters[i + 1]?.at ?? total;
          const p = clamp((t - c.at) / (end - c.at));
          return (
            <button
              className="focus-ring group flex flex-col gap-2 pt-1 text-left cursor-pointer"
              key={c.name}
              onClick={() => seek(c.at)}
              type="button"
            >
              <span className="h-[3px] rounded-full bg-white/12 overflow-hidden">
                <span
                  className="block h-full bg-clover"
                  style={{ width: `${p * 100}%` }}
                />
              </span>
              <span
                className={`text-[12px] md:text-[14px] transition-colors ${chapter === c ? 'text-white' : 'text-white/45 group-hover:text-white/75'}`}
              >
                {c.name}
              </span>
            </button>
          );
        })}
      </div>
    </figure>
  );
}

// ---- The sample ------------------------------------------------------------------------------------

export const FIRST = 'Classify this support ticket.';
export const RULE = 'Read sarcasm as what the customer means.';

/** triage.baml as baml fmt writes it. `prompt` is the prompt's first line, and `rule` is a line added
 *  under it, which moves every later line down one. */
export function source(prompt: string, rule?: string) {
  return [
    'type Label = "billing" | "bug" | "other";',
    '',
    'function classify(ticket: string) -> Label {',
    '    client: "openai/gpt-6-luna"',
    '    prompt: `',
    `        ${prompt}`,
    ...(rule === undefined ? [] : [`        ${rule}`]),
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
}

export const span = (from: number, to: number) =>
  Array.from({ length: to - from + 1 }, (_, k) => from + k);

const LABEL_QUERY = `baml query "SELECT output_value AS label, count(*) FROM spans WHERE span_name = 'classify' GROUP BY label"`;
export const OTHER_QUERY = `baml query "SELECT input FROM spans WHERE span_name = 'classify' AND output_value = 'other' LIMIT 3"`;
// What baml pack prints: the package, the function it packs, and the platform.
export const PACKED = 'Finished triage [route, aarch64-apple-darwin] in 4s';

const LABELS = (since: number) => (
  <Bars
    rows={[
      { label: 'billing', value: 855 },
      { label: 'bug', value: 318 },
      { bad: true, label: 'other', value: 31 },
    ]}
    since={since}
  />
);

// ---- Version 22: the loop --------------------------------------------------------------------------

const LOOP_RULE_AT = 19300;

const LOOP: Script = {
  captions: [
    { at: 0, text: 'Write a BAML function in your editor.' },
    { at: 3200, text: 'Check it, and test it against the real model.' },
    { at: 7600, text: 'Pack it into one executable.' },
    { at: 9400, text: 'In production, every call is recorded.' },
    { at: 10200, text: 'Query those calls with SQL.' },
    { at: 13200, text: 'Your agent runs the same commands.' },
    { at: 19000, text: 'It fixes the prompt, tests it, and packs it again.' },
    { at: 25200, text: 'Then it checks the fix in production.' },
    { at: 30500, text: 'Build, test, pack, and query with one toolchain.' },
  ],
  chapters: [
    { at: 0, name: 'Build' },
    { at: 4800, name: 'Test' },
    { at: 7600, name: 'Pack' },
    { at: 9400, name: 'Query' },
    { at: 13200, name: 'Fix' },
  ],
  code: (t) =>
    t >= LOOP_RULE_AT
      ? source(FIRST, typed(RULE, t, LOOP_RULE_AT, 900))
      : source(typed(FIRST, t, 600, 1500)),
  deco: (t) => {
    const fixed = t >= LOOP_RULE_AT;
    const lines: Record<number, Tone> = {};
    if (within(t, 4800, 7000) || within(t, 21000, 22200)) {
      for (const i of span(2, fixed ? 9 : 8)) lines[i] = 'focus';
    }
    if (fixed) lines[6] = 'ok';
    return {
      caret: within(t, 600, 2200)
        ? 5
        : within(t, LOOP_RULE_AT, LOOP_RULE_AT + 1000)
          ? 6
          : undefined,
      lines,
      marks: within(t, 7600, 9000) ? [{ line: 10, text: 'route' }] : [],
    };
  },
  hold: 29500,
  label:
    'A product video of BAML in two windows, an editor and a terminal. A developer writes a classify function in triage.baml, checks it with baml check, tests it with baml test, and packs route into one executable with baml pack. In production, baml query counts calls by label, and 31 went to other. In the same terminal an agent queries those calls, finds sarcastic tickets, adds a line to the prompt, tests and packs it, and after the next deploy reports that calls labelled other fell from 3.1% to 0.3%.',
  terminal: [
    { at: 3200, kind: 'cmd', text: 'baml check' },
    {
      at: 3900,
      kind: 'out',
      text: 'Checked triage.baml. 0 errors.',
      tone: 'ok',
    },
    { at: 4800, kind: 'cmd', text: 'baml test classify' },
    {
      at: 5500,
      kind: 'out',
      right: '1.21s',
      text: 'refund_request',
      tone: 'pass',
    },
    { at: 5700, kind: 'out', right: '1.08s', text: 'export_bug', tone: 'pass' },
    {
      at: 5900,
      kind: 'out',
      right: '0.97s',
      text: 'plan_question',
      tone: 'pass',
    },
    {
      at: 6100,
      kind: 'out',
      right: '1.44s',
      text: 'angry_refund',
      tone: 'pass',
    },
    { at: 6300, kind: 'out', text: '5 more', tone: 'muted' },
    { at: 6600, kind: 'out', text: '9 passed, 0 failed', tone: 'ok' },
    { at: 7600, kind: 'cmd', text: 'baml pack route' },
    { at: 8400, kind: 'out', text: PACKED, tone: 'ok' },
    { at: 9400, kind: 'sep', text: 'deployed, a day later' },
    { at: 10200, kind: 'cmd', text: LABEL_QUERY },
    { at: 11000, kind: 'node', render: LABELS },
    { at: 13200, kind: 'cmd', text: 'agent' },
    { at: 13900, kind: 'user', text: 'Why are 31 tickets labelled other?' },
    { at: 15200, kind: 'tool', text: OTHER_QUERY },
    { at: 15900, kind: 'node', render: (since) => <Inputs since={since} /> },
    {
      at: 17200,
      kind: 'agent',
      text: 'They are sarcastic. I will add a line to the prompt and turn these 31 calls into tests.',
    },
    { at: 19000, kind: 'tool', text: 'edit triage.baml' },
    { at: 19500, kind: 'out', text: `+ ${RULE}`, tone: 'ok' },
    { at: 21000, kind: 'tool', text: 'baml test classify' },
    { at: 21700, kind: 'out', text: '40 passed, 0 failed', tone: 'ok' },
    { at: 22600, kind: 'tool', text: 'baml pack route' },
    { at: 23300, kind: 'out', text: PACKED, tone: 'ok' },
    {
      at: 24000,
      kind: 'agent',
      text: 'Packed. Deploy it, and I will compare the two versions.',
    },
    { at: 25200, kind: 'sep', text: 'deployed, a day later' },
    { at: 26000, kind: 'user', text: 'How did the fix do?' },
    {
      at: 26900,
      kind: 'tool',
      text: `baml query "SELECT version, avg(output_value = 'other') AS other FROM spans WHERE span_name = 'classify' GROUP BY version"`,
    },
    {
      at: 27700,
      kind: 'node',
      render: (since) => (
        <Bars
          format={(v) => `${v.toFixed(1)}%`}
          rows={[
            { bad: true, label: 'v42', value: 3.1 },
            { good: true, label: 'v43', value: 0.3 },
          ]}
          since={since}
        />
      ),
    },
    {
      at: 29000,
      kind: 'agent',
      text: 'Calls labelled other fell from 3.1% to 0.3%.',
    },
  ],
  total: 34000,
};

export default function HeroVideo() {
  return <Player script={LOOP} />;
}

// ---- Version 24: the code lights up ----------------------------------------------------------------

const CODE_RULE_AT = 25000;

// The first query watches: calls per minute keep arriving until the next command stops it at WATCH_END.
export const WATCH_SHOWN = 1500;
const WATCH_END = 5800;
const WATCH_BARS = 36;
const WATCH_BAR_MS = 650;

/** Calls so far, `since` ms into the watch: three more with each bar. */
export const watchedCalls = (since: number) =>
  1204 +
  Math.floor(clamp(since, 0, WATCH_END - WATCH_SHOWN) / WATCH_BAR_MS) * 3;

/** Noise in [0, 1) for bar i, so the same bars come back each time the video loops. */
function watchNoise(i: number) {
  const x = Math.sin(i * 12.9898 + 4.1) * 43758.5453;
  return x - Math.floor(x);
}

/** A live chart of calls per minute over a table of each function's calls and p50. The newest bar fills
 *  on the right while the chart slides left, and the call counts go up with it. */
export function Watch({ since }: { since: number }) {
  const t = Math.min(since, WATCH_END - WATCH_SHOWN);
  const live = since < WATCH_END - WATCH_SHOWN;
  const first = Math.floor(t / WATCH_BAR_MS);
  const progress = (t % WATCH_BAR_MS) / WATCH_BAR_MS;
  const bars = Array.from({ length: WATCH_BARS + 1 }, (_, k) => first + k);
  const calls = watchedCalls(since).toLocaleString('en-US');
  return (
    <div className="mt-1">
      <div className="relative h-[84px] overflow-hidden">
        <div
          className="absolute inset-y-0 left-0 flex items-end gap-[2px]"
          style={{
            transform: `translateX(${(-progress / (WATCH_BARS + 1)) * 100}%)`,
            width: `${((WATCH_BARS + 1) / WATCH_BARS) * 100}%`,
          }}
        >
          {bars.map((i, k) => {
            const value =
              0.45 + 0.22 * Math.sin(i / 3.3) + 0.3 * (watchNoise(i) - 0.5);
            const failed = watchNoise(i * 7 + 3) < 0.08;
            const last = k === WATCH_BARS;
            return (
              <span
                className="flex-1 min-w-0 rounded-t-[1px]"
                key={i}
                style={{
                  background: failed ? RED : 'var(--color-clover)',
                  height: `${Math.max(4, value * 100 * (last ? progress : 1))}%`,
                  opacity: failed ? 1 : 0.6,
                }}
              />
            );
          })}
        </div>
      </div>
      <p className="flex justify-between mt-1 text-[11.5px] text-white/35">
        <span>30 min ago</span>
        <span>calls per minute</span>
        <span className="inline-flex items-center gap-1.5 text-clover">
          {live && (
            <span className="size-1.5 rounded-full bg-current animate-pulse" />
          )}
          {live ? 'watching' : 'stopped'}
        </span>
      </p>
      <div className="grid grid-cols-[minmax(0,1fr)_auto_auto] gap-x-6 mt-2">
        <span className="text-white/35">span_name</span>
        <span className="text-white/35 text-right">calls</span>
        <span className="text-white/35 text-right">p50</span>
        {[
          ['route', '1.4s'],
          ['classify', '1.3s'],
        ].map(([name, p50]) => (
          <Fragment key={name}>
            <span className="text-white/80">{name}</span>
            <span className="text-right tabular-nums text-white/80">
              {calls}
            </span>
            <span className="text-right text-white/80">{p50}</span>
          </Fragment>
        ))}
      </div>
    </div>
  );
}

function codeDeco(t: number): Deco {
  const ends: NonNullable<Deco['ends']> = [];
  const marks: NonNullable<Deco['marks']> = [];
  const lines: NonNullable<Deco['lines']> = {};
  const deco: Deco = { ends, lines, marks };
  const fixed = t >= CODE_RULE_AT;

  // Calls and latency per function, at the end of each function's first line.
  if (within(t, 600, 5800)) {
    marks.push({ line: 2, text: 'classify' }, { line: 10, text: 'route' });
    const calls = watchedCalls(t - WATCH_SHOWN).toLocaleString('en-US');
    if (t >= WATCH_SHOWN) ends.push({ hint: `${calls} calls  1.4s`, line: 10 });
    if (t >= WATCH_SHOWN + 220)
      ends.push({ hint: `${calls} calls  1.3s`, line: 2 });
  }
  // Labels, on the type that declares them.
  if (within(t, 5800, 11000)) {
    marks.push({ line: 2, text: 'classify' });
    if (t >= 6600) marks.push({ hint: '855', line: 0, text: '"billing"' });
    if (t >= 6820) marks.push({ hint: '318', line: 0, text: '"bug"' });
    if (t >= 7040)
      marks.push({ hint: '31', line: 0, text: '"other"', tone: 'bad' });
  }
  // Routes, on the return that produced each one.
  if (within(t, 11000, 16400)) {
    marks.push({ line: 10, text: 'route' });
    if (t >= 11800) marks.push({ hint: '74%', line: 15, text: '"support"' });
    if (t >= 12020)
      marks.push({ hint: '26%', line: 13, text: '"engineering"' });
  }
  // The alert, from when it fires until it resolves.
  if (within(t, 17100, 31500)) {
    deco.alarm = { firing: true, line: 2 };
    ends.push({ hint: 'other 3.1%, above 2%', line: 2, tone: 'bad' });
  }
  if (t >= 31500) {
    deco.alarm = { firing: false, line: 2 };
    ends.push({ hint: 'other 0.3%, resolved', line: 2, tone: 'ok' });
  }
  // The agent reads the calls behind the alert, so the prompt that produced them lights up.
  if (within(t, 21300, CODE_RULE_AT)) {
    for (const i of span(4, 7)) lines[i] = 'focus';
    marks.push({ line: 0, text: '"other"', tone: 'bad' });
  }
  if (within(t, 26700, 27900)) {
    for (const i of span(2, 9)) lines[i] = 'focus';
  }
  if (fixed) deco.added = [6];
  if (within(t, CODE_RULE_AT, CODE_RULE_AT + 1000)) deco.caret = 6;
  return deco;
}

export function AlertRow({ resolved }: { resolved?: boolean }) {
  return (
    <p className="flex items-center gap-2.5">
      <Alarm firing={!resolved} />
      <span style={{ color: resolved ? 'var(--color-pasture-200)' : RED }}>
        {resolved ? 'resolved' : 'firing'}
      </span>
      <span className="text-white/85">classify</span>
      <span className="truncate text-white/50">
        {resolved
          ? 'other is 0.3% of calls'
          : 'other is 3.1% of calls, above 2%'}
      </span>
    </p>
  );
}

const CODE: Script = {
  chapters: [
    { at: 0, name: 'Calls' },
    { at: 5800, name: 'Labels' },
    { at: 11000, name: 'Routes' },
    { at: 16400, name: 'Alert' },
    { at: 19500, name: 'Fix' },
  ],
  code: (t) =>
    t >= CODE_RULE_AT
      ? source(FIRST, typed(RULE, t, CODE_RULE_AT, 900))
      : source(FIRST),
  deco: codeDeco,
  // Tall enough for the file with the fix in it, so the terminal sits beside the code it lights up.
  height: 480,
  hold: 22500,
  label:
    'A product video of BAML in two windows, an editor with triage.baml and a terminal. The first query watches calls per minute on a live chart. Each baml query highlights the code it is about: calls and latency on each function, label counts on the Label type (billing 855, bug 318, other 31), and the share of tickets on each return in route (support 74%, engineering 26%). baml alerts shows an alert firing on classify, and an alarm appears next to the function. An agent queries the sarcastic tickets behind it, adds a line to the prompt, tests and packs it, and a day later the alert has resolved.',
  terminal: [
    {
      at: 600,
      kind: 'cmd',
      text: `baml query --watch "SELECT span_name, date_trunc('minute', start_time) AS minute, count(*) AS calls, quantile(0.5)(latency) AS p50 FROM spans GROUP BY span_name, minute"`,
    },
    {
      at: WATCH_SHOWN,
      kind: 'node',
      render: (since) => <Watch since={since} />,
    },
    { at: 5800, kind: 'cmd', text: LABEL_QUERY },
    { at: 6600, kind: 'node', render: LABELS },
    {
      at: 11000,
      kind: 'cmd',
      text: `baml query "SELECT output_value AS route, count(*) FROM spans WHERE span_name = 'route' GROUP BY route"`,
    },
    {
      at: 11800,
      kind: 'node',
      render: (since) => (
        <Bars
          format={(v) => `${v}%`}
          rows={[
            { label: 'support', value: 74 },
            { label: 'engineering', value: 26 },
          ]}
          since={since}
        />
      ),
    },
    { at: 16400, kind: 'cmd', text: 'baml alerts' },
    { at: 17100, kind: 'node', render: () => <AlertRow /> },
    { at: 19500, kind: 'cmd', text: 'agent' },
    { at: 20100, kind: 'user', text: 'Fix the alert on classify.' },
    { at: 21300, kind: 'tool', text: OTHER_QUERY },
    { at: 22000, kind: 'node', render: (since) => <Inputs since={since} /> },
    {
      at: 23300,
      kind: 'agent',
      text: 'They are sarcastic. I will add a line to the prompt and turn these calls into tests.',
    },
    { at: 24800, kind: 'tool', text: 'edit triage.baml' },
    { at: 25300, kind: 'out', text: `+ ${RULE}`, tone: 'ok' },
    { at: 26700, kind: 'tool', text: 'baml test classify' },
    { at: 27400, kind: 'out', text: '40 passed, 0 failed', tone: 'ok' },
    { at: 28300, kind: 'tool', text: 'baml pack route' },
    { at: 29000, kind: 'out', text: PACKED, tone: 'ok' },
    { at: 30000, kind: 'sep', text: 'deployed, a day later' },
    { at: 30800, kind: 'tool', text: 'baml alerts' },
    { at: 31500, kind: 'node', render: () => <AlertRow resolved /> },
    {
      at: 32800,
      kind: 'agent',
      text: 'The alert on classify resolved. Calls labelled other fell from 3.1% to 0.3%.',
    },
  ],
  total: 38000,
};

export function HeroVideoCode() {
  return <Player script={CODE} />;
}
