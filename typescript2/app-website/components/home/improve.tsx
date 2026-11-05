'use client';

import { useId, useRef } from 'react';
import { Panel, Phase } from './phase';
import { ramp, useScrollProgress } from './scroll';

// Section 7, Improve: an experiment moves classify from gpt-6-luna to gpt-6-sol a share of traffic at
// a time. One chart shows gpt-6-sol taking over in three steps over three days, with what each day
// cost under it. Scrolling runs the experiment: the chart fills in from Monday to Wednesday, its steps
// climbing as they're reached, and each day's spend counts up as the day goes by.

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

/** The rollout chart: gpt-6-sol's share of traffic over three days and each day's spend, filled in as
 *  the reader scrolls. */
function RolloutChart() {
  const plot = useRef<HTMLDivElement>(null);
  const [progress] = useScrollProgress(plot, plot);
  // How far the experiment has run, in hours.
  const now = Math.min(1, Math.max(0, ((progress ?? 1) - 0.05) / 0.85)) * HOURS;
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
        <div className="relative w-8 h-44 md:h-56 font-mono text-[11px] text-muted">
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

        <div className="relative h-44 md:h-56" ref={plot}>
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

/** Section 7, Improve: the experiments phase, holding the rollout chart. */
export default function Improve() {
  return (
    <Phase
      heading="Roll out experiments 100x faster than your CI/CD runs."
      id="improve"
      intro="The BAML runtime can track experiments and A/B test functions securely for you, without having to redeploy."
      name="Improve"
      soon
    >
      <Panel title="gpt-6-sol's share of classify's traffic, and what it cost">
        <RolloutChart />
      </Panel>
    </Phase>
  );
}
