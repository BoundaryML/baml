'use client';

import { type ReactNode, useLayoutEffect, useRef, useState } from 'react';
import { section } from './analytics';
import {
  clamp,
  type Deco,
  Editor,
  type Entry,
  FIRST,
  source,
  Terminal,
  Window,
  within,
} from './hero-video';
import { ramp, useScrollProgress } from './scroll';
import { RED } from './story';

// Section 3: triage.baml in an editor. A terminal opens over the code, and the user asks the agent
// three things in turn: to watch the file's functions for errors, for the top errors, and to say when
// OpenAI starts failing. Each answer is a small card printed in the terminal, and the code beside it
// picks up the same numbers. Scrolling plays it: the frame holds in the middle of the window while the
// reader scrolls through the section, and the scroll position sets the time. Under reduced motion it
// isn't pinned and shows the end, and the rail picks a moment instead.

const OPEN = 700;
const ASK_WATCH = 1500;
const WATCH_SHOWN = 3000;
const ASK_TOP = 5400;
const TOP_SHOWN = 6900;
const ASK_ALERT = 9300;
const ALERT_SHOWN = 10800;
const END = 13000;

// `at` is where a step starts. `show` is where clicking it jumps: just before the next step, once the
// step's card and the agent's reply are fully in.
const STEPS = [
  { at: 0, name: 'Code', show: ASK_WATCH - 100 },
  { at: ASK_WATCH, name: 'Watch', show: ASK_TOP - 500 },
  { at: ASK_TOP, name: 'Top errors', show: ASK_ALERT - 500 },
  { at: ASK_ALERT, name: 'Alert', show: END - 300 },
];

const LINES = source(FIRST);
// Lines of triage.baml the cards point back to.
const CLASSIFY = 2;
const CLIENT = 3;
const ROUTE = 10;

const LABEL =
  'triage.baml open in an editor. A terminal opens over the code, and the user asks the agent to watch these functions for errors. It prints a live card with calls and error rate for classify and route. Asked for the top errors, it prints a ranked list led by OpenAI rate limits on classify. Asked to say when OpenAI starts failing, it sets an alert for an OpenAI failure rate above 3%.';

// ---- The cards the agent prints --------------------------------------------------------------------

/** A card the agent prints in the terminal: a title row with a note on the right, over a body. */
function Card({
  children,
  right,
  title,
}: {
  children: ReactNode;
  right?: ReactNode;
  title: ReactNode;
}) {
  return (
    <div className="mt-2 rounded-lg border border-white/12 bg-white/[0.035] overflow-hidden">
      <div className="h-8 flex items-center gap-2 px-3 border-b border-white/[0.08] font-sans text-[12px] text-white/75">
        {title}
        <span className="ml-auto text-white/40">{right}</span>
      </div>
      <div className="px-3 py-2.5">{children}</div>
    </div>
  );
}

/** Calls per minute over the last hour, with the share that failed. Example data. */
const MINUTES = Array.from({ length: 48 }, (_, i) => {
  const calls = 0.62 + 0.22 * Math.sin(i / 4.2) + 0.1 * Math.sin(i * 1.7);
  const errors = i % 11 === 7 ? 0.18 : i % 5 === 2 ? 0.05 : 0;
  return { calls, errors };
});

/** One function's calls per minute as bars, with the failed share in red. Bars appear as `since`
 *  grows. */
function Spark({ scale, since }: { scale: number; since: number }) {
  const shown = Math.floor(since / 18);
  return (
    <svg
      aria-hidden="true"
      className="block w-full h-7"
      preserveAspectRatio="none"
      viewBox={`0 0 ${MINUTES.length * 4} 28`}
    >
      {MINUTES.slice(0, shown).map((m, i) => {
        const h = m.calls * 26 * scale;
        const e = m.errors * 26 * scale;
        return (
          // biome-ignore lint/suspicious/noArrayIndexKey: one bar per minute, in order
          <g key={i}>
            <rect
              fill="var(--color-clover)"
              height={h - e}
              opacity={0.55}
              width={3}
              x={i * 4}
              y={28 - h}
            />
            {e > 0 && (
              <rect fill={RED} height={e} width={3} x={i * 4} y={28 - e} />
            )}
          </g>
        );
      })}
    </svg>
  );
}

const WATCHED = [
  { calls: 12480, errors: '0.4%', name: 'classify', scale: 1 },
  { calls: 12480, errors: '0.1%', name: 'route', scale: 0.96 },
];

/** The card for the watch request: calls and error rate over the last hour for classify and route. */
function WatchCard({ since }: { since: number }) {
  const grow = ramp(since, 0, 900);
  return (
    <Card
      right="last hour"
      title={
        <>
          <span className="size-1.5 rounded-full bg-clover motion-safe:animate-pulse" />
          Watching 2 functions
        </>
      }
    >
      <div className="grid grid-cols-[72px_minmax(0,1fr)_64px_56px] gap-x-4 gap-y-2 items-center">
        <span className="text-[11px] text-white/35">function</span>
        <span className="text-[11px] text-white/35">calls per minute</span>
        <span className="text-right text-[11px] text-white/35">calls</span>
        <span className="text-right text-[11px] text-white/35">errors</span>
        {WATCHED.map((f) => (
          <div className="contents" key={f.name}>
            <span className="text-white/85">{f.name}</span>
            <Spark scale={f.scale} since={since} />
            <span className="text-right tabular-nums text-white/70">
              {Math.round(f.calls * grow).toLocaleString('en-US')}
            </span>
            <span className="text-right tabular-nums text-white">
              {f.errors}
            </span>
          </div>
        ))}
      </div>
    </Card>
  );
}

const TOP_ERRORS = [
  { count: 312, fn: 'classify', text: 'openai: 429 Too Many Requests' },
  { count: 118, fn: 'classify', text: 'openai: timed out after 30s' },
  { count: 41, fn: 'classify', text: 'parse: "maybe" is not a Label' },
  { count: 9, fn: 'route', text: 'Unroutable { label: "other" }' },
];

/** The card for the top errors request: a ranked list of errors over the last day, one row at a time. */
function TopErrorsCard({ since }: { since: number }) {
  const max = TOP_ERRORS[0].count;
  return (
    <Card right="last 24 hours" title="Top errors">
      <div className="grid grid-cols-[minmax(0,1fr)_64px_120px_36px] gap-x-4 gap-y-1.5 items-center">
        {TOP_ERRORS.slice(0, Math.floor(since / 160) + 1).map((e) => (
          <div className="contents" key={e.text}>
            <span className="truncate text-white/85">{e.text}</span>
            <span className="text-white/40">{e.fn}</span>
            <span className="h-2 rounded-[2px] bg-white/[0.06] overflow-hidden">
              <span
                className="block h-full rounded-[2px]"
                style={{
                  background: RED,
                  opacity: 0.85,
                  width: `${(e.count / max) * 100 * ramp(since, 0, 500)}%`,
                }}
              />
            </span>
            <span className="text-right tabular-nums text-white/70">
              {e.count}
            </span>
          </div>
        ))}
      </div>
    </Card>
  );
}

/** A small alarm clock with a face, which rings for a moment when the alert is set. */
function AlarmClock({ since }: { since: number }) {
  const ring = clamp(1 - since / 1100);
  const angle = Math.sin(since / 42) * 12 * ring;
  return (
    <svg
      aria-hidden="true"
      className="text-clover"
      fill="none"
      height="34"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.8"
      style={{ transform: `rotate(${angle}deg)` }}
      viewBox="0 0 32 32"
      width="34"
    >
      <path d="M4.2 10.4a4.6 4.6 0 0 1 6.4-6.4Z" fill="currentColor" />
      <path d="M27.8 10.4a4.6 4.6 0 0 0-6.4-6.4Z" fill="currentColor" />
      <circle cx="16" cy="17.5" r="9.5" />
      <circle cx="12.6" cy="16.4" fill="currentColor" r="1" stroke="none" />
      <circle cx="19.4" cy="16.4" fill="currentColor" r="1" stroke="none" />
      <path d="M13.4 20.4q2.6 2.2 5.2 0" />
      <path d="M9.5 26 7.6 28.4M22.5 26l1.9 2.4" />
      {ring > 0 && (
        <g opacity={ring}>
          <path d="M1.8 16.5h-1M30.2 16.5h1M2.6 21.5l-1 .6M29.4 21.5l1 .6" />
        </g>
      )}
    </svg>
  );
}

// The alert's threshold and where OpenAI's failure rate is now, both in percent, on a 0 to 5% track.
const THRESHOLD = 3;
const NOW = 0.4;
const TRACK = 5;

/** The card for the alert request: the OpenAI failure rate against its 3% threshold, marked armed. */
function AlertCard({ since }: { since: number }) {
  return (
    <Card right="checks every minute" title="New alert">
      <div className="flex items-center gap-4">
        <span className="size-12 shrink-0 grid place-items-center rounded-xl border border-clover/30 bg-clover/10">
          <AlarmClock since={since} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="font-sans text-[15px] text-white">
            OpenAI failure rate &gt; 3%
          </p>
          <div className="mt-2 flex items-center gap-3 text-[11px] text-white/45">
            <span className="relative h-1.5 flex-1 rounded-full bg-white/[0.08]">
              <span
                className="absolute inset-y-0 left-0 rounded-full bg-clover"
                style={{
                  width: `${(NOW / TRACK) * 100 * ramp(since, 0, 600)}%`,
                }}
              />
              <span
                className="absolute -inset-y-1 w-[2px] rounded-full"
                style={{
                  background: RED,
                  left: `${(THRESHOLD / TRACK) * 100}%`,
                }}
              />
            </span>
            <span className="tabular-nums">now {NOW}%</span>
          </div>
        </div>
        <span className="shrink-0 flex items-center gap-1.5 rounded-full border border-clover/40 px-2.5 py-1 font-sans text-[12px] text-clover">
          <span className="size-1.5 rounded-full bg-clover" />
          Armed
        </span>
      </div>
    </Card>
  );
}

// ---- The session -----------------------------------------------------------------------------------

const ENTRIES: Entry[] = [
  {
    at: ASK_WATCH,
    kind: 'user',
    text: 'Can you watch these functions for errors?',
  },
  {
    at: ASK_WATCH + 900,
    kind: 'tool',
    text: 'baml watch triage.baml --errors',
  },
  {
    at: WATCH_SHOWN,
    kind: 'node',
    render: (since) => <WatchCard since={since} />,
  },
  {
    at: WATCH_SHOWN + 1000,
    kind: 'agent',
    text: 'Watching classify and route. Both are healthy.',
  },
  { at: ASK_TOP, kind: 'user', text: 'What are the top errors?' },
  {
    at: ASK_TOP + 900,
    kind: 'tool',
    text: `baml query "SELECT error, count(*) FROM spans WHERE status = 'error' GROUP BY error"`,
  },
  {
    at: TOP_SHOWN,
    kind: 'node',
    render: (since) => <TopErrorsCard since={since} />,
  },
  {
    at: TOP_SHOWN + 1000,
    kind: 'agent',
    text: 'Most are OpenAI rate limits on classify.',
  },
  { at: ASK_ALERT, kind: 'user', text: 'Tell me when OpenAI starts failing.' },
  {
    at: ASK_ALERT + 900,
    kind: 'tool',
    text: `baml alerts add "failure_rate(provider = 'openai') > 3%"`,
  },
  {
    at: ALERT_SHOWN,
    kind: 'node',
    render: (since) => <AlertCard since={since} />,
  },
  {
    at: ALERT_SHOWN + 1000,
    kind: 'agent',
    text: "Done. I'll message you when it fires.",
  },
];

/** The code picks up what the terminal is talking about: the watched functions and their error rates,
 *  then the model behind the top errors, then the alert on it. */
function deco(t: number): Deco {
  const marks: NonNullable<Deco['marks']> = [];
  const result: Deco = { marks };
  if (within(t, ASK_WATCH + 400, ASK_TOP)) {
    const shown = t >= WATCH_SHOWN;
    marks.push(
      {
        hint: shown ? '0.4% errors' : undefined,
        line: CLASSIFY,
        text: 'classify',
      },
      { hint: shown ? '0.1% errors' : undefined, line: ROUTE, text: 'route' },
    );
  }
  if (within(t, ASK_TOP + 400, ASK_ALERT)) {
    marks.push({
      hint: t >= TOP_SHOWN ? '430 errors' : undefined,
      line: CLIENT,
      text: '"openai/gpt-6-luna"',
      tone: 'bad',
    });
  }
  if (t >= ALERT_SHOWN) {
    result.alarm = { firing: false, line: CLIENT };
    marks.push({
      hint: 'alert above 3%',
      line: CLIENT,
      text: '"openai/gpt-6-luna"',
    });
  }
  return result;
}

// ---- The frame -------------------------------------------------------------------------------------

// The frame is drawn at 1200 by 560 and scaled to fit. The terminal sits over the right of the editor,
// clear of the code, so the marks in the code stay readable.
const W = 1200;
const H = 560;

/** The editor and the terminal over it at time `t`, scaled to the frame's width. */
function Stage({ t }: { t: number }) {
  const frame = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState(0);
  useLayoutEffect(() => {
    const el = frame.current;
    if (!el) return;
    const measure = () => setFit(el.clientWidth / W);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const open = ramp(t, OPEN, OPEN + 600);

  return (
    <div className="min-w-0 flex-1">
      <div className="rounded-2xl border border-white/12 bg-[#050505] p-1.5 md:p-2">
        <div
          className="relative overflow-hidden rounded-[11px] bg-[#161618]"
          ref={frame}
          style={{ height: H * fit }}
        >
          {fit > 0 && (
            <div
              aria-hidden
              className="absolute left-0 top-0 origin-top-left"
              style={{ height: H, transform: `scale(${fit})`, width: W }}
            >
              <Window height={512} title="triage.baml" w={1152} x={24} y={24}>
                <Editor deco={deco(t)} lines={LINES} />
              </Window>
              {open > 0 && (
                <div
                  className="absolute inset-0"
                  style={{
                    opacity: open,
                    transform: `translateY(${(1 - open) * 40}px)`,
                  }}
                >
                  {/* A ring in the frame's colour keeps the terminal apart from the editor under it. */}
                  <Window
                    className="border-white/20 shadow-[0_0_0_6px_#161618]"
                    height={440}
                    title="~/triage"
                    w={642}
                    x={510}
                    y={96}
                  >
                    <Terminal entries={ENTRIES} t={t} />
                  </Window>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/** The steps beside the frame: down a line on wide screens, a row on narrow ones. The current step is
 *  lit, and clicking one jumps to it. */
function Rail({ seek, t }: { seek: (t: number) => void; t: number }) {
  const current = STEPS.reduce((last, step) => (t >= step.at ? step : last));
  return (
    <>
      <ol className="hidden lg:flex flex-col w-[110px] shrink-0 pt-2">
        {STEPS.map((step, i) => {
          const live = step === current;
          const reached = t >= step.at;
          const next = STEPS[i + 1];
          return (
            <li className="relative pb-8 last:pb-0" key={step.name}>
              {next && (
                <span
                  aria-hidden
                  className={`absolute left-[5px] top-4 bottom-0 w-px transition-colors duration-500 ${t >= next.at ? 'bg-clover/70' : 'bg-rule'}`}
                />
              )}
              <button
                className={`v4-caption focus-ring relative flex items-center gap-3 transition-colors ${live ? 'text-head' : 'text-muted hover:text-ink-2'}`}
                onClick={() => seek(step.show)}
                type="button"
              >
                <span
                  aria-hidden
                  className={`size-[11px] shrink-0 rounded-full border transition-colors duration-500 ${live ? 'border-clover bg-clover' : reached ? 'border-clover/70 bg-ground' : 'border-rule bg-ground'}`}
                />
                {step.name}
              </button>
            </li>
          );
        })}
      </ol>

      <div className="grid grid-cols-4 gap-2 lg:hidden">
        {STEPS.map((step, i) => {
          const end = STEPS[i + 1]?.at ?? END;
          return (
            <button
              className="focus-ring flex flex-col gap-2 pt-1 text-left"
              key={step.name}
              onClick={() => seek(step.show)}
              type="button"
            >
              <span className="h-[3px] rounded-full bg-rule overflow-hidden">
                <span
                  className="block h-full bg-clover"
                  style={{
                    width: `${clamp((t - step.at) / (end - step.at)) * 100}%`,
                  }}
                />
              </span>
              <span
                className={`text-[13px] ${step === current ? 'text-head' : 'text-muted'}`}
              >
                {step.name}
              </span>
            </button>
          );
        })}
      </div>
    </>
  );
}

/** Section 3: the pinned editor and terminal frame with its step rail, played by scrolling. */
export default function Workflow() {
  const track = useRef<HTMLDivElement>(null);
  const pin = useRef<HTMLDivElement>(null);
  const [progress, scrollTo, still] = useScrollProgress(track, pin);
  // Under reduced motion the rail picks the moment; otherwise scrolling does.
  const [picked, setPicked] = useState<number | null>(null);
  const t = still ? (picked ?? END) : (progress ?? 0) * END;
  const seek = (ms: number) => (still ? setPicked(ms) : scrollTo(ms / END));

  return (
    <section
      {...section('how')}
      aria-labelledby="how"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) pt-20 md:pt-28 pb-4">
        <h2 className="v4-h2 max-w-[20em] text-balance" id="how">
          Build, deploy, observe, and improve with one CLI.
        </h2>
      </div>
      {/* The frame runs wider than the rest of the page so the editor and terminal are readable. */}
      <div
        className="mx-auto max-w-[1560px] px-(--frame-pad) mt-10 md:mt-14"
        ref={track}
      >
        <div className="motion-safe:sticky" ref={pin}>
          <figure
            aria-label={LABEL}
            className="pb-8 flex flex-col lg:flex-row gap-6 lg:gap-10"
          >
            <Rail seek={seek} t={t} />
            <Stage t={t} />
          </figure>
        </div>
        {/* Scroll room for the session while the frame above is pinned. */}
        <div aria-hidden className="motion-reduce:hidden h-[180vh]" />
      </div>
    </section>
  );
}
