'use client';

import { type ReactNode, useEffect, useId, useRef, useState } from 'react';
import { Phase } from './phase';
import { useReducedMotion, useVisible } from './pinned';
import { ramp } from './scroll';

// Section 7, Improve: what's coming, as one collage a dot steps through. HyperReload's tile is an
// experiment moving classify from gpt-6-luna to gpt-6-sol a share of traffic at a time: gpt-6-sol takes
// over in three steps over three days, with what each day cost under it. The experiment runs while the
// dot is on the tile, filling in from Monday to Wednesday.

const CLOVER = 'var(--color-clover)';
const GREY = 'rgb(from var(--color-ink) r g b / 0.16)';

// Hours across the chart, Monday 00:00 to Thursday 00:00.
const HOURS = 72;
// gpt-6-sol's share of traffic, as [hour, share] corners. Each step ramps up over an hour and a half.
const ROLLOUT: [number, number][] = [
  [0, 0],
  [10, 0],
  [11.5, 0.1],
  [34, 0.1],
  [35.5, 0.5],
  [58, 0.5],
  [59.5, 1],
  [HOURS, 1],
];

/** gpt-6-sol's share of traffic at an hour, between the corners. */
function share(hour: number) {
  for (let i = 1; i < ROLLOUT.length; i++) {
    const [h1, s1] = ROLLOUT[i];
    if (hour <= h1) {
      const [h0, s0] = ROLLOUT[i - 1];
      return s0 + ((hour - h0) / (h1 - h0)) * (s1 - s0);
    }
  }
  return 1;
}

// What a call costs. classify sends about 300 tokens and gets a one-word label back. Prices are
// OpenAI's list prices in dollars per million tokens, input then output. Example traffic.
const CALLS_PER_HOUR = 4200;
const TOKENS = { input: 300, output: 5 };
const PRICE = { 'gpt-6-luna': [0.1, 0.5], 'gpt-6-sol': [2, 10] } as const;
/** Dollars for one call at a model's [input, output] price per million tokens. */
const perCall = ([input, output]: readonly [number, number]) =>
  (TOKENS.input * input + TOKENS.output * output) / 1e6;
const LUNA = perCall(PRICE['gpt-6-luna']);
const SOL = perCall(PRICE['gpt-6-sol']);

/** Dollars spent on classify between two hours. */
function spend(from: number, to: number) {
  let total = 0;
  for (let h = from; h < to; h += 0.25) {
    const s = share(Math.min(h + 0.125, to));
    const dt = Math.min(0.25, to - h);
    total += CALLS_PER_HOUR * dt * (LUNA * (1 - s) + SOL * s);
  }
  return total;
}

/** A dollar amount formatted as US currency with two decimals. */
const dollars = (v: number) =>
  v.toLocaleString('en-US', {
    currency: 'USD',
    maximumFractionDigits: 2,
    minimumFractionDigits: 2,
    style: 'currency',
  });
/** A per-call cost written as the cost of a thousand calls. */
const per1k = (v: number) => `${dollars(v * 1000)} per 1k calls`;

const W = 720;
const H = 200;
/** The chart x position of an hour. */
const x = (hour: number) => (hour / HOURS) * W;
/** The chart y position of a share of traffic. */
const y = (s: number) => H - s * H;

const EDGE = ROLLOUT.map(
  ([h, s], i) => `${i ? 'L' : 'M'}${x(h).toFixed(1)},${y(s).toFixed(1)}`,
).join('');

// The step labels sit just above each step of gpt-6-sol's area, in the middle of the step. Each
// appears when the experiment reaches it.
const STEPS = [
  { at: 11.5, hour: 22.75, label: '10%', share: 0.1 },
  { at: 35.5, hour: 46.75, label: '50%', share: 0.5 },
  { at: 59.5, hour: 65.75, label: '100%', share: 1 },
];

const DAYS = ['Mon', 'Tue', 'Wed'];

const MODELS = [
  {
    name: 'gpt-6-luna',
    note: `3.1% labelled other, ${per1k(LUNA)}`,
    swatch: GREY,
  },
  {
    name: 'gpt-6-sol',
    note: `0.3% labelled other, ${per1k(SOL)}`,
    swatch: CLOVER,
  },
];

/** The rollout chart: gpt-6-sol's share of traffic over three days and each day's spend. `t` is how far
 *  the collage has played this tile, so the experiment runs while the dot is on it, not on scroll. */
function RolloutChart({ t }: { t: number }) {
  // How far the experiment has run, in hours.
  const now = Math.min(1, Math.max(0, t / 0.9)) * HOURS;
  const id = useId();

  return (
    <div className="p-5 md:p-6">
      <div className="flex flex-wrap gap-x-8 gap-y-2 text-[13px] md:text-[14px]">
        {MODELS.map((m) => (
          <p className="flex items-center gap-2.5" key={m.name}>
            <span
              className="size-2.5 rounded-[2px]"
              style={{ background: m.swatch }}
            />
            <span className="font-mono text-ink">{m.name}</span>
            <span className="text-muted">{m.note}</span>
          </p>
        ))}
      </div>

      {/* The plot is drawn for sighted readers; screen readers get the same experiment as a sentence. */}
      <p className="sr-only">
        gpt-6-sol took 10% of classify's traffic on Monday, 50% on Tuesday, and
        100% on Wednesday, replacing gpt-6-luna. The three days cost{' '}
        {DAYS.map((_, i) => dollars(spend(i * 24, (i + 1) * 24))).join(', ')}.
      </p>
      <div
        aria-hidden
        className="mt-6 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3"
      >
        {/* Share of traffic, recessive: just the ends and the middle, as labels with no gridlines. */}
        <div className="relative w-8 h-44 font-mono text-[11px] text-muted">
          {['100%', '50%', '0'].map((label, i) => (
            <span
              className="absolute right-0 -translate-y-1/2"
              key={label}
              style={{ top: `${i * 50}%` }}
            >
              {label}
            </span>
          ))}
        </div>

        <div className="relative h-44">
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full"
            preserveAspectRatio="none"
            viewBox={`0 0 ${W} ${H}`}
          >
            <defs>
              <clipPath id={id}>
                <rect height={H} width={x(now)} />
              </clipPath>
            </defs>
            {/* Only the hours the experiment has run are drawn. */}
            <g clipPath={`url(#${id})`}>
              <rect fill={GREY} height={H} width={W} />
              <path
                d={`${EDGE}L${W},${H}L0,${H}Z`}
                fill={CLOVER}
                opacity={0.55}
              />
              {/* A gap in the page's surface colour between the two areas. */}
              <path
                d={EDGE}
                fill="none"
                stroke="var(--color-raised)"
                strokeWidth={2}
                vectorEffect="non-scaling-stroke"
              />
            </g>
          </svg>

          {now > 0 && now < HOURS && (
            <span
              className="absolute inset-y-0 w-px bg-ink/40"
              style={{ left: `${(now / HOURS) * 100}%` }}
            />
          )}
          <span
            className="absolute left-3 top-3 font-mono text-[12px] md:text-[13px] text-ink"
            style={{ opacity: ramp(now, 0, 6) }}
          >
            gpt-6-luna
          </span>
          {STEPS.map((s) => (
            <span
              className="absolute -translate-x-1/2 font-mono text-[12px] md:text-[13px] text-ink"
              key={s.label}
              style={{
                bottom:
                  s.share === 1
                    ? undefined
                    : `calc(${s.share * 100}% + 0.4rem)`,
                left: `${(s.hour / HOURS) * 100}%`,
                opacity: ramp(now, s.at, s.at + 4),
                top: s.share === 1 ? '0.75rem' : undefined,
                transform: `translate(-50%, ${(1 - ramp(now, s.at, s.at + 4)) * 6}px)`,
              }}
            >
              {s.label}
            </span>
          ))}
        </div>

        <div />
        <div className="grid grid-cols-3 mt-2 font-mono text-[11px]">
          {DAYS.map((d, i) => {
            const start = i * 24;
            return (
              <div className="flex flex-col gap-1" key={d}>
                <span className="text-muted">{d}</span>
                <span
                  className="tabular-nums text-ink-2"
                  style={{ opacity: now > start ? 1 : 0 }}
                >
                  {dollars(spend(start, Math.min(now, start + 24)))}
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// ---------- the collage ----------

// Everything that's coming, as one collage you can take in at a glance. A dot runs along under the
// names, one feature at a time, and the feature it's on lights up and plays; then it wraps around to
// the first and keeps going. None of these has shipped.

const RED = '#f85149';

// Where the run lives over time: one lane per machine, as in the durable-functions demo.
const LANES = ['local', 'cloud-1', 'cloud-2'];
// The recorded demo of durable functions, on YouTube.
const DEMO = 'https://youtu.be/vAJA9iGXjcc';
// One run, hopping machines: it moves to cloud-1, cloud-1 dies mid-run, cloud-2 picks it up from the last
// snapshot, and it finishes back on local. Each hop is [lane, start, end] along the run, 0 to 1; the
// failed one ends in a crash instead of a handoff.
const HOPS: [number, number, number][] = [
  [0, 0, 0.26],
  [1, 0.26, 0.5],
  [2, 0.5, 0.76],
  [0, 0.76, 1],
];
// The hop whose machine dies.
const FAILS = 1;
// Where the label column ends and the lanes begin.
const GUTTER = '3.5rem';
// Lanes are 1rem tall, 0.5rem apart: a lane's middle, from the top of the stack.
const laneMiddle = (lane: number) => `${0.5 + lane * 1.5}rem`;

/** One run across machines: local, then cloud-1, which dies, then cloud-2 from the last snapshot, and
 *  back to local to finish. */
function Durable({ t }: { t: number }) {
  // How far along the run has got, 0 to 1.
  const now = Math.min(1, t / 0.9);
  const at = HOPS.findIndex(([, from, to]) => now >= from && now < to);
  const current = at === -1 ? HOPS.length - 1 : at;
  const done = now >= 1;
  const across = (x: number) => `calc(${GUTTER} + (100% - ${GUTTER}) * ${x})`;
  return (
    <div>
      <div className="relative space-y-2">
        {LANES.map((lane, i) => (
          <div className="flex items-center gap-2" key={lane}>
            <span
              className={`w-12 shrink-0 font-mono text-[11px] transition-colors duration-300 ${HOPS[current][0] === i && !done ? 'text-clover' : 'text-muted'}`}
            >
              {lane}
            </span>
            <div className="relative h-4 flex-1 rounded-[3px] bg-white/[0.04]">
              {HOPS.map(([lane2, from, to]) =>
                lane2 === i && now > from ? (
                  <span
                    className="absolute inset-y-0 rounded-[3px] bg-clover"
                    key={from}
                    style={{
                      backgroundColor:
                        lane2 === HOPS[FAILS][0] && now >= to ? RED : undefined,
                      left: `${from * 100}%`,
                      width: `${(Math.min(now, to) - from) * 100}%`,
                    }}
                  />
                ) : null,
              )}
            </div>
          </div>
        ))}
        {/* Where cloud-1 died. */}
        {now >= HOPS[FAILS][2] && (
          <span
            className="absolute -translate-y-1/2 pl-1 font-mono text-[11px] font-semibold"
            style={{
              color: RED,
              left: across(HOPS[FAILS][2]),
              top: laneMiddle(HOPS[FAILS][0]),
            }}
          >
            ✗
          </span>
        )}
        {/* The hops: a line from where the run paused down or up to where it picks back up, with the
            snapshot it carries. */}
        {HOPS.slice(1).map(([lane, from], k) => {
          const prev = HOPS[k][0];
          if (now < from) return null;
          const top = Math.min(prev, lane);
          const span = Math.abs(lane - prev);
          return (
            <span key={from}>
              <span
                className="absolute w-px bg-clover/70"
                style={{
                  height: `${span * 1.5}rem`,
                  left: across(from),
                  top: laneMiddle(top),
                }}
              />
              <span
                className="absolute size-2 -translate-x-1/2 -translate-y-1/2 rotate-45 border border-black/40 bg-[#f5c451]"
                style={{ left: across(from), top: laneMiddle(prev) }}
              />
            </span>
          );
        })}
      </div>
      <p className="mt-3 font-mono text-[12px]">
        {done ? (
          <span className="text-clover">
            ✓ one run, finished: cloud-1 died, cloud-2 picked it up
          </span>
        ) : current === FAILS + 1 ? (
          <span style={{ color: RED }}>
            cloud-1 died, resuming on cloud-2 from the last snapshot
          </span>
        ) : (
          <span className="text-muted">
            running on {LANES[HOPS[current][0]]}…
          </span>
        )}
      </p>
      {/* The demo, as a card that opens it: drawn here, not a screenshot of the video. */}
      <a
        className="focus-ring group mt-3 flex items-center gap-3 rounded-lg border border-white/10 bg-white/[0.03] p-2 transition-colors hover:border-clover/50 hover:bg-white/[0.05]"
        href={DEMO}
        rel="noopener noreferrer"
        target="_blank"
      >
        <span className="grid size-8 shrink-0 place-items-center rounded-md bg-clover text-black transition-transform group-hover:scale-105">
          <svg aria-hidden="true" className="ml-0.5 size-4" viewBox="0 0 16 16">
            <path d="M4 2.5v11l9-5.5z" fill="currentColor" />
          </svg>
        </span>
        <span className="min-w-0">
          <span className="block text-[14px] text-head">
            Watch the proof of concept ✓
          </span>
          <span className="block font-mono text-[11px] text-muted">
            It works. BEP incoming to design it for reliability.
          </span>
        </span>
        <span className="ml-auto shrink-0 font-mono text-[12px] text-clover">
          ↗
        </span>
      </a>
    </div>
  );
}

// Inputs as dots: a test suite tries a handful, a proof covers all of them.
const TESTED = 5;
const EVERY = 84;

function Dots({ count, lit }: { count: number; lit: number }) {
  return (
    <div className="grid w-fit grid-cols-[repeat(14,0.5rem)] gap-1">
      {Array.from({ length: count }, (_, i) => (
        <span
          className={`size-2 rounded-full transition-colors duration-200 ${i < lit ? 'bg-clover' : 'bg-white/10'}`}
          // biome-ignore lint/suspicious/noArrayIndexKey: a fixed drawing
          key={i}
        />
      ))}
    </div>
  );
}

/** A rule checked two ways: by tests, against the few inputs someone wrote down, and by a proof,
 *  against every input there is. */
function Verify({ t }: { t: number }) {
  const tests = Math.min(TESTED, Math.floor((t / 0.2) * TESTED));
  const proof = Math.min(
    EVERY,
    Math.floor(Math.max(0, (t - 0.2) / 0.6) * EVERY),
  );
  const done = proof >= EVERY;
  return (
    <div className="flex h-full flex-col gap-4">
      <div className="rounded-md border border-white/8 bg-white/[0.02] px-2.5 py-2 font-mono text-[12px] text-ink-2">
        <span className="text-clover">refund</span>(order) ≤ order.total
      </div>
      <div>
        <p className="mb-1.5 flex justify-between font-mono text-[11px] text-muted">
          <span>tests</span>
          <span>{TESTED} inputs</span>
        </p>
        <Dots count={TESTED} lit={tests} />
      </div>
      <div>
        <p className="mb-1.5 flex justify-between font-mono text-[11px] text-muted">
          <span>proof</span>
          <span>every input</span>
        </p>
        <Dots count={EVERY} lit={proof} />
      </div>
      <p className="mt-auto font-mono text-[11px]">
        {done ? (
          <span className="text-clover">✓ holds for every input</span>
        ) : (
          <span className="text-muted">checking…</span>
        )}
      </p>
    </div>
  );
}

// One benchmark on one machine (Apple M2 Max): quicksort on the native bench's quick-random input at
// 65,536 integers (seed 1729). Median per-job wall time of 20 trials; a second 20-trial run agreed within
// about 2%. BAML rows are built from the ahead-of-time Rust backend with native telemetry (#5240); the
// Rust, Go, Node and Python rows are line-for-line ports of the same BAML. Times are in microseconds.
// Tracing records each call, not each loop iteration, so it adds nothing measurable here; the "on" row
// is the highest level.
const SPEEDS: { lit?: boolean; name: string; time: number }[] = [
  { lit: true, name: 'BAML interpreted', time: 151_600 },
  { name: 'Python', time: 111_700 },
  { name: 'Node', time: 6560 },
  { lit: true, name: 'BAML compiled', time: 6070 },
  { lit: true, name: '↳ tracing on', time: 6080 },
  { name: 'Go', time: 4930 },
  { name: 'Rust', time: 4000 },
];

// The axis runs to just past Node, where the compiled rows can be told apart. Anything slower runs off
// the end, drawn broken, with its time beside it.
const AXIS = 7500;

/** A time in microseconds, to three significant figures, in ms or µs. */
const duration = (us: number) =>
  us >= 1000 ? `${(us / 1000).toPrecision(3)} ms` : `${us.toPrecision(3)} µs`;

/** The same work timed several ways: each bar is how long it takes, so shorter is better. Static: the
 *  numbers are the point, not a race. */
function Native() {
  return (
    <div>
      <p className="mb-2 flex justify-between gap-3 font-mono text-[11px]">
        <span className="text-ink-2">
          quicksort, 65,536 random ints · run on Apple M2 Max
        </span>
        <span className="shrink-0 text-muted">shorter is better</span>
      </p>
      <div className="space-y-1">
        {SPEEDS.map((s) => {
          const off = s.time > AXIS;
          const lit = Boolean(s.lit);
          return (
            <div className="flex items-center gap-2" key={s.name}>
              <span
                className={`w-[7.5rem] shrink-0 whitespace-nowrap font-mono text-[11px] ${lit ? 'text-clover' : 'text-muted'}`}
              >
                {s.name}
              </span>
              <div className="relative h-2.5 flex-1 rounded-[3px] bg-white/[0.04]">
                <span
                  className={`absolute inset-y-0 left-0 rounded-[3px] ${lit ? 'bg-clover' : 'bg-white/30'}`}
                  style={{ width: `${Math.min(1, s.time / AXIS) * 100}%` }}
                />
                {off && (
                  // The break: the bar keeps going past the end of the axis.
                  <span className="absolute -inset-y-0.5 right-3 w-1.5 -skew-x-[20deg] bg-raised" />
                )}
              </div>
              <span
                className={`w-14 shrink-0 text-right font-mono text-[11px] tabular-nums ${lit ? 'text-clover' : 'text-ink-2'}`}
              >
                {duration(s.time)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// What a sandbox costs, as one BAML task on the interpreter (M2 Max): about 13 µs to start and finish
// one, and about 31 KB to hold one parked. Shown as rounded-up bounds.
const SANDBOX_STATS: [string, string][] = [
  ['<20 µs', 'to start'],
  ['<50 KB', 'of memory'],
  ['Zero', 'infrastructure'],
];

// A sandbox from BEP-063, in its proposed syntax: a memory cap, a deadline, and an allow-list of IO.
// Everything not allowed is denied.
const SANDBOX_CODE: [string, string?][][] = [
  [['let', 'k'], [' sb = sandbox.'], ['new', 'f'], ['(']],
  [['  max_memory = '], ['2 GB', 'n'], [',']],
  [['  deadline = '], ['30s', 'n'], [',']],
  [['  allow = [FsRead])']],
  [['sb.'], ['run', 'f'], ['(() -> '], ['agent', 'f'], ['(task))']],
];
const CODE_TONE: Record<string, string> = {
  c: 'text-muted',
  f: 'text-clover',
  k: 'text-ink-2',
  n: 'text-head',
};

// What the agent's code tries, in order, and what the sandbox does about it.
const ATTEMPTS: { at: number; call: string; ok: boolean; result: string }[] = [
  { at: 0.12, call: 'fs.read("docs/faq.md")', ok: true, result: 'allowed' },
  { at: 0.3, call: 'http.get("pastebin.com")', ok: false, result: 'IoDenied' },
  { at: 0.52, call: 'spawn 50k tasks', ok: false, result: 'MemoryExceeded' },
];

/** A sandbox enforcing its policies on an agent's code: what's allowed runs, everything else is stopped,
 *  and the program around it carries on. */
function Sandbox({ t }: { t: number }) {
  const caught = t > 0.75;
  return (
    <div className="flex h-full flex-col gap-1.5">
      <pre className="rounded-md border border-dashed border-clover/60 px-3 py-2 font-mono text-[12px] leading-[1.4] text-body">
        {SANDBOX_CODE.map((runs, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the sample's lines never reorder.
          <span className="block whitespace-pre" key={i}>
            {runs.map(([text, tone], j) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: a line's runs never reorder.
              <span className={tone ? CODE_TONE[tone] : undefined} key={j}>
                {text}
              </span>
            ))}
          </span>
        ))}
      </pre>
      <div className="space-y-1.5">
        {ATTEMPTS.map((a) => {
          const shown = t >= a.at;
          return (
            <div
              className="flex items-center justify-between gap-3 rounded-md border border-white/8 bg-white/[0.02] px-2.5 py-1.5 font-mono text-[11px] transition-opacity duration-300"
              key={a.call}
              style={{ opacity: shown ? 1 : 0.25 }}
            >
              <span className="truncate text-ink-2">{a.call}</span>
              <span
                className={`shrink-0 ${a.ok ? 'text-clover' : ''}`}
                style={a.ok ? undefined : { color: RED }}
              >
                {shown ? `${a.ok ? '✓' : '✗'} ${a.result}` : ''}
              </span>
            </div>
          );
        })}
      </div>
      <p className="mt-auto font-mono text-[11px]">
        {caught ? (
          <span className="text-clover">
            ✓ caught, your program keeps running
          </span>
        ) : (
          <span className="text-muted">agent running in the sandbox…</span>
        )}
      </p>
      {/* So nobody takes these for a VM: they hold no state between runs. */}
      <p className="border-t border-white/8 pt-2 text-[12px] leading-snug text-muted">
        For stateless code. Long-lived, stateful environments still need a
        traditional VM sandbox.
      </p>
    </div>
  );
}

// How far along each status is, as the color of its dot: built, being built, written down, not begun.
const STAGE: Record<string, string> = {
  Designing: 'bg-white/50',
  'In progress': 'bg-clover',
  'Not started': 'border border-white/40',
  Proposed: 'bg-white/50',
  'Weekend prototype': 'bg-clover',
};

// Furthest along first. Formal verification hasn't started, and says so plainly
// and comes last.
const FEATURES: {
  // The rollout chart draws edge to edge, so its tile has no inner padding.
  bleed?: boolean;
  draw: (t: number) => ReactNode;
  ms: number;
  name: string;
  says: ReactNode;
  // Where it is today, and where to see it: a BEP, a pull request, a demo.
  status?: {
    label: string;
    links?: { href: string; label: string }[];
    // What's next, when it isn't a link yet.
    note?: string;
  };
  tile: string;
}[] = [
  {
    draw: () => <Native />,
    ms: 3800,
    name: 'Compiled to native',
    says: 'Interpret for instant builds. Compile for instant execution.',
    status: {
      label: 'In progress',
      links: [
        {
          href: 'https://github.com/BoundaryML/baml/pull/5218',
          label: 'PR #5218',
        },
      ],
    },
    tile: 'lg:col-span-7',
  },
  {
    draw: (t) => <Durable t={t} />,
    ms: 4600,
    name: 'Durable execution',
    says: 'Pause a run on one machine and pick it up on another.',
    status: {
      label: 'Weekend prototype',
      links: [
        {
          href: 'https://github.com/BoundaryML/baml/pull/4956',
          label: 'PR #4956',
        },
      ],
    },
    tile: 'lg:col-span-5',
  },
  {
    draw: (t) => <Sandbox t={t} />,
    ms: 4200,
    name: 'HyperSandboxes',
    says: (
      <>
        Milliseconds are too slow, megabytes too big.
        {/* Below sm each column is as wide as its own text: in equal thirds "infrastructure" ran
            out of its column and past the card's padding. */}
        <span className="mt-2 grid grid-cols-[auto_auto_auto] justify-between gap-2 sm:grid-cols-3 sm:justify-normal">
          {SANDBOX_STATS.map(([value, label]) => (
            <span className="flex flex-col" key={label}>
              <span className="font-display text-[20px] font-medium leading-none text-clover">
                {value}
              </span>
              <span className="mt-1 font-mono text-[11px] text-muted">
                {label}
              </span>
            </span>
          ))}
        </span>
      </>
    ),
    status: {
      label: 'Proposed',
      links: [
        { href: 'https://beps.boundaryml.com/beps/63', label: 'BEP-063' },
      ],
    },
    tile: 'lg:col-span-4',
  },
  {
    bleed: true,
    draw: (t) => <RolloutChart t={t} />,
    ms: 5200,
    name: 'HyperReload',
    says: 'Roll out experiments 100x faster than CI/CD.',
    status: { label: 'Designing' },
    tile: 'sm:col-span-2 lg:col-span-5',
  },
  {
    draw: (t) => <Verify t={t} />,
    ms: 4200,
    name: 'Formal verification',
    says: 'Prove a rule holds for every input, not just the ones you tested.',
    status: { label: 'Not started' },
    tile: 'lg:col-span-3',
  },
];

/** The collage and the dot that steps through it. Clicking a feature holds it while the pointer
 *  stays on the collage; leaving lets the loop carry on. */
function Collage() {
  const ref = useRef<HTMLDivElement>(null);
  const dot = useRef<HTMLSpanElement>(null);
  // The loop runs only while the collage is on screen, and not at all for reduced motion, where
  // every tile shows its last frame.
  const visible = useVisible(ref);
  const reduce = useReducedMotion();
  const [stage, setStage] = useState(0);
  const [held, setHeld] = useState(false);
  // How far the lit feature has played, 0 to 1. Features that aren't lit show their last frame.
  const [t, setT] = useState(1);
  const at = useRef(0);
  useEffect(() => {
    at.current = stage;
  }, [stage]);
  useEffect(() => {
    const el = dot.current;
    if (!el) return;
    if (!visible || held || reduce) {
      el.style.opacity = '0';
      setT(1);
      return;
    }
    let start = performance.now();
    let frame = 0;
    const tick = (now: number) => {
      let p = (now - start) / FEATURES[at.current].ms;
      if (p >= 1) {
        start = now;
        p = 0;
        at.current = (at.current + 1) % FEATURES.length;
        setStage(at.current);
      }
      setT(p);
      const x = (at.current + p) / FEATURES.length;
      // Fades out at the right end and back in at the left, so the wrap reads as a loop.
      const edge = Math.min(
        1,
        x * FEATURES.length * 4,
        (1 - x) * FEATURES.length * 4,
      );
      el.style.opacity = String(edge);
      el.style.transform = `translate3d(${x * 100}%, 0, 0)`;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [visible, held, reduce]);
  // Scrolling away lets go of a pick, so the loop is running again when the reader comes back.
  useEffect(() => {
    if (!visible) setHeld(false);
  }, [visible]);
  const pick = (i: number) => {
    setHeld(true);
    setStage(i);
    setT(1);
  };
  return (
    // A pick holds until the mouse leaves or focus leaves the collage. Touch fires its leave before
    // the click, so it's left out, and a touch pick holds until the collage scrolls away.
    <div
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setHeld(false);
      }}
      onPointerLeave={(e) => {
        if (e.pointerType === 'mouse') setHeld(false);
      }}
      ref={ref}
    >
      <ol
        className="grid gap-2"
        style={{
          gridTemplateColumns: `repeat(${FEATURES.length}, minmax(0, 1fr))`,
        }}
      >
        {FEATURES.map((f, i) => {
          const on = i === stage;
          return (
            <li className="min-w-0" key={f.name}>
              <button
                aria-current={on ? 'step' : undefined}
                // Below md only the number shows, so the name is given here at every width.
                aria-label={f.name}
                className="focus-ring flex w-full cursor-pointer items-start gap-2.5 rounded-lg py-1.5 pr-2 text-left transition-colors hover:bg-white/5"
                onClick={() => pick(i)}
                type="button"
              >
                <span
                  className={`grid size-7 shrink-0 place-items-center rounded-full border font-mono text-[13px] transition-colors ${on ? 'border-clover bg-clover text-black' : 'border-white/20 text-white/50'}`}
                >
                  {i + 1}
                </span>
                <span
                  className={`hidden pt-1 text-[14px] leading-snug transition-colors md:block ${on ? 'text-white' : 'text-white/50'}`}
                >
                  {f.name}
                </span>
              </button>
            </li>
          );
        })}
      </ol>
      {/* The track the dot runs along, under the names. */}
      <div
        aria-hidden
        className="relative mt-2 h-px overflow-x-clip bg-white/15"
      >
        <span
          className="absolute inset-x-0 top-0 block will-change-[transform,opacity]"
          ref={dot}
          style={{ opacity: 0 }}
        >
          <span className="absolute left-0 top-0 size-[7px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-clover" />
        </span>
      </div>
      <div className="mt-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-12">
        {FEATURES.map((f, i) => {
          const on = i === stage;
          return (
            <div
              className={`flex min-w-0 flex-col rounded-xl border bg-raised transition-[border-color,opacity] duration-300 ${f.tile} ${on ? 'border-clover/70 opacity-100' : 'border-rule opacity-60 hover:opacity-90'}`}
              key={f.name}
            >
              {/* Where it stands, then its name, then what it does: each on its own line, so a long name
                  or a long status never squeezes the other. */}
              <div className="px-4 pt-4">
                {f.status && (
                  <p className="mb-2 flex flex-wrap items-center gap-x-2 gap-y-1 font-mono text-[11px] uppercase tracking-wider">
                    <span className="flex items-center gap-1.5 text-ink-2">
                      <span
                        className={`size-1.5 rounded-full ${STAGE[f.status.label] ?? 'bg-white/30'}`}
                      />
                      {f.status.label}
                    </span>
                    {f.status.note && (
                      <span className="text-clover">{f.status.note}</span>
                    )}
                    {f.status.links?.map((l) => (
                      <a
                        className="tap focus-ring rounded-md normal-case tracking-normal text-clover hover:underline"
                        href={l.href}
                        key={l.href}
                        rel="noopener noreferrer"
                        target="_blank"
                      >
                        {l.label} ↗
                      </a>
                    ))}
                  </p>
                )}
                <button
                  // Taller on a touch screen, pulled back by its margins so the card's rhythm holds.
                  className="focus-ring flex cursor-pointer items-center gap-2.5 whitespace-nowrap rounded-md text-left font-display text-[17px] font-medium text-head md:text-[18px] pointer-coarse:py-2 pointer-coarse:-my-2"
                  onClick={() => pick(i)}
                  type="button"
                >
                  <span
                    className={`grid size-5 place-items-center rounded-full font-mono text-[11px] ${on ? 'bg-clover text-black' : 'border border-white/20 text-white/50'}`}
                  >
                    {i + 1}
                  </span>
                  {f.name}
                </button>
                <p className="mt-1 pl-[1.875rem] text-[14px] leading-snug text-ink-2">
                  {f.says}
                </p>
              </div>
              <div className={`flex-1 ${f.bleed ? '' : 'p-4'}`}>
                {f.draw(on ? t : 1)}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** The Improve phase: everything coming next, as one collage the dot steps through. */
export default function Improve() {
  return (
    <Phase
      heading="More things to look forward to."
      id="improve"
      name="Improve"
      soon
    >
      <Collage />
    </Phase>
  );
}
