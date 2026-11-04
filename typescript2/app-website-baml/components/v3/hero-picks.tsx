'use client';

import { useEffect, useRef, useState } from 'react';
import { useCalls, useNow } from './hero-minimal';
import { RED } from './story';

// Five quiet takes built from the versions that were kept: 5 and 16 (each language's reach), 6 (failures
// come back), 15 (the tracker), and 31 to 37 (the lifeline). Each is one shape that says the whole thing
// at a glance; any motion is ambient. The numbers are example data.

const CAPTION =
  'mt-8 md:mt-12 text-center text-[15px] md:text-xl text-white/55';

/** A point on a circle, `deg` clockwise from the top. */
const polar = (deg: number, r: number) => {
  const a = (deg * Math.PI) / 180;
  return { x: r * Math.sin(a), y: -r * Math.cos(a) };
};

/** An arc clockwise from `from` to `to` degrees, as an SVG path. */
const arcPath = (from: number, to: number, r: number) => {
  const a = polar(from, r);
  const b = polar(to, r);
  return `M${a.x} ${a.y} A${r} ${r} 0 ${to - from > 180 ? 1 : 0} 1 ${b.x} ${b.y}`;
};

// One ring per toolchain, drawn only as far as that toolchain reaches: build, test, deploy, monitor,
// clockwise from the top. Go's monitor quarter is dashed, because pprof samples production. Only BAML's
// ring closes, and a dot keeps going around it.
const QUARTERS = ['Build', 'Test', 'Deploy', 'Monitor'];
const REACH: { name: string; quarters: (boolean | 'partial' | 'key')[] }[] = [
  { name: 'C', quarters: [true, false, false, false] },
  { name: 'Go', quarters: [true, true, false, 'partial'] },
  { name: 'Rust', quarters: [true, true, true, false] },
  { name: 'BAML', quarters: [true, true, true, true] },
];
const RING = { gap: 6, r: 44, w: 6 };

function Ring({
  baml,
  children,
  dot,
  quarters,
}: {
  baml?: boolean;
  children?: React.ReactNode;
  dot?: number;
  quarters: (boolean | 'partial' | 'key')[];
}) {
  const p = dot === undefined ? null : polar(dot, RING.r);
  return (
    <div className="relative aspect-square w-full max-w-[190px] mx-auto">
      <svg aria-hidden="true" className="size-full" viewBox="-60 -60 120 120">
        <circle
          fill="none"
          r={RING.r}
          stroke="rgb(255 255 255 / 0.07)"
          strokeWidth={RING.w}
        />
        {quarters.map((q, i) =>
          q ? (
            <path
              d={arcPath(
                i * 90 + RING.gap / 2,
                (i + 1) * 90 - RING.gap / 2,
                RING.r,
              )}
              fill="none"
              key={QUARTERS[i]}
              stroke={baml ? 'var(--color-clover-core)' : '#fff'}
              strokeDasharray={q === 'partial' ? '1 6' : undefined}
              strokeLinecap="round"
              strokeOpacity={
                q === 'key' ? 0.2 : q === 'partial' ? 0.5 : baml ? 1 : 0.85
              }
              strokeWidth={RING.w}
            />
          ) : null,
        )}
        {p && (
          <circle cx={p.x} cy={p.y} fill="var(--color-clover-100)" r="3.2" />
        )}
      </svg>
      <div className="absolute inset-0 grid place-items-center">{children}</div>
    </div>
  );
}

export function HeroLanguageRings() {
  const t = useNow();

  return (
    <figure
      aria-label="One ring per toolchain, drawn only as far as it reaches through build, test, deploy, and monitor. C reaches build. Go reaches build and test, and samples production. Rust reaches build, test, and deploy. Only BAML's ring closes."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="grid grid-cols-2 md:grid-cols-5 gap-x-4 gap-y-8 md:gap-x-6 items-center">
        {/* The key: which quarter is which stage. */}
        <div className="col-span-2 md:col-span-1 w-[170px] md:w-full mx-auto">
          <Ring quarters={['key', 'key', 'key', 'key']}>
            {QUARTERS.map((q, i) => (
              <span
                className="absolute text-[11px] md:text-[12px] text-white/50"
                key={q}
                style={
                  [
                    { right: '0%', top: '4%' },
                    { bottom: '4%', right: '0%' },
                    { bottom: '4%', left: '0%' },
                    { left: '0%', top: '4%' },
                  ][i]
                }
              >
                {q}
              </span>
            ))}
          </Ring>
        </div>
        {REACH.map((lang) => {
          const baml = lang.name === 'BAML';
          return (
            <Ring
              baml={baml}
              dot={baml ? (t / 30) % 360 : undefined}
              key={lang.name}
              quarters={lang.quarters}
            >
              <span
                className={
                  baml
                    ? 'v2-baml-mark text-[18px] md:text-[20px]'
                    : 'font-display font-medium text-[17px] md:text-[19px] text-white/80'
                }
              >
                {lang.name}
              </span>
            </Ring>
          );
        })}
      </div>
      <p className={CAPTION}>
        Each ring is as far as a language’s own toolchain reaches. Only BAML’s
        closes.
      </p>
    </figure>
  );
}

// The tracker from 15 as two rows. Build and test, a deploy line, then trace, query, and fix. The row for
// other languages is empty after deploy. The BAML row is full, and a line runs from fix back to build.
const PRE = ['Build', 'Test'];
const POST = ['Trace', 'Query', 'Fix'];

export function HeroTrackerRows() {
  const seg = 'h-9 md:h-12 rounded-md';
  const cols =
    'grid grid-cols-[4.5rem_1fr_1fr_18px_1fr_1fr_1fr] md:grid-cols-[13rem_1fr_1fr_32px_1fr_1fr_1fr] gap-x-1.5 md:gap-x-2 items-center';

  return (
    <figure
      aria-label="A tracker with build and test, a deploy line, then trace, query, and fix. For Python, TypeScript, and Go, everything after deploy is empty. For BAML, every stage is filled, and a line runs from fix back to build."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative pt-6">
        <div className={`${cols} mb-3`}>
          <span />
          {PRE.map((s) => (
            <span className="text-[12px] md:text-[14px] text-white/50" key={s}>
              {s}
            </span>
          ))}
          <span className="relative self-stretch justify-self-center w-px bg-white/40 -mb-3">
            <span className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1.5 font-mono text-[10px] md:text-[12px] text-white/60">
              deploy
            </span>
          </span>
          {POST.map((s) => (
            <span className="text-[12px] md:text-[14px] text-white/50" key={s}>
              {s}
            </span>
          ))}
        </div>
        <div className={`${cols} gap-y-3`}>
          <span className="text-[12px] md:text-[15px] text-white/50 pr-2">
            <span className="hidden md:inline">Python, TypeScript, Go</span>
            <span className="md:hidden">Others</span>
          </span>
          {PRE.map((s) => (
            <span className={`${seg} bg-white/70`} key={s} />
          ))}
          <span className="self-stretch justify-self-center w-px bg-white/40 -my-1.5" />
          {POST.map((s) => (
            <span
              className={`${seg} border border-dashed border-white/20`}
              key={s}
            />
          ))}

          <span className="v2-baml-mark text-[15px] md:text-[20px]">BAML</span>
          {PRE.map((s) => (
            <span className={`${seg} bg-white`} key={s} />
          ))}
          <span className="self-stretch justify-self-center w-px bg-white/40 -my-1.5" />
          {POST.map((s) => (
            <span className={`${seg} bg-clover`} key={s} />
          ))}

          {/* From fix back to build, under the BAML row. */}
          <span />
          <span className="relative col-span-6 h-7 md:h-9 -mt-1">
            <span className="absolute left-[8%] right-[8%] top-0 h-full rounded-b-xl border-x border-b border-clover/70" />
            <span className="absolute left-[8%] -top-px size-2 -translate-x-1/2 rotate-45 border-l border-t border-clover" />
            <span className="absolute left-1/2 top-full -translate-x-1/2 mt-1.5 text-[11px] md:text-[13px] text-clover whitespace-nowrap">
              into the next build
            </span>
          </span>
        </div>
      </div>
      <p className={`${CAPTION} mt-12 md:mt-16`}>
        Other toolchains stop at deploy. BAML’s keeps going, and comes back
        around.
      </p>
    </figure>
  );
}

/** Dots moving along an SVG path drawn in a 1000 by 100 box, placed as HTML so they stay round. */
function FlowDots({
  color,
  count,
  d,
  lapMs,
  t,
}: {
  color: string;
  count: number;
  d: string;
  lapMs: number;
  t: number;
}) {
  const path = useRef<SVGPathElement>(null);
  const [len, setLen] = useState(0);
  useEffect(() => {
    setLen(path.current?.getTotalLength() ?? 0);
  }, []);
  return (
    <>
      <svg
        aria-hidden="true"
        className="absolute inset-0 size-full invisible"
        preserveAspectRatio="none"
        viewBox="0 0 1000 100"
      >
        <path d={d} ref={path} />
      </svg>
      {len > 0 &&
        Array.from({ length: count }, (_, i) => {
          const u = (((t / lapMs + i / count) % 1) + 1) % 1;
          const p = path.current?.getPointAtLength(u * len);
          if (!p) return null;
          return (
            <span
              aria-hidden
              className="absolute size-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full"
              // biome-ignore lint/suspicious/noArrayIndexKey: dots keep their order
              key={i}
              style={{
                background: color,
                left: `${p.x / 10}%`,
                opacity: Math.min(1, u * 10, (1 - u) * 10),
                top: `${p.y}%`,
              }}
            />
          );
        })}
    </>
  );
}

// Two pipes. Both run through build and test to deploy. After deploy, the pipe for other languages splits
// into five separate tools and ends there. BAML's carries on into production and loops back to build.
const TOOLS = ['Tracing', 'Errors', 'Metrics', 'Analytics', 'Alerts'];
const SPLIT_YS = [12, 31, 50, 69, 88];

export function HeroPipes() {
  const t = useNow();
  const stroke = {
    fill: 'none',
    strokeWidth: 1.5,
    vectorEffect: 'non-scaling-stroke',
  } as const;
  const branch = (y: number) => `M0 50 H420 C500 50 500 ${y} 580 ${y} H760`;
  const loop = 'M0 35 H880 C960 35 960 80 880 80 H140 C80 80 80 35 140 35';
  const node = (x: string, label: string, y = '50%') => (
    <span key={label}>
      <span
        className="absolute size-2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white"
        style={{ left: x, top: y }}
      />
      <span
        className="absolute -translate-x-1/2 text-[11px] md:text-[13px] text-white/50"
        style={{ left: x, top: `calc(${y} + 10px)` }}
      >
        {label}
      </span>
    </span>
  );

  return (
    <figure
      aria-label="Two pipes. Both run through build and test to deploy. After deploy, the pipe for Python, TypeScript, and Go splits into five separate tools: tracing, errors, metrics, analytics, and alerts. BAML's pipe carries on into production and loops back to build."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative">
        <span
          aria-hidden
          className="absolute top-0 bottom-0 left-[42%] border-l border-dashed border-white/25"
        />
        <p className="absolute left-[42%] -top-6 -translate-x-1/2 font-mono text-[11px] md:text-[12px] text-white/60">
          deploy
        </p>

        <div className="relative h-44 md:h-52">
          <p className="absolute left-0 top-0 text-[13px] md:text-[15px] text-white/50">
            Python, TypeScript, Go
          </p>
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full overflow-visible"
            preserveAspectRatio="none"
            viewBox="0 0 1000 100"
          >
            {SPLIT_YS.map((y) => (
              <path
                {...stroke}
                d={branch(y)}
                key={y}
                stroke="rgb(255 255 255 / 0.45)"
              />
            ))}
          </svg>
          {SPLIT_YS.map((y, i) => (
            <span key={y}>
              <span
                className="absolute size-2 -translate-y-1/2 rounded-[2px] border border-white/50"
                style={{ left: '76%', top: `${y}%` }}
              />
              <span
                className="absolute -translate-y-1/2 text-[11px] md:text-[13px] text-white/45"
                style={{ left: 'calc(76% + 16px)', top: `${y}%` }}
              >
                {TOOLS[i]}
              </span>
            </span>
          ))}
          {SPLIT_YS.map((y) => (
            <FlowDots
              color="#fff"
              count={3}
              d={branch(y)}
              key={y}
              lapMs={5200}
              t={t + y * 37}
            />
          ))}
          {node('10%', 'Build')}
          {node('26%', 'Test')}
        </div>

        <div className="relative h-36 md:h-40 mt-6">
          <p className="absolute left-0 -top-2 v2-baml-mark text-[16px] md:text-[20px]">
            BAML
          </p>
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full overflow-visible"
            preserveAspectRatio="none"
            viewBox="0 0 1000 100"
          >
            <path {...stroke} d="M0 35 H420" stroke="#fff" />
            <path {...stroke} d={loop} stroke="var(--color-clover-core)" />
          </svg>
          <FlowDots
            color="var(--color-clover-100)"
            count={14}
            d={loop}
            lapMs={9000}
            t={t}
          />
          {node('20%', 'Build', '35%')}
          {node('31%', 'Test', '35%')}
          <span
            className="absolute -translate-x-1/2 text-[11px] md:text-[13px] text-clover whitespace-nowrap"
            style={{ left: '66%', top: 'calc(35% - 26px)' }}
          >
            production
          </span>
          <span
            className="absolute -translate-x-1/2 text-[11px] md:text-[13px] text-clover whitespace-nowrap"
            style={{ left: '50%', top: 'calc(80% + 8px)' }}
          >
            back into the next build
          </span>
        </div>
      </div>
      <p className={CAPTION}>
        After deploy, other languages split into five tools. BAML stays one
        loop.
      </p>
    </figure>
  );
}

// Calls fan out of the function into production. The failed ones curve back into it underneath and
// become tests. Dots go out along every line; on the failed lines, a red dot comes back.
const FAN = Array.from({ length: 11 }, (_, i) => 18 + i * 6.4);
const FAILED = new Set([2, 6, 9]);
const NODE_X = 140;
const PROD_X = 880;

export function HeroOutAndBack() {
  const t = useNow();
  const stroke = {
    fill: 'none',
    strokeWidth: 1.25,
    vectorEffect: 'non-scaling-stroke',
  } as const;
  const out = (y: number) =>
    `M${NODE_X + 70} 50 C${NODE_X + 300} 50 ${PROD_X - 300} ${y} ${PROD_X} ${y}`;
  // Each failed call drops down a rail beside production, then one line carries them all back.
  const back = (y: number) =>
    `M${PROD_X} ${y} H${PROD_X + 30} V96 H${NODE_X + 40} C${NODE_X} 96 ${NODE_X} 75 ${NODE_X} 58`;

  return (
    <figure
      aria-label="Calls fan out of a classify function into production. Three of them fail, and curve back underneath into the function as three new tests."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative h-[300px] md:h-[360px]">
        <svg
          aria-hidden="true"
          className="absolute inset-0 size-full overflow-visible"
          preserveAspectRatio="none"
          viewBox="0 0 1000 100"
        >
          {FAN.map((y, i) => (
            <path
              {...stroke}
              d={out(y)}
              key={y}
              stroke={FAILED.has(i) ? RED : 'rgb(255 255 255 / 0.35)'}
              strokeOpacity={FAILED.has(i) ? 0.8 : 1}
            />
          ))}
          {FAN.filter((_, i) => FAILED.has(i)).map((y) => (
            <path
              {...stroke}
              d={back(y)}
              key={y}
              stroke="var(--color-clover-core)"
              strokeOpacity="0.7"
            />
          ))}
          <line
            stroke="rgb(255 255 255 / 0.25)"
            strokeDasharray="3 4"
            vectorEffect="non-scaling-stroke"
            x1={PROD_X}
            x2={PROD_X}
            y1="6"
            y2="90"
          />
        </svg>
        {FAN.map((y, i) => (
          <FlowDots
            color={FAILED.has(i) ? RED : '#fff'}
            count={2}
            d={out(y)}
            key={y}
            lapMs={3600}
            t={t + i * 290}
          />
        ))}
        {FAN.filter((_, i) => FAILED.has(i)).map((y, i) => (
          <FlowDots
            color="var(--color-clover-100)"
            count={1}
            d={back(y)}
            key={y}
            lapMs={4200}
            t={t + i * 1400}
          />
        ))}
        <span
          className="absolute -translate-x-1/2 -translate-y-1/2 rounded-full border border-white/30 bg-black px-3 py-1.5 font-mono text-[13px] md:text-[16px] text-white"
          style={{ left: `${(NODE_X + 30) / 10}%`, top: '50%' }}
        >
          classify
        </span>
        <span
          className="absolute -translate-x-1/2 font-mono text-[11px] md:text-[12px] text-white/50"
          style={{ left: `${PROD_X / 10}%`, top: 0 }}
        >
          production
        </span>
        <span
          className="absolute -translate-x-1/2 text-[11px] md:text-[13px] text-clover whitespace-nowrap"
          style={{ left: '50%', top: 'calc(96% + 8px)' }}
        >
          3 failed calls, back as tests
        </span>
      </div>
      <p className={`${CAPTION} mt-12`}>
        Calls go out to production. The ones that fail come back as tests.
      </p>
    </figure>
  );
}

// The failure rate as a step line over three versions, stepping down at each agent fix. Failed calls sit
// along the bottom as red ticks, thinning out as the line steps down.
const STEPS = [
  { at: 4, label: 'v41', rate: '3.1%', y: 22 },
  { at: 40, label: 'v42', rate: '1.2%', y: 62 },
  { at: 72, label: 'v43', rate: '0.3%', y: 84 },
];
const RUG = Array.from({ length: 140 }, (_, i) => {
  const x = 4 + ((i * 61) % 139) * (95 / 139);
  const k = STEPS.findLastIndex((s) => x >= s.at);
  return { failed: (i * 37) % 100 < [26, 9, 2][k], x };
}).filter((c) => c.failed);

export function HeroStepChart() {
  const n = useCalls();
  const line = `M${STEPS[0].at * 10} ${STEPS[0].y} H${STEPS[1].at * 10} V${STEPS[1].y} H${STEPS[2].at * 10} V${STEPS[2].y} H1000`;

  return (
    <figure
      aria-label="The failure rate of a classify function as a step line over three versions. It is 3.1% in v41, steps down to 1.2% at v42 after an agent's fix, and to 0.3% at v43 after another. Failed calls sit along the bottom as red ticks and thin out as the line steps down."
      className="relative py-6 md:py-12 pb-24"
    >
      <p className="mb-3 font-mono text-[12px] md:text-[14px] text-white/60">
        classify, failed calls
      </p>
      <div className="relative h-[220px] md:h-[280px]">
        <svg
          aria-hidden="true"
          className="absolute inset-0 size-full overflow-visible"
          preserveAspectRatio="none"
          viewBox="0 0 1000 100"
        >
          <line
            stroke="rgb(255 255 255 / 0.2)"
            vectorEffect="non-scaling-stroke"
            x1="0"
            x2="1000"
            y1="100"
            y2="100"
          />
          {STEPS.slice(1).map((s) => (
            <line
              key={s.label}
              stroke="rgb(255 255 255 / 0.18)"
              strokeDasharray="3 4"
              vectorEffect="non-scaling-stroke"
              x1={s.at * 10}
              x2={s.at * 10}
              y1="0"
              y2="100"
            />
          ))}
          <path
            d={line}
            fill="none"
            stroke="var(--color-clover-core)"
            strokeWidth="2"
            vectorEffect="non-scaling-stroke"
          />
        </svg>
        {STEPS.map((s, i) => (
          <span key={s.label}>
            <span
              className="absolute font-mono text-[11px] md:text-[13px] text-white/70"
              style={{
                left: `calc(${s.at}% + 6px)`,
                top: `calc(${s.y}% - 22px)`,
              }}
            >
              {s.label} <span className="text-white/40">{s.rate}</span>
            </span>
            {i > 0 && (
              <>
                <span
                  className="absolute size-2 -translate-x-1/2 -translate-y-1/2 rounded-full bg-clover"
                  style={{ left: `${s.at}%`, top: `${STEPS[i - 1].y}%` }}
                />
                <span
                  className="absolute -translate-x-1/2 text-[11px] md:text-[13px] text-clover whitespace-nowrap"
                  style={{
                    left: `${s.at}%`,
                    top: `calc(${STEPS[i - 1].y}% - 26px)`,
                  }}
                >
                  agent’s fix
                </span>
              </>
            )}
          </span>
        ))}
        <span
          className="absolute right-0 size-2 translate-x-1/2 -translate-y-1/2 rounded-full bg-clover-100 animate-pulse"
          style={{ top: `${STEPS[2].y}%` }}
        />
        {/* Failed calls along the bottom. The newest one in the live version blinks in now and then. */}
        {RUG.map((c, i) => (
          <span
            aria-hidden
            className="absolute bottom-0 w-px h-3 translate-y-full"
            // biome-ignore lint/suspicious/noArrayIndexKey: ticks keep their order
            key={i}
            style={{
              background: RED,
              left: `${c.x}%`,
              opacity: c.x > 72 && n % 40 > 30 ? 0.3 : 1,
            }}
          />
        ))}
      </div>
      <p className={`${CAPTION} mt-10 md:mt-14`}>
        Each time an agent queries the failed calls and ships a fix, the rate
        steps down.
      </p>
    </figure>
  );
}
