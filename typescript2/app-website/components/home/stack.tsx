'use client';

import { Cloud, EyeOff, Fingerprint, Layers as LayersIcon } from 'lucide-react';
import { type ReactNode, useEffect, useId, useRef, useState } from 'react';
import {
  SiBun,
  SiGo,
  SiNodedotjs,
  SiNpm,
  SiOpentelemetry,
  SiPython,
  SiRust,
  SiTypescript,
  SiV8,
} from 'react-icons/si';
import { section } from '@/components/home/analytics';
import { ALT_MARK } from '@/components/home/boundary-alt';
import { BoundaryLogo, BoundaryMark } from '@/components/home/boundary-logo';
import { LAMB_PATHS } from '@/components/home/lamb';
import { MarkTile } from '@/components/home/mark';
import { MARKS } from '@/components/home/marks';
import { Hyper, Looped, Pinned, Played, sweep } from '@/components/home/pinned';
import { ramp } from '@/components/home/scroll';

// The stack, then what it costs to see every function. BAML times every function, which measured
// 16 KB in all for a million calls, and captures only the AI calls, billed about 20,000 logical bytes
// each. To see the same functions, OpenTelemetry needs a span on each: 70.4 OTLP bytes empty, 20,099
// with the prompt. Duplicate content isn't billed: content stored once costs once, however many
// calls carry it. OpenTelemetry has no such dedupe, so every span is billed in full.
//
// The assumptions, each set to err against BAML, checked on a real computer-use agent's recordings
// (boundary/codeturtle, 2026-11: 424,492 function calls, 308 AI calls, 262 request bodies):
// - AI_SHARE: measured 1 in 1,378 there, an unusually AI-heavy app. A normal app has fewer, but
//   more AI calls cost BAML more, so 1 in 2,000 sits between them on the high side.
// - BAML_AI_BILLED: 20 KB. The text of a call there measured 7.4 KB (1.6 MB with screenshots,
//   which a typical app doesn't send). Larger calls shrink OpenTelemetry's span overhead, so the
//   larger figure is the harder case for BAML.
// - DUPLICATE_SHARE: the share of a call's bytes that repeats content already stored. Measured
//   68.5% there overall (system prompts 97%, text 63%, screenshots resent by later turns 68%) and
//   80% for the text alone; 65% is just under the lowest.

const AI_SHARE = 1 / 2000;
const DUPLICATE_SHARE = 0.65;
const OTEL_SPAN = 70.4;
const OTEL_AI_SPAN = 20_099;
const BAML_AI_BILLED = 20_000;
const CALLS = 1_000_000_000_000;

/** Noise in [0, 1) for point i, so a figure draws the same way on every render. */
function noise(i: number) {
  const v = Math.sin(i * 12.9898 + 78.233) * 43758.5453;
  return v - Math.floor(v);
}

const count = new Intl.NumberFormat('en-US', {
  maximumSignificantDigits: 3,
  notation: 'compact',
});

function data(bytes: number) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${units[unit]}`;
}

// ---------- the stack ----------

// Bottom to top. The first two are lit from the start.
const LAYERS = [
  'runtime',
  'language',
  'test',
  'review',
  'publish',
  'trace',
  'dashboards',
  'alerts',
  'and more',
];
const FIRST_LIT = 2;
const TOP = 12;
const STEP = 46;
const LAMB = 62;

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

// The figure's drawing space. Every layer, the lamb and each spark is its own full-size layer over the
// same space, drawn once. Scrolling only moves and fades those layers, which the GPU does without
// repainting anything, so it stays smooth.
// The space starts left of the stack, where the tools each layer replaces wait to be kicked off.
const VB_X = -100;
// Room above the stack for the lamb's last leap, to its place beside the Boundary mark.
const VB_TOP = -90;
const VB_W = 600;
const VB_H = TOP + LAYERS.length * STEP + 150;
const VIEW_BOX = `${VB_X} ${VB_TOP} ${VB_W} ${VB_H}`;
/** A distance in drawing units across or down, as a share of the figure, for a CSS translate. */
const across = (v: number) => `${(v / VB_W) * 100}%`;
const down = (v: number) => `${(v / VB_H) * 100}%`;
const LAYER = 'absolute inset-0 h-full w-full overflow-visible';
const MOVING = 'absolute inset-0 will-change-[transform,opacity]';

type Tone = 'dim' | 'lit' | 'hot';
const TONES: Record<
  Tone,
  {
    color: string;
    label: number;
    side: number;
    stroke: number;
    top: number;
    weight: number;
    width: number;
  }
> = {
  dim: {
    color: '#fff',
    label: 0.6,
    side: 0.1,
    stroke: 0.5,
    top: 0.06,
    weight: 400,
    width: 1.4,
  },
  hot: {
    color: 'var(--color-clover)',
    label: 1,
    side: 0.4,
    stroke: 1,
    top: 0.82,
    weight: 700,
    width: 2.6,
  },
  lit: {
    color: 'var(--color-clover)',
    label: 1,
    side: 0.35,
    stroke: 1,
    top: 0.56,
    weight: 700,
    width: 1.4,
  },
};

/** One layer of the stack drawn in one look. Each layer is drawn three times, dim, lit and hot, and
 *  scrolling only fades between them. */
function SlabArt({
  name,
  row,
  tone,
}: {
  name: string;
  row: number;
  tone: Tone;
}) {
  const y = TOP + (LAYERS.length - 1 - row) * STEP;
  const d = 14;
  const t = TONES[tone];
  return (
    <svg
      aria-hidden="true"
      className={LAYER}
      style={
        tone === 'hot'
          ? { filter: 'drop-shadow(0 0 14px rgb(202 170 242 / 0.7))' }
          : undefined
      }
      viewBox={VIEW_BOX}
    >
      <path
        d={`M180,${y}L340,${y + 44}L180,${y + 88}L20,${y + 44}Z`}
        fill={t.color}
        fillOpacity={t.top}
        stroke={t.color}
        strokeOpacity={t.stroke}
        strokeWidth={t.width}
      />
      <path
        d={`M20,${y + 44}V${y + 44 + d}L180,${y + 88 + d}L340,${y + 44 + d}V${y + 44}L180,${y + 88}Z`}
        fill={t.color}
        fillOpacity={t.side}
        stroke={t.color}
        strokeOpacity={t.stroke}
        strokeWidth="1.4"
      />
      <text
        fill={t.color}
        fillOpacity={t.label}
        fontFamily="ui-monospace,SFMono-Regular,Menlo,Consolas,monospace"
        fontSize="16"
        fontWeight={t.weight}
        x="362"
        y={y + 50}
      >
        {name}
      </text>
    </svg>
  );
}

/** The ground behind a layer, so the layers above hide the ones below. */
function SlabBase({ row }: { row: number }) {
  const y = TOP + (LAYERS.length - 1 - row) * STEP;
  const d = 14;
  return (
    <svg aria-hidden="true" className={LAYER} viewBox={VIEW_BOX}>
      <path
        d={`M180,${y}L340,${y + 44}V${y + 44 + d}L180,${y + 88 + d}L20,${y + 44 + d}V${y + 44}Z`}
        fill="var(--color-ground)"
      />
    </svg>
  );
}

// What each layer replaces, sitting beside it until BAML lights it up and kicks them off. Only tools
// BAML really does the job of: nothing for review, which BAML adds to rather than replaces.
const KICKED: Record<string, ReactNode[]> = {
  alerts: [
    <MarkTile key="s" mark={MARKS.sentry} size={16} />,
    <MarkTile key="p" mark={MARKS.pagerduty} size={16} />,
  ],
  'and more': [
    <MarkTile key="l" mark={MARKS.launchdarkly} size={16} />,
    <MarkTile key="s" mark={MARKS.statsig} size={16} />,
    <MarkTile key="p" mark={MARKS.posthog} size={16} />,
    <MarkTile key="t" mark={MARKS.temporal} size={16} />,
  ],
  dashboards: [
    <MarkTile key="g" mark={MARKS.grafana} size={16} />,
    <MarkTile key="m" mark={MARKS.metabase} size={16} />,
  ],
  language: [<SiTypescript key="t" />, <SiPython key="p" />],
  publish: [<SiNpm key="n" />],
  review: [],
  runtime: [<SiV8 key="v" />, <SiNodedotjs key="n" />, <SiBun key="b" />],
  test: [
    <MarkTile key="j" mark={MARKS.jest} size={16} />,
    <MarkTile key="v" mark={MARKS.vitest} size={16} />,
    <MarkTile key="m" mark={MARKS.mocha} size={16} />,
  ],
  trace: [
    <MarkTile key="d" mark={MARKS.datadog} size={16} />,
    <MarkTile key="h" mark={MARKS.honeycomb} size={16} />,
    <MarkTile key="l" mark={MARKS.langsmith} size={16} />,
  ],
};

// The burst at the top: small four-point stars flying out from the top layer.
const BURST = [-170, -140, -110, -80, -50, -20, 10, 40, 200, 230, 260, 300];
const SPARK =
  'M0,-7C1,-2 2,-1 7,0C2,1 1,2 0,7C-1,2 -2,1 -7,0C-2,-1 -1,-2 0,-7Z';
const TOP_Y = TOP + 44;
// Where the lamb stands at the start, on the language layer.
const LAMB_H = LAMB * (422 / 356);
const LAMB_Y = TOP + (LAYERS.length - 2) * STEP + 50 - LAMB_H;

function LambArt({ glow }: { glow?: boolean }) {
  return (
    <svg
      aria-hidden="true"
      className={LAYER}
      style={
        glow
          ? { filter: 'drop-shadow(0 0 12px rgb(202 170 242 / 0.9))' }
          : undefined
      }
      viewBox={VIEW_BOX}
    >
      <g
        fill="#fff"
        transform={`translate(${180 - LAMB / 2} ${LAMB_Y}) scale(${LAMB / 356})`}
      >
        {LAMB_PATHS.map((d) => (
          <path d={d} key={d} />
        ))}
      </g>
    </svg>
  );
}

// The finale: the two logos stand side by side on the top layer, the Boundary mark on the left and
// the BAML lamb on the right. The mark rises out of the layer; the lamb lands beside it. Both are
// LOGO_H tall.
const LOGO_H = 80;
const MARK_W = LOGO_H * (86 / 144);
const BIG_LAMB_W = LOGO_H * (356 / 422);
const LOGO_GAP = 18;
// The logos' feet, on the top layer's face.
const LOGO_BASE = TOP + 50;
const MARK_X = 180 - (MARK_W + LOGO_GAP + BIG_LAMB_W) / 2;
const BIG_LAMB_X = MARK_X + MARK_W + LOGO_GAP;

function LogoArt({
  glow,
  part,
}: {
  glow?: boolean;
  part: 'boundary' | 'lamb';
}) {
  const boundary = part === 'boundary';
  return (
    <svg
      aria-hidden="true"
      className={LAYER}
      style={
        glow
          ? { filter: 'drop-shadow(0 0 12px rgb(202 170 242 / 0.9))' }
          : undefined
      }
      viewBox={VIEW_BOX}
    >
      <g
        fill="#fff"
        transform={`translate(${boundary ? MARK_X : BIG_LAMB_X} ${LOGO_BASE - LOGO_H}) scale(${LOGO_H / (boundary ? 144 : 422)})`}
      >
        {(boundary ? ALT_MARK.paths : LAMB_PATHS).map((d) => (
          <path d={d} key={d} />
        ))}
      </g>
    </svg>
  );
}

function Layers({ p }: { p: number }) {
  const s = sweep(p);
  const climbs = LAYERS.length - FIRST_LIT;
  // It starts dark. First the runtime and the language pop in, with the first line of the heading
  // and the lamb landing on top of them.
  const base = ramp(s, 0.06, 0.28) * FIRST_LIT;
  // Then the rest of the stack, one layer at a time, as the lamb climbs.
  const rest = ramp(s, 0.34, 0.82) * climbs;
  // Once the lamb is on top, the Boundary mark rises out of the top layer and the lamb leaps to its
  // place beside it.
  const leap = ramp(s, 0.83, 0.93);
  const gate = ramp(leap, 0.1, 0.55);
  // Mid-air the climbing lamb becomes the larger one that stands beside the mark.
  const swap = ramp(leap, 0.3, 0.55);
  const arrive = ramp(leap, 0.3, 1);
  // Then the finale.
  const done = ramp(s, 0.88, 0.98);
  // How far along lighting each layer is, from 0 to 1.
  const lit = (row: number) =>
    row < FIRST_LIT ? clamp01(base - row) : clamp01(rest - (row - FIRST_LIT));
  // A layer pops up and glows while it lights, then settles.
  const pop = (row: number) => Math.sin(Math.PI * lit(row));
  // Between layers the lamb hops: up in an arc, then down onto the next one.
  const hop = rest < climbs ? Math.sin(Math.PI * (rest % 1)) * 30 : 0;
  const burst = Math.sin(Math.PI * done);
  // The first line arrives with the language layer; the lamb drops in just after.
  const first = ramp(base, 1, 2);
  const landed = ramp(base, 1.4, 2);
  return (
    <div className="grid md:grid-cols-[minmax(0,1fr)_minmax(0,1.05fr)] gap-x-12 gap-y-8 items-center">
      <h3 className="v4-h2 text-head">
        <span
          className="block origin-left will-change-[transform,opacity]"
          style={{
            opacity: first,
            transform: `translate3d(0, ${(1 - first) * 16}px, 0) scale(${1 + Math.sin(Math.PI * first) * 0.06})`,
          }}
        >
          BAML, the <Hyper>hyperobservable</Hyper> language.
        </span>
        {/* Two products, two lines: the second lands with the Boundary mark, a beat later. */}
        <span
          className="mt-5 md:mt-7 block origin-left will-change-[transform,opacity]"
          style={{
            opacity: done,
            transform: `translate3d(0, ${(1 - done) * 16}px, 0) scale(${1 + burst * 0.06})`,
          }}
        >
          Boundary Cloud, the <Hyper>hyperobservable</Hyper> stack.
        </span>
      </h3>
      <div
        aria-label="The BAML stack, from the runtime and language at the bottom to alerts and more at the top"
        className="relative w-full max-w-[620px]"
        role="img"
        style={{ aspectRatio: `${VB_W} / ${VB_H}` }}
      >
        {LAYERS.map((name, row) => {
          const on = lit(row);
          const hot = Math.max(pop(row), done * 0.45);
          return (
            <div
              className={MOVING}
              key={name}
              style={{
                transform: `translate3d(0, -${down(pop(row) * 12)}, 0)`,
              }}
            >
              <SlabBase row={row} />
              <div className={MOVING} style={{ opacity: 1 - on }}>
                <SlabArt name={name} row={row} tone="dim" />
              </div>
              <div className={MOVING} style={{ opacity: on }}>
                <SlabArt name={name} row={row} tone="lit" />
              </div>
              <div className={MOVING} style={{ opacity: hot }}>
                <SlabArt name={name} row={row} tone="hot" />
              </div>
            </div>
          );
        })}
        {/* The tools each layer replaces sit on it while it's dark, and get kicked off as it lights. */}
        {LAYERS.map((name, row) =>
          KICKED[name].map((tool, k, all) => {
            const t = lit(row);
            // In a row to the left of the layer, level with its name on the right.
            const y = TOP + (LAYERS.length - 1 - row) * STEP + 44;
            const x = -8 - (all.length - 1 - k) * 32;
            const dx = -t * (240 + k * 60);
            const dy = -Math.sin(Math.PI * t) * 90 + t * 70;
            return (
              <div
                className="absolute grid size-[26px] place-items-center rounded-md bg-black/70 text-[15px] text-white/85 will-change-[transform,opacity]"
                // biome-ignore lint/suspicious/noArrayIndexKey: a fixed list per layer
                key={`${name}-${k}`}
                style={{
                  left: `${((x - VB_X) / VB_W) * 100}%`,
                  opacity: 1 - ramp(t, 0.35, 0.95),
                  top: down(y - VB_TOP),
                  transform: `translate3d(calc(-50% + ${dx}px), calc(-50% + ${dy}px), 0) rotate(${-t * (150 + k * 50)}deg)`,
                }}
              >
                {tool}
              </div>
            );
          }),
        )}
        {/* The burst, once the lamb reaches the top. */}
        {BURST.map((deg, i) => {
          const a = (deg * Math.PI) / 180;
          const reach = 40 + done * (110 + (i % 3) * 30);
          return (
            <div
              className={MOVING}
              key={deg}
              style={{
                opacity: done > 0 && done < 1 ? burst : 0,
                transform: `translate3d(${across(Math.cos(a) * reach)}, ${down(Math.sin(a) * reach * 0.6)}, 0)`,
              }}
            >
              <svg aria-hidden="true" className={LAYER} viewBox={VIEW_BOX}>
                <path
                  d={SPARK}
                  fill={i % 2 ? '#fff' : 'var(--color-clover)'}
                  transform={`translate(180 ${TOP_Y}) scale(1.1)`}
                />
              </svg>
            </div>
          );
        })}
        {/* The lamb stands on the language, then hops up a layer as each one lights. */}
        <div
          className={MOVING}
          style={{
            opacity: landed * (1 - swap),
            transform: `translate3d(${across(leap * 30)}, -${down(rest * STEP + hop + (1 - landed) * 50 + Math.sin((Math.PI / 2) * Math.min(1, leap * 2)) * 50)}, 0)`,
          }}
        >
          <LambArt />
        </div>
        {/* The Boundary mark, rising out of the top layer. */}
        <div
          className={MOVING}
          style={{
            opacity: gate,
            transform: `translate3d(0, ${down((1 - gate) * 24)}, 0)`,
          }}
        >
          <LogoArt part="boundary" />
        </div>
        {/* The BAML lamb, landing beside the mark. */}
        <div
          className={MOVING}
          style={{
            opacity: swap,
            transform: `translate3d(${across(-(1 - arrive) * 34)}, ${down((1 - arrive) * 26)}, 0)`,
          }}
        >
          <LogoArt part="lamb" />
          <div className={MOVING} style={{ opacity: done * 0.8 }}>
            <LogoArt glow part="lamb" />
          </div>
        </div>
      </div>
    </div>
  );
}

// ---------- what it adds up to ----------

function Meter({
  label,
  lit,
  share,
  value,
}: {
  label: string;
  lit?: boolean;
  share: number;
  value: string;
}) {
  return (
    <div>
      <div className="flex items-baseline justify-between gap-3">
        <p className="text-[15px] text-white/70">{label}</p>
        <p
          className={`font-mono text-[28px] tabular-nums ${lit ? 'text-clover' : 'text-head'}`}
        >
          {value}
        </p>
      </div>
      <div className="mt-1 h-3 rounded-[3px] bg-white/8 overflow-hidden">
        <span
          className={`block h-full rounded-[3px] ${lit ? 'bg-clover' : 'bg-white/40'}`}
          style={{ minWidth: 3, width: `${share * 100}%` }}
        />
      </div>
    </div>
  );
}

const STRIP = 120;

/** One function in every box. Lit boxes are the ones that were recorded. */
function CallStrip({ lit, rate }: { lit?: boolean; rate: number }) {
  return (
    <div
      className="grid gap-[3px]"
      style={{ gridTemplateColumns: 'repeat(30, minmax(0, 1fr))' }}
    >
      {Array.from({ length: STRIP }, (_, i) => {
        const kept = noise(i) < rate;
        const error = noise(i + 999) < 0.12;
        return (
          <span
            className={`aspect-square rounded-[1px] ${
              error
                ? kept
                  ? 'bg-[#e5484d]'
                  : 'border border-[#e5484d]'
                : kept
                  ? lit
                    ? 'bg-clover'
                    : 'bg-white/80'
                  : 'bg-white/10'
            }`}
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed drawing
            key={i}
          />
        );
      })}
    </div>
  );
}

const otelBytes = (calls: number, rate: number) =>
  calls * rate * ((1 - AI_SHARE) * OTEL_SPAN + AI_SHARE * OTEL_AI_SPAN);
const bamlBytes = (calls: number, rate: number) =>
  calls * rate * AI_SHARE * BAML_AI_BILLED * (1 - DUPLICATE_SHARE);

type Rates = { baml: number; otel: number };

/** A ratio as "5×" or "4.6×". */
const times = (v: number) =>
  `${v >= 10 ? Math.round(v) : Number(v.toFixed(1))}×`;

/** BAML against OpenTelemetry at the current sampling, in one line that follows the sliders:
 *  how many more functions it records, in how much less storage. */
function versus(rates: Rates) {
  const functions = rates.baml / rates.otel;
  const storage = bamlBytes(CALLS, rates.baml) / otelBytes(CALLS, rates.otel);
  const first =
    functions > 1.05
      ? `${times(functions)} more functions recorded`
      : functions < 0.95
        ? `${Math.round(functions * 100)}% of the functions recorded`
        : 'The same functions recorded';
  const second =
    storage <= 0.5
      ? `${times(1 / storage)} less storage`
      : storage < 0.95
        ? `${Math.round(storage * 100)}% of the storage`
        : storage <= 1.05
          ? 'the same storage'
          : `${times(storage)} more storage`;
  return `${first}, in ${second}.`;
}

const LANGUAGES = [
  { Icon: SiPython, name: 'Python' },
  { Icon: SiTypescript, name: 'TypeScript' },
  { Icon: SiRust, name: 'Rust' },
  { Icon: SiGo, name: 'Go' },
];

/** OpenTelemetry plus one language at a time, on a line that stays the same length. */
function OtelName() {
  const [at, setAt] = useState(0);
  useEffect(() => {
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const timer = setInterval(
      () => setAt((i) => (i + 1) % LANGUAGES.length),
      2200,
    );
    return () => clearInterval(timer);
  }, []);
  return (
    <p className="flex items-center gap-2 whitespace-nowrap font-display text-[16px] sm:text-[20px] md:text-[22px] font-medium text-head">
      <SiOpentelemetry aria-hidden="true" className="shrink-0" />
      OpenTelemetry
      <span className="text-white/50">+</span>
      <span className="relative inline-block h-[1.3em] w-[7.2em]">
        {LANGUAGES.map(({ Icon, name }, i) => (
          <span
            aria-hidden={i !== at}
            className={`absolute inset-0 flex items-center gap-2 ${i === at ? 'transition-opacity duration-500' : ''}`}
            key={name}
            style={{ opacity: i === at ? 1 : 0 }}
          >
            <Icon aria-hidden="true" className="shrink-0" />
            {name}
          </span>
        ))}
      </span>
    </p>
  );
}

function Side({
  lit,
  n,
  rates,
  setRate,
}: {
  lit?: boolean;
  n: number;
  rates: Rates;
  setRate: (rate: number) => void;
}) {
  const rate = lit ? rates.baml : rates.otel;
  const functions = n * rate;
  const bytes = lit ? bamlBytes(n, rates.baml) : otelBytes(n, rates.otel);
  const most = otelBytes(CALLS, 1);
  // A column of the box, not a card of its own: BAML's gets the speed chart's clover highlight.
  return (
    <div className={`min-w-0 rounded-lg p-4 ${lit ? 'bg-clover/10' : ''}`}>
      <div className="flex h-[44px] items-center">
        {lit ? (
          <BoundaryLogo className="h-[40px] w-auto text-white" />
        ) : (
          <OtelName />
        )}
      </div>
      <div className="mt-5 flex flex-col gap-6">
        <Meter
          label="Functions recorded"
          lit={lit}
          share={functions / CALLS}
          value={count.format(functions)}
        />
        <Meter
          label="Storage"
          lit={lit}
          share={bytes / most}
          value={data(bytes)}
        />
        <div>
          <div className="mb-2 flex items-center justify-between gap-3 min-h-[28px]">
            <p className="text-[15px] text-white/70">Sampling</p>
            {lit && rate === 1 ? (
              <p className="font-mono text-[18px] text-clover">
                why would you?
              </p>
            ) : (
              // What sampling throws away, not what it keeps: that's the part you lose.
              <p
                className={`font-mono text-[18px] tabular-nums ${rate < 1 ? 'text-[#e5484d]' : lit ? 'text-clover' : 'text-head'}`}
              >
                drops {100 - Math.round(rate * 100)}%
              </p>
            )}
          </div>
          {/* BAML's starts at 100% ("why would you?"), but it can sample too. */}
          <input
            aria-label={`Share of ${lit ? 'Boundary' : 'OpenTelemetry'} functions kept by sampling`}
            // On a touch screen the slider is 40px tall to the thumb, pulled back by its margins so
            // the rows around it don't move.
            className="block h-4 w-full accent-[#caaaf2] pointer-coarse:h-10 pointer-coarse:-my-3"
            max={100}
            min={1}
            onChange={(e) => setRate(Number(e.target.value) / 100)}
            type="range"
            value={Math.round(rate * 100)}
          />
          {/* Readers took the slider for a gauge, so it says it moves. */}
          <p className="mt-1.5 text-[13px] text-white/45">
            Drag to change the sampling
          </p>
        </div>
        <div>
          <div className="mb-2 flex items-baseline justify-between gap-3">
            <p className="text-[15px] text-white/70">Errors lost</p>
            <p
              className={`font-mono text-[28px] tabular-nums ${rate < 1 ? 'text-[#e5484d]' : lit ? 'text-clover' : 'text-head'}`}
            >
              {Math.round((1 - rate) * 100)}%
            </p>
          </div>
          <CallStrip lit={lit} rate={rate} />
        </div>
      </div>
    </div>
  );
}

/** Both stores on one axis, so the size gap is the first thing you see, not a line you have to
 *  read: OpenTelemetry's bar runs the full width and Boundary's is what's left of it. */
function Shrink({ n, rates }: { n: number; rates: Rates }) {
  const otel = otelBytes(n, rates.otel);
  const baml = bamlBytes(n, rates.baml);
  const top = Math.max(otel, baml);
  const smaller = baml <= otel;
  const rows = [
    { bytes: otel, lit: false, name: 'OpenTelemetry' },
    { bytes: baml, lit: true, name: 'Boundary' },
  ];
  return (
    <div className="mt-6 grid items-center gap-x-10 gap-y-6 md:grid-cols-[minmax(0,1fr)_auto]">
      <div className="space-y-3">
        {rows.map((r) => (
          <div key={r.name}>
            <div className="flex items-baseline justify-between gap-3">
              <p
                className={`flex items-center gap-2 text-[15px] ${r.lit ? 'text-clover' : 'text-white/70'}`}
              >
                {r.lit && <BoundaryMark className="h-[16px] w-auto" />}
                {r.name} storage
              </p>
              <p
                className={`font-mono text-[22px] tabular-nums ${r.lit ? 'text-clover' : 'text-head'}`}
              >
                {data(r.bytes)}
              </p>
            </div>
            <div className="mt-1 h-7 overflow-hidden rounded-[4px] bg-white/[0.06]">
              <span
                className={`block h-full rounded-[4px] ${r.lit ? 'bg-clover' : 'bg-white/40'}`}
                style={{ minWidth: 4, width: `${(r.bytes / top) * 100}%` }}
              />
            </div>
          </div>
        ))}
      </div>
      <div className="md:text-right">
        <p className="font-display text-[64px] font-semibold leading-none tabular-nums text-clover md:text-[96px]">
          {times(smaller ? otel / baml : baml / otel)}
        </p>
        <p className="mt-2 text-[18px] text-head">
          {smaller ? 'smaller' : 'larger'}
        </p>
        {/* Follows the sliders, so it always says what the meters show. */}
        <p className="mt-1 max-w-[18rem] text-[14px] text-white/60 tabular-nums md:ml-auto">
          {versus(rates)}
        </p>
      </div>
    </div>
  );
}

function Billion({ p }: { p: number }) {
  const [rates, setRates] = useState<Rates>({ baml: 1, otel: 0.2 });
  const n = Math.round(CALLS ** Math.max(0.5, sweep(p)));
  // One box, like the speed chart's: the title, then both sides as columns.
  return (
    <div className="rounded-xl border border-rule bg-raised p-5">
      {/* The pair to the speed box's "We made observability fast." */}
      <p className="v4-h3 text-head">We made observability tiny.</p>
      <Shrink n={n} rates={rates} />
      <div className="mt-6 grid md:grid-cols-2 gap-3">
        <Side
          n={n}
          rates={rates}
          setRate={(otel) => setRates({ ...rates, otel })}
        />
        <Side
          lit
          n={n}
          rates={rates}
          setRate={(baml) => setRates({ ...rates, baml })}
        />
      </div>
    </div>
  );
}

// ---------- media: two systems, then one ----------

// The Multimodal figure: split storage (traces plus a blob store) against one storage engine.
const COPY = {
  bottom: 'Blob store',
  title: 'We made observability multimodal.',
  top: ['Traces', 'Unified storage engine'],
};

const WAVE = [14, 34, 22, 48, 30, 56, 26, 44, 18, 52, 28, 38, 16, 30];

/** The photo tile's picture. Its gradient id is per instance: the tile renders several times on a
 *  page (desktop and phone copies), and a shared id would point every copy at the first, which may
 *  sit in a hidden subtree. */
function Landscape() {
  const sky = useId();
  return (
    <svg
      aria-hidden="true"
      className="block h-full w-full"
      preserveAspectRatio="xMidYMax slice"
      viewBox="0 0 160 90"
    >
      <defs>
        <linearGradient id={sky} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0" stopColor="#3a2f63" />
          <stop offset="1" stopColor="#e6a97a" />
        </linearGradient>
      </defs>
      <rect fill={`url(#${sky})`} height="90" width="160" />
      <circle cx="116" cy="28" fill="#F5C542" r="11" />
      <path
        d="M0 90 L0 66 L38 30 L66 58 L84 42 L120 76 L160 54 L160 90 Z"
        fill="#6f5aa0"
      />
      <path
        d="M0 90 L0 78 L46 58 L92 82 L128 68 L160 80 L160 90 Z"
        fill="#2b2440"
      />
    </svg>
  );
}

const TILES = [
  {
    body: <Landscape />,
    name: 'image',
    span: 'describe_photo',
    url: 'a8f3.png',
  },
  {
    body: (
      <span className="flex h-full items-center justify-center gap-[3px] px-2">
        {WAVE.map((h, i) => (
          <span
            className="w-[3px] rounded-full bg-white/80"
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed drawing
            key={i}
            style={{ height: `${h}%` }}
          />
        ))}
      </span>
    ),
    name: 'audio',
    span: 'transcribe',
    url: '7c1e.wav',
  },
  {
    body: (
      <span className="grid h-full place-items-center bg-[#2b2440]">
        <span className="block border-y-[11px] border-l-[18px] border-y-transparent border-l-white/85" />
      </span>
    ),
    name: 'video',
    span: 'watch_clip',
    url: '91ad.mp4',
  },
  {
    body: (
      <span className="flex h-full flex-col justify-center gap-[5px] bg-white/8 px-3">
        <span className="font-mono text-[11px] font-bold text-white/85">
          PDF
        </span>
        <span className="h-[2px] w-full bg-white/35" />
        <span className="h-[2px] w-4/5 bg-white/35" />
        <span className="h-[2px] w-full bg-white/35" />
      </span>
    ),
    name: 'PDF',
    span: 'read_contract',
    url: 'e20c.pdf',
  },
  {
    body: (
      <span className="block p-2 font-mono text-[10px] leading-[1.35] text-white/80">
        "dear customer, your order has shipped..."
      </span>
    ),
    name: 'large string',
    span: 'draft_reply',
    url: '04bd.txt',
  },
];

const TRACE_H = 146;
const TILE_H = 82;
const TILE_TOP = 52;
const GAP_H = 76;

// Which stack is showing. A dot runs along the toggle's bottom, across and back, and whichever side
// it is over lights up, so a switch is always visible before it happens. Picking a side goes there
// and stays.
export function Toggle({
  after,
  pick,
  wait,
}: {
  after: number;
  pick: (end: 0 | 1) => void;
  wait: number | null;
}) {
  // The side the reader clicked, lit at once while the figure catches up. It lets go once the loop
  // carries on again.
  const [chosen, setChosen] = useState<0 | 1 | null>(null);
  const held = useRef(false);
  useEffect(() => {
    if (wait === null) held.current = true;
    else if (held.current) {
      held.current = false;
      setChosen(null);
    }
  }, [wait]);
  return (
    <div className="relative inline-grid grid-cols-2 rounded-full border border-rule bg-black/40 p-1">
      {(['Traditional stack', 'Boundary'] as const).map((name, i) => {
        // A click lights its side at once; otherwise the side the dot is over is lit.
        const right =
          chosen !== null
            ? chosen === 1
            : wait === null
              ? after >= 0.5
              : wait >= 0.5;
        const on = i === 0 ? !right : right;
        return (
          <button
            aria-pressed={on}
            className={`focus-ring relative flex items-center justify-center gap-2 whitespace-nowrap px-4 py-1.5 ${i === 0 ? 'rounded-l-full' : 'rounded-r-full'} font-display text-[15px] font-medium transition-colors duration-300 md:text-[17px] cursor-pointer ${on ? 'bg-clover text-black' : 'text-white/60 hover:bg-white/8 hover:text-white'}`}
            key={name}
            onClick={() => {
              setChosen(i as 0 | 1);
              pick(i as 0 | 1);
            }}
            type="button"
          >
            {i === 1 && <BoundaryMark className="h-[18px] w-auto" />}
            {name}
          </button>
        );
      })}
      {/* A small dot along the bottom inside the toggle, across and back. It's always over the
                lit side, so it's dark against the purple. */}
      <span
        aria-hidden
        className="pointer-events-none absolute bottom-[7px] left-6 right-6 z-10 h-0 transition-opacity duration-300"
        style={{ opacity: wait === null ? 0 : 1 }}
      >
        <span
          className="absolute inset-x-0 top-0 will-change-transform"
          style={{
            transform: `translate3d(${(wait ?? 0) * 100}%, 0, 0)`,
          }}
        >
          <span className="absolute left-0 top-0 size-[5px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-black/75" />
        </span>
      </span>
    </div>
  );
}

// The Unified engine version. Two traces send the same photo, so the traditional blob store holds two
// copies; on the Boundary side the second copy folds into the first. Only opacity and transform move.
const SAME = 1;
// Each trace's bar, as a share of its column.
const bar = (i: number) => (88 - i * 11) / 100;
// The photo's two traces point from the middles of their bars. In the overlay over both columns
// (width w = 2c + 12, c a column), a point at a·c + b sits at a/2 of w, less 6a, plus b pixels.
const at = (a: number, b: number) => `calc(${a * 50}% + ${b - 6 * a}px)`;
const LEG = [bar(0) / 2, 1 + bar(SAME) / 2];
const MID = (LEG[0] + LEG[1]) / 2;
const FORK = {
  // The fork's 2px sides are centered on the legs.
  left: at(LEG[0], -1),
  mid: at(MID, 6),
  // From the right edge, the leg's distance is w less the point.
  right: `calc(${100 - LEG[1] * 50}% + ${6 * LEG[1] - 12 - 1}px)`,
};
// How far each copy of the photo moves to meet under the fork, as a share of a column and pixels.
const MEET = [`(${(MID - 0.5) * 100}% + 6px)`, `(${(MID - 1.5) * 100}% - 6px)`];
// How far the files sit under their traces in the engine, to leave room for the pointers.
const DROP = 40;
const PHOTO = TILES[0];
const U_TILES = [
  PHOTO,
  { ...PHOTO, span: 'caption_photo', url: 'c19b.png' },
  TILES[2],
  TILES[3],
  TILES[4],
];
// A column apart, so the tags fit side by side on a phone.
const U_BREAKS: Record<number, string> = { 2: 'link expired', 4: 'file moved' };
// The BAML lamb, in the size and color of the other features' icons.
function Lamb({ className }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={className}
      fill="currentColor"
      viewBox="0 0 356 422"
    >
      {LAMB_PATHS.map((d) => (
        <path d={d} key={d} />
      ))}
    </svg>
  );
}

// What the storage engine does, shown where the blob store was.
const FEATURES: {
  body: string;
  icon: (props: { className?: string }) => ReactNode;
  soon?: boolean;
  title: string;
}[] = [
  {
    body: 'Images, audio, video, PDFs, large strings and bytes.',
    icon: LayersIcon,
    title: 'Every kind of data',
  },
  {
    body: 'Runtime and cloud collaborate to minimize duplicated data.',
    icon: Fingerprint,
    title: 'Recursive deduping',
  },
  {
    body: 'Withhold a value, and the trace records why.',
    icon: EyeOff,
    soon: true,
    title: 'Redaction',
  },
  {
    body: 'Classes and enums stored with their definitions.',
    icon: Lamb,
    title: 'Keeps your types',
  },
];
const BLOB_TOP = 30 + TRACE_H + GAP_H;
// Where the engine's features start: under the files.
const SPEC_TOP = 30 + TILE_TOP + DROP + TILE_H + 12;
// The blob store's name sits in the box, over its files.
const BLOB_HEAD = 26;
const BLOB_H = BLOB_HEAD + TILE_H + 24;
const ENGINE_H = TRACE_H + GAP_H + BLOB_H;
// The strip under the features, the full width of the engine.
const STRIP_H = 40;
// Phones list the features one per line, so the engine grows by `--more` there.
const MORE = 'var(--more)';
// Animated pieces get their own layer, and only opacity and transform change.
const GPU = 'will-change-[opacity,transform]';

function Media({
  p,
  pick,
  still,
  wait,
}: {
  p: number;
  pick: (end: 0 | 1) => void;
  // On phones each side is drawn once, one under the other, with its name in place of the toggle.
  still?: 'first' | 'second';
  wait: number | null;
}) {
  const move = ramp(sweep(p), 0.2, 0.8);
  const old = ramp(move, 0, 0.3);
  const engine = ramp(move, 0.3, 0.65);
  // The two copies of the photo meet in the middle, under both traces, and the second dissolves into
  // the first. Then both traces draw a line to it: two pointers, one file.
  const fold = ramp(move, 0.5, 0.74);
  const gone = ramp(move, 0.66, 0.78);
  const beat = Math.sin(Math.PI * ramp(move, 0.72, 0.88)) * 0.035;
  const join = ramp(move, 0.76, 0.88);
  const stem = ramp(move, 0.86, 0.97);
  const note = ramp(move, 0.75, 1);
  const after = move >= 0.5 ? 1 : 0;
  const lift = BLOB_TOP + BLOB_HEAD + 12 - (30 + TILE_TOP);
  return (
    <div
      className={still ? (still === 'second' ? 'mt-6' : '') : 'mt-4 md:mt-6'}
    >
      <div className="rounded-xl border border-rule bg-raised p-5">
        {still !== 'second' && <p className="v4-h3 text-head">{COPY.title}</p>}
        <div
          className={`flex flex-wrap items-center gap-x-4 gap-y-2 ${still === 'second' ? '' : 'mt-3'}`}
        >
          {still ? (
            <span
              className={`flex items-center gap-2 rounded-full px-4 py-1.5 font-display text-[15px] font-medium ${after ? 'bg-clover text-black' : 'border border-rule text-white/85'}`}
            >
              {after ? <BoundaryMark className="h-[18px] w-auto" /> : null}
              {after ? 'Boundary' : 'Traditional stack'}
            </span>
          ) : (
            <Toggle after={after} pick={pick} wait={wait} />
          )}
          <span
            className={`hidden items-center gap-2 text-[15px] text-white/50 sm:flex ${GPU}`}
            style={{ opacity: 1 - old }}
          >
            <SiOpentelemetry aria-hidden="true" className="shrink-0" />
            OpenTelemetry + your blob store
          </span>
        </div>
        <div
          className="relative mt-5 [--more:20px] md:[--more:0px]"
          style={{ height: `calc(${30 + ENGINE_H}px + ${MORE})` }}
        >
          <p
            className={`absolute left-0 top-0 text-[15px] text-white/70 ${GPU}`}
            style={{ opacity: 1 - engine }}
          >
            {COPY.top[0]}
          </p>
          <p
            className={`absolute left-0 top-0 text-[15px] text-clover ${GPU}`}
            style={{ opacity: engine }}
          >
            {COPY.top[1]}
          </p>
          {/* Two systems: traces, and a blob store holding the files. */}
          <div
            className={`absolute inset-x-0 rounded-md border border-white/20 bg-white/5 ${GPU}`}
            style={{ height: TRACE_H, opacity: 1 - old, top: 30 }}
          />
          <div
            className={`absolute inset-x-0 rounded-md border border-white/20 bg-white/5 ${GPU}`}
            style={{ height: BLOB_H, opacity: 1 - old, top: BLOB_TOP }}
          />
          <p
            className={`absolute left-3 text-[15px] leading-none text-white/70 ${GPU}`}
            style={{ opacity: 1 - old, top: BLOB_TOP + 11 }}
          >
            {COPY.bottom}
          </p>
          {/* One storage engine over the whole figure. */}
          <div
            className={`absolute inset-x-0 rounded-md border border-clover/60 bg-[linear-gradient(180deg,rgb(from_var(--color-clover)_r_g_b/0.06),rgb(from_var(--color-clover)_r_g_b/0.02))] shadow-[0_0_24px_-14px_var(--color-clover)] ${GPU}`}
            style={{
              height: `calc(${ENGINE_H}px + ${MORE})`,
              opacity: engine,
              top: 30,
            }}
          />
          {/* Under the files, what the engine does with them. */}
          <div
            className={`absolute inset-x-0 border-t border-clover/25 ${GPU}`}
            style={{ opacity: note, top: SPEC_TOP }}
          />
          <ul
            className="absolute inset-x-0 grid content-start gap-y-2.5 pt-4 md:grid-cols-4 md:gap-y-0 md:pt-0"
            style={{
              height: `calc(${30 + ENGINE_H - STRIP_H - SPEC_TOP}px + ${MORE})`,
              top: SPEC_TOP,
            }}
          >
            {FEATURES.map((f, i) => {
              const v = ramp(note, i * 0.08, 0.6 + i * 0.08);
              const Icon = f.icon;
              return (
                <li
                  className={`px-4 md:pt-3 ${i > 0 ? 'md:border-l md:border-clover/15' : ''} ${GPU}`}
                  key={f.title}
                  style={{
                    opacity: v,
                    transform: `translate3d(0, ${(1 - v) * 8}px, 0)`,
                  }}
                >
                  <p className="flex items-center gap-2 text-[15px] font-medium text-white">
                    <Icon className="size-4 shrink-0 text-clover" />
                    {f.title}
                    {f.soon && (
                      <span className="rounded-full border border-white/20 px-1.5 py-px text-[11px] font-normal text-white/60">
                        Coming soon
                      </span>
                    )}
                  </p>
                  <p className="mt-1.5 hidden text-[13px] leading-snug text-white/55 md:block">
                    {f.body}
                  </p>
                </li>
              );
            })}
          </ul>
          {/* Under all of them: where the engine runs. */}
          <p
            className={`absolute inset-x-0 flex items-center gap-2 border-t border-clover/15 px-4 text-[13px] text-white/80 md:text-[14px] ${GPU}`}
            style={{
              height: STRIP_H,
              opacity: ramp(note, 0.4, 1),
              top: `calc(${30 + ENGINE_H - STRIP_H}px + ${MORE})`,
            }}
          >
            <Cloud aria-hidden="true" className="size-4 shrink-0 text-clover" />
            Runs on your laptop. Runs in the cloud.
          </p>
          <div className="absolute inset-x-0 top-[30px] grid grid-cols-5 gap-3 px-3">
            {U_TILES.map((t, i) => {
              const up = ramp(move, 0.12 + i * 0.03, 0.55 + i * 0.03);
              const same = i === SAME;
              // The photo both traces use.
              const shared = i <= SAME;
              return (
                <div
                  className="relative min-w-0"
                  key={t.span}
                  style={{ height: TRACE_H }}
                >
                  <div className="absolute inset-x-0 top-3 font-mono text-[12px] leading-4 text-white/85">
                    <p className="truncate">{t.span}</p>
                    <span
                      className="mt-1 block h-1.5 rounded-[2px] bg-clover/70"
                      style={{ width: `${bar(i) * 100}%` }}
                    />
                    <div className="mt-3 grid text-[11px]">
                      <p
                        className={`[grid-area:1/1] truncate text-[#F5C542] ${GPU}`}
                        style={{ opacity: 1 - old }}
                      >
                        s3://{t.url}
                      </p>
                    </div>
                  </div>
                  {/* The pointer down into the blob store, and where it breaks. */}
                  <div
                    className={`absolute inset-x-0 top-0 flex flex-col items-center ${GPU}`}
                    style={{ height: TRACE_H + GAP_H, opacity: 1 - old }}
                  >
                    <svg
                      aria-hidden="true"
                      className="absolute left-1/2 -translate-x-1/2"
                      height={GAP_H - 16}
                      style={{ top: TRACE_H + 3 }}
                      width="4"
                    >
                      <line
                        stroke="#F5C542"
                        strokeDasharray="0 8"
                        strokeLinecap="round"
                        strokeWidth="3"
                        x1="2"
                        x2="2"
                        y1="2"
                        y2={GAP_H - 18}
                      />
                    </svg>
                    {i in U_BREAKS && (
                      <span className="relative z-10 mt-auto mb-[24px] whitespace-nowrap rounded-full bg-[#e5484d] px-2 py-0.5 text-[11px] font-semibold md:px-2.5 md:py-1 md:text-[12px] text-white shadow-[0_0_0_4px_var(--color-raised)]">
                        {U_BREAKS[i]}
                      </span>
                    )}
                  </div>
                  {/* Its trace's pointer to the file. The photo's two pointers are the fork. */}
                  {!shared && (
                    <>
                      <span
                        className={`absolute w-[2px] -translate-x-1/2 origin-top bg-clover ${GPU}`}
                        style={{
                          height: TILE_TOP + DROP - 38 - 6,
                          left: `${(bar(i) * 100) / 2}%`,
                          opacity: stem > 0 ? 1 : 0,
                          top: 38,
                          transform: `scaleY(${stem})`,
                        }}
                      />
                      <span
                        className={`absolute size-0 -translate-x-1/2 border-x-[5px] border-t-[7px] border-x-transparent border-t-clover ${GPU}`}
                        style={{
                          left: `${(bar(i) * 100) / 2}%`,
                          opacity: stem >= 0.98 ? 1 : 0,
                          top: TILE_TOP + DROP - 7,
                        }}
                      />
                    </>
                  )}
                  <div
                    className={`absolute inset-x-0 overflow-hidden rounded-[5px] border border-white/25 bg-black/40 ${GPU}`}
                    style={{
                      height: TILE_H,
                      opacity: same ? 1 - gone : 1,
                      top: TILE_TOP,
                      transform: shared
                        ? `translate3d(calc(${fold} * ${same ? MEET[1] : MEET[0]}), ${(1 - up) * lift + up * DROP}px, 0) scale(${same ? 1 : 1 + beat})`
                        : `translate3d(0, ${(1 - up) * lift + up * DROP}px, 0)`,
                    }}
                  >
                    {same ? (
                      <>
                        <span className="block h-full opacity-35">
                          {t.body}
                        </span>
                        <span
                          className={`absolute inset-0 grid place-items-center ${GPU}`}
                          style={{ opacity: 1 - old }}
                        >
                          <span className="w-min rounded-md bg-black/75 px-1.5 py-1 text-center text-[10px] font-semibold leading-tight text-white md:w-auto md:whitespace-nowrap md:rounded-full md:px-2.5 md:text-[12px]">
                            2nd copy
                          </span>
                        </span>
                      </>
                    ) : (
                      t.body
                    )}
                  </div>
                </div>
              );
            })}
          </div>
          {/* Both traces point at the one photo between them. */}
          <div
            aria-hidden="true"
            className="pointer-events-none absolute left-3"
            style={{
              height: TILE_TOP + DROP,
              top: 30,
              width: 'calc((100% - 72px) * 0.4 + 12px)',
            }}
          >
            {/* A fork: down from each trace, joined, and one line into the photo. */}
            <span
              className={`absolute top-[38px] h-[16px] origin-top rounded-b-[6px] border-x-2 border-b-2 border-clover ${GPU}`}
              style={{
                left: FORK.left,
                opacity: join > 0 ? 1 : 0,
                right: FORK.right,
                transform: `scaleY(${join})`,
              }}
            />
            <span
              className={`absolute top-[52px] w-[2px] -translate-x-1/2 origin-top bg-clover ${GPU}`}
              style={{
                height: TILE_TOP + DROP - 52 - 6,
                left: FORK.mid,
                opacity: stem > 0 ? 1 : 0,
                transform: `scaleY(${stem})`,
              }}
            />
            <span
              className={`absolute size-0 -translate-x-1/2 border-x-[5px] border-t-[7px] border-x-transparent border-t-clover ${GPU}`}
              style={{
                left: FORK.mid,
                opacity: stem >= 0.98 ? 1 : 0,
                top: TILE_TOP + DROP - 7,
              }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}

const COSTS = [
  { Icon: SiPython, name: 'Python', ns: 9_288 },
  { Icon: SiTypescript, name: 'TypeScript', ns: 702 },
  { Icon: SiRust, name: 'Rust', ns: 186 },
];
const BAML_NS = 32;
const FULL_BAR_NS = 1_000;
const DATE_NOW_NS = 91;
const MACHINE = 'Apple M2 Max, 64 GB';

const slower = (ns: number) => {
  const times = ns / BAML_NS;
  return times >= 10 ? Math.round(times) : Number(times.toFixed(1));
};

// Four columns from md: the name, its bar, the time, and how much slower. A phone has no room for
// all four on one line (the name ran under the time), so there a row is two lines: the name and the
// time, then the bar and how much slower. The time is the narrower of the two figures, so it is the
// one that shares a line with the name, which fits down to a 320px screen. The `max-md:order-*`
// classes below put the four in that order.
const ROW =
  'grid grid-cols-[minmax(0,1fr)_auto] md:grid-cols-[minmax(0,210px)_minmax(0,1fr)_84px_130px] items-center gap-x-3 md:gap-x-4 gap-y-2 py-3 md:py-0 md:min-h-[52px]';

function CostRow({
  lit,
  name,
  ns,
  on,
}: {
  lit?: boolean;
  name: ReactNode;
  ns: number;
  on: number;
}) {
  return (
    // The row is in from the start; only its bar fills in, with `on`.
    <div
      className={`${ROW} ${lit ? 'rounded-lg bg-clover/10 -mx-3 px-3' : ''}`}
    >
      <div className="max-md:order-1 flex min-w-0 items-center gap-1.5 md:gap-2 whitespace-nowrap text-[14px] md:text-[16px] text-white">
        {name}
      </div>
      <span className="max-md:order-3 relative block h-2.5 rounded-full bg-white/8 overflow-hidden">
        <span
          className={`block h-full rounded-full ${lit ? 'bg-clover' : 'bg-white/45'}`}
          style={{
            minWidth: 4,
            width: `${Math.min(1, ns / FULL_BAR_NS) * 100 * on}%`,
          }}
        />
        {/* A break in the bar: this one runs far past the end of the scale. */}
        {ns > FULL_BAR_NS &&
          [88, 92].map((left) => (
            <span
              className="absolute -top-1 -bottom-1 w-[3px] rotate-[20deg] bg-raised"
              key={left}
              style={{ left: `${left}%` }}
            />
          ))}
      </span>
      <span
        className={`max-md:order-2 text-right font-mono text-[14px] md:text-[15px] tabular-nums ${lit ? 'text-clover' : 'text-white/60'}`}
      >
        {ns.toLocaleString('en-US')} ns
      </span>
      <span className="max-md:order-4 text-right whitespace-nowrap font-display text-[17px] md:text-[20px] font-medium leading-none tabular-nums text-head">
        {lit ? (
          <span className="text-[16px] font-normal text-clover">baseline</span>
        ) : (
          `${slower(ns)}× slower`
        )}
      </span>
    </div>
  );
}

function Speed({ p }: { p: number }) {
  const s = sweep(p);
  return (
    <div className="mt-4 md:mt-6 rounded-xl border border-rule bg-raised p-5">
      <p className="v4-h3 text-head">We made observability fast.</p>
      <p className="mt-2 text-[18px] text-white/70">
        The overhead of observing a function.
      </p>
      <div className="mt-6">
        <CostRow
          lit
          name={
            <>
              <BoundaryLogo className="h-[40px] w-auto text-white" />
              <span className="sr-only">Boundary</span>
            </>
          }
          ns={BAML_NS}
          on={ramp(s, 0, 0.2)}
        />
        <CostRow
          name={
            <>
              <SiTypescript aria-hidden="true" className="shrink-0" />
              One Date.now()
            </>
          }
          ns={DATE_NOW_NS}
          on={ramp(s, 0.2, 0.4)}
        />
        {[...COSTS].reverse().map(({ Icon, name, ns }, i) => (
          <CostRow
            key={name}
            name={
              <>
                <SiOpentelemetry aria-hidden="true" className="shrink-0" />
                OTel
                <span className="text-white/50">+</span>
                <Icon aria-hidden="true" className="shrink-0" />
                {name}
              </>
            }
            ns={ns}
            on={ramp(s, 0.4 + i * 0.2, 0.6 + i * 0.2)}
          />
        ))}
      </div>
      <p className="mt-5 text-[20px] text-white">
        And we're making it even faster.
      </p>
      <p className="mt-3 text-[13px] text-white/45">Measured on {MACHINE}.</p>
    </div>
  );
}

// ---------- the section ----------

/** Why BAML is a language: the stack it takes, then what seeing every function costs. */
export default function Stack() {
  return (
    <section
      {...section('stack')}
      aria-labelledby="stack"
      className="border-t border-rule overflow-x-clip"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        {/* The worry, asked out loud, then answered in the order it's asked: fast, then small
            (storage, then the multimodal store), then the answer. Like the review section's
            "Can agents write high-quality BAML?" and its "So yes… By design." */}
        {/* Given room on both sides, so it reads as a new beat, not a caption on the figure. */}
        <h2 className="v4-h2 text-head max-w-[22em]" id="stack">
          Is <Hyper /> practical?
        </h2>
        {/* The worry, stated before anything answers it. */}
        <p className="mt-4 max-w-[40rem] v4-body text-ink-2">
          Recording every function means overhead on every call and far more
          data to store.
        </p>
        <div className="mt-14 md:mt-20">
          <Played ms={2600}>{(p) => <Speed p={p} />}</Played>
        </div>
        <div className="mt-20 md:mt-28">
          {/* Shown already filled in, so the reader can go straight to the sliders. */}
          <Billion p={1} />
        </div>
        <div className="mt-20 md:mt-28">
          <Looped>
            {(p, pick, wait) => (
              <div className="hidden md:block">
                <Media p={p} pick={pick} wait={wait} />
              </div>
            )}
          </Looped>
          {/* Phones show both sides, one under the other, without the animation. */}
          <div className="md:hidden">
            <Media p={0} pick={() => {}} still="first" wait={null} />
            <Media p={1} pick={() => {}} still="second" wait={null} />
          </div>
        </div>
        <p className="mt-16 md:mt-20 v4-h2 text-head max-w-[22em]">
          So yes, <Hyper /> is practical. By design.
        </p>
        <h2 className="mt-32 md:mt-48 v4-h2 text-head max-w-[22em]">
          But to make <Hyper /> possible, we had to build a programming
          language.
        </h2>
        <div className="mt-12 md:mt-16">
          <Pinned length="160vh">{(p) => <Layers p={p} />}</Pinned>
        </div>
      </div>
    </section>
  );
}
