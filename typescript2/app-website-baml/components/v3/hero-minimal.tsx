'use client';

import { type CSSProperties, type ReactNode, useEffect, useState } from 'react';
import { RED } from './story';

// Five quiet takes on the same idea: other toolchains stop at deploy, and BAML's keeps going into
// production. Each one shows the whole comparison at once, with large type, flat color, and one accent.
// Any motion is ambient and carries no meaning of its own. The numbers are example data.

function prefersReducedMotion() {
  return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** Milliseconds since mount, every frame. Stays at 0 when motion is reduced. */
export function useNow() {
  const [t, setT] = useState(0);
  useEffect(() => {
    if (prefersReducedMotion()) return;
    const begin = performance.now();
    let id = 0;
    const tick = (now: number) => {
      setT(Math.max(0, now - begin));
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(id);
  }, []);
  return t;
}

/** Milliseconds into the current lap. Stays at 0 when motion is reduced. */
const useLap = (lapMs: number) => useNow() % lapMs;

export const clamp = (n: number) => Math.min(1, Math.max(0, n));
const ease = (n: number) => {
  const u = clamp(n);
  return u < 0.5 ? 4 * u * u * u : 1 - (-2 * u + 2) ** 3 / 2;
};

// A ring through the four stages. The half from build to deploy is white, because every toolchain covers
// it. The half from deploy back to build is clover, because only BAML does. A dot travels the whole ring.
const STATIONS = [
  { label: 'Build', style: { left: '50%', top: '0%' } },
  { label: 'Test', style: { left: '100%', top: '50%' } },
  { label: 'Deploy', style: { left: '50%', top: '100%' } },
  { label: 'Monitor', style: { left: '0%', top: '50%' } },
];
const LABEL_OFFSET = [
  'translate(-50%, calc(-100% - 18px))',
  'translate(18px, -50%)',
  'translate(-50%, 18px)',
  'translate(calc(-100% - 18px), -50%)',
];

export function HeroRing() {
  const t = useLap(7000);
  const R = 170;
  const C = 2 * Math.PI * R;
  const a = (t / 7000) * 2 * Math.PI;

  return (
    <figure
      aria-label="A ring through build, test, deploy, and monitor. The half from build to deploy is white: every toolchain covers it. The half from deploy through monitor and back to build is clover: only BAML covers it."
      className="relative pb-24"
    >
      <div className="relative mx-auto w-full max-w-[560px] aspect-square">
        <div className="absolute inset-[20%] md:inset-[14%]">
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full -rotate-90 overflow-visible"
            viewBox="-170 -170 340 340"
          >
            <circle
              fill="none"
              r={R}
              stroke="rgb(255 255 255 / 0.85)"
              strokeDasharray={`${C / 2} ${C}`}
              strokeWidth="1.5"
            />
            <circle
              fill="none"
              r={R}
              stroke="var(--color-clover-core)"
              strokeDasharray={`${C / 2} ${C}`}
              strokeDashoffset={-C / 2}
              strokeWidth="1.5"
            />
            <circle
              cx={R * Math.cos(a)}
              cy={R * Math.sin(a)}
              fill="var(--color-clover-100)"
              r="4"
            />
          </svg>
          {STATIONS.map((s, i) => (
            <span
              className="absolute size-2.5 -translate-x-1/2 -translate-y-1/2 rounded-full"
              key={s.label}
              style={{
                ...s.style,
                background: i === 3 ? 'var(--color-clover-core)' : '#fff',
              }}
            />
          ))}
          <div className="absolute inset-0 grid place-items-center text-center">
            <div>
              <p className="v2-baml-mark text-[26px] md:text-[40px]">BAML</p>
              <p className="mt-2 text-[14px] md:text-[17px] text-white/55">
                goes all the way around
              </p>
            </div>
          </div>
        </div>
        {/* Labels sit just outside the ring, at each station. */}
        <div className="absolute inset-[20%] md:inset-[14%] pointer-events-none">
          {STATIONS.map((s, i) => (
            <span
              className={`absolute font-display font-medium text-[15px] md:text-[22px] tracking-[-0.02em] whitespace-nowrap ${i === 3 ? 'text-clover' : 'text-white'}`}
              key={s.label}
              style={{ ...s.style, transform: LABEL_OFFSET[i] }}
            >
              {s.label}
            </span>
          ))}
        </div>
      </div>
      <div className="mt-4 md:mt-2 flex flex-col sm:flex-row items-center justify-center gap-x-10 gap-y-3 text-[14px] md:text-[16px]">
        <p className="flex items-center gap-3 text-white/70">
          <span aria-hidden className="w-6 h-px bg-white/85" />
          Build to deploy, in every toolchain
        </p>
        <p className="flex items-center gap-3 text-clover">
          <span aria-hidden className="w-6 h-px bg-clover" />
          Deploy back to build, only in BAML
        </p>
      </div>
    </figure>
  );
}

// The four stages as two lines of type, one per toolchain. Python, TypeScript, and Go leave Monitor as
// an outline; BAML fills it in.
const STAGES = ['Build', 'Test', 'Deploy', 'Monitor'];

export function HeroType() {
  const rows = [
    { baml: false, label: 'Python, TypeScript, Go' },
    { baml: true, label: 'BAML' },
  ];

  return (
    <figure
      aria-label="The words Build, Test, Deploy, Monitor, twice. For Python, TypeScript, and Go, Monitor is only an outline. For BAML, Monitor is filled in."
      className="relative py-6 md:py-14 pb-24"
    >
      <div className="mx-auto w-fit">
        {rows.map((r) => (
          <div
            className="grid md:grid-cols-[15rem_auto] md:items-baseline gap-y-2 gap-x-10 py-6 md:py-9 border-t border-white/10 first:border-t-0"
            key={r.label}
          >
            <p className="md:text-right">
              {r.baml ? (
                <span className="v2-baml-mark text-[20px] md:text-[26px]">
                  BAML
                </span>
              ) : (
                <span className="text-[14px] md:text-lg text-white/50">
                  {r.label}
                </span>
              )}
            </p>
            <p className="flex gap-x-[0.28em] font-display font-medium text-[clamp(24px,4.4vw,64px)] leading-none tracking-[-0.05em] whitespace-nowrap">
              {STAGES.map((s) => {
                if (s !== 'Monitor')
                  return (
                    <span
                      className={r.baml ? 'text-white' : 'text-white/80'}
                      key={s}
                    >
                      {s}
                    </span>
                  );
                return r.baml ? (
                  <span className="text-clover" key={s}>
                    {s}
                  </span>
                ) : (
                  <span
                    key={s}
                    style={
                      {
                        color: 'transparent',
                        WebkitTextStroke: '1px rgb(255 255 255 / 0.32)',
                      } as CSSProperties
                    }
                  >
                    {s}
                  </span>
                );
              })}
            </p>
          </div>
        ))}
      </div>
      <p className="mt-6 md:mt-10 text-center text-[15px] md:text-xl text-white/55">
        Other toolchains end where production starts. BAML’s keeps going.
      </p>
    </figure>
  );
}

// Each stage beside the tool it takes today and the baml command that replaces it. The four baml
// prefixes line up into one framed column.
const PREFIX_ROWS = [
  ['Build', 'tsc --noEmit', 'check'],
  ['Test', 'pytest -q', 'test'],
  ['Deploy', 'docker push app:42', 'pack'],
  ['Monitor', 'curl $TRACES/search', 'query'],
];

export function HeroPrefix() {
  const row = 'h-16 md:h-24';

  return (
    <figure
      aria-label="Four stages. Today each takes its own tool: tsc, pytest, docker, and a tracing API. With BAML they are baml check, baml test, baml pack, and baml query, and the word baml lines up in one column."
      className="relative py-6 md:py-14 pb-24"
    >
      <div className="mx-auto w-fit flex gap-6 md:gap-16">
        <div className="flex flex-col items-end text-right">
          <p className="h-8 md:h-10 text-[13px] md:text-[15px] text-white/40">
            Four tools
          </p>
          {PREFIX_ROWS.map(([stage, before]) => (
            <div
              className={`${row} flex flex-col justify-center gap-1`}
              key={stage}
            >
              <p className="font-display font-medium text-[13px] md:text-[17px] tracking-[-0.02em] text-white/70">
                {stage}
              </p>
              <p className="font-mono text-[12px] md:text-[17px] text-white/35 line-through decoration-white/25 whitespace-nowrap">
                {before}
              </p>
            </div>
          ))}
        </div>
        <div className="flex flex-col">
          <p className="h-8 md:h-10 text-[13px] md:text-[15px] text-clover">
            One toolchain
          </p>
          <div className="relative font-mono text-[22px] md:text-[44px] tracking-[-0.02em]">
            {/* The column the four prefixes make. */}
            <span
              aria-hidden
              className="absolute inset-y-1 -left-[0.35ch] w-[4.7ch] rounded-lg border border-clover"
            />
            {PREFIX_ROWS.map(([stage, , after]) => (
              <p
                className={`${row} flex items-center whitespace-nowrap`}
                key={stage}
              >
                <span className="text-clover">baml</span>&nbsp;{after}
              </p>
            ))}
          </div>
        </div>
      </div>
      <p className="mt-8 md:mt-12 text-center text-[15px] md:text-xl text-white/55">
        Each stage takes a different tool today. With BAML, all four run through
        one.
      </p>
    </figure>
  );
}

// The page split in two by deploy. Every language helps on the left; on the right, only BAML does.
const LEFT = ['Write', 'Check', 'Test'];
const RIGHT = ['Trace', 'Query', 'Fix'];

export function HeroHalves() {
  const word =
    'font-display font-medium text-[38px] sm:text-[52px] md:text-[84px] leading-[1.02] tracking-[-0.05em]';

  return (
    <figure
      aria-label="A page split down the middle by deploy. On the left, write, check, and test, where every language helps. On the right, trace, query, and fix, where BAML helps too."
      className="relative pb-20"
    >
      <div className="relative grid grid-cols-2 h-[380px] md:h-[480px]">
        <span
          aria-hidden
          className="absolute left-1/2 top-10 bottom-0 w-px bg-white/25"
        />
        <p className="absolute left-1/2 top-0 -translate-x-1/2 px-3 font-mono text-[12px] md:text-[13px] text-white/60">
          deploy
        </p>

        <div className="flex flex-col justify-center pr-5 md:pr-16 items-end text-right">
          <p className="mb-5 md:mb-8 text-[13px] md:text-[15px] text-white/50">
            Every language helps here
          </p>
          {LEFT.map((w) => (
            <p className={word} key={w}>
              {w}
            </p>
          ))}
        </div>

        <div className="flex flex-col justify-center pl-5 md:pl-16">
          <p className="mb-5 md:mb-8 text-[13px] md:text-[15px] text-clover">
            BAML helps here too
          </p>
          {RIGHT.map((w, i) => (
            <p
              className={`${word} ${i === RIGHT.length - 1 ? 'text-clover' : 'text-white'}`}
              key={w}
            >
              {w}
            </p>
          ))}
        </div>
      </div>
    </figure>
  );
}

// The same function's life on two lines, one per toolchain. Ticks mark where it was written, checked,
// tested, and deployed. The first line fades out at deploy. The BAML line goes on, and every call in
// production lands on it; a new one lights up every moment, and the count keeps climbing.
const DEPLOY = 44;
const MARKS = [
  { at: 6, label: 'Written' },
  { at: 18, label: 'Checked' },
  { at: 30, label: 'Tested' },
  { at: DEPLOY, label: 'Deployed' },
];
const CALLS = Array.from({ length: 80 }, (_, i) => ({
  bad: i % 23 === 7,
  h: 6 + ((i * 7) % 5) * 3,
  x: DEPLOY + 2 + (((i * 53) % 97) / 97) * (100 - DEPLOY - 3),
}));

/** A call count that keeps climbing, for the lifelines. Stays at 0 when motion is reduced. */
export function useCalls() {
  const [n, setN] = useState(0);
  useEffect(() => {
    if (prefersReducedMotion()) return;
    const id = setInterval(() => setN((v) => v + 1), 160);
    return () => clearInterval(id);
  }, []);
  return n;
}

const callCount = (n: number) =>
  `${(12_408 + n * 2).toLocaleString('en-US')} calls`;

/** The production calls on a BAML line, sitting on a line `bottom` up from the container's foot, from
 * `from` percent across. */
function Calls({
  bottom,
  from = 0,
  n,
}: {
  bottom: string;
  from?: number;
  n: number;
}) {
  const latest = (n * 37) % CALLS.length;
  return CALLS.map((c, i) =>
    c.x < from ? null : (
      <span
        aria-hidden
        className="absolute w-px"
        // biome-ignore lint/suspicious/noArrayIndexKey: calls keep their order
        key={i}
        style={{
          background:
            i === latest
              ? 'var(--color-clover-100)'
              : c.bad
                ? RED
                : 'rgb(from var(--color-clover-core) r g b / 0.7)',
          bottom,
          height: i === latest ? c.h + 10 : c.h,
          left: `${c.x}%`,
        }}
      />
    ),
  );
}

/** The labels over the stage ticks, in a row above the lines. On a phone the earlier labels run
 * together, so only Deployed keeps its label. */
function MarkLabels() {
  return (
    <div className="relative h-8">
      {MARKS.map((m) => (
        <span
          className={`absolute top-0 -translate-x-1/2 text-[12px] md:text-[14px] whitespace-nowrap ${m.at === DEPLOY ? 'text-white' : 'hidden sm:block text-white/50'}`}
          key={m.label}
          style={{ left: `${m.at}%` }}
        >
          {m.label}
        </span>
      ))}
    </div>
  );
}

const DEPLOY_RULE = (
  <span
    aria-hidden
    className="absolute top-8 bottom-0 border-l border-dashed border-white/20"
    style={{ left: `${DEPLOY}%` }}
  />
);

/** The line before deploy, with its stage ticks, at `top` within the lane. */
function Shipped({ dim, top }: { dim?: boolean; top: string }) {
  return (
    <>
      <span
        aria-hidden
        className={`absolute left-0 h-px ${dim ? 'bg-white/30' : 'bg-white/80'}`}
        style={{ top, width: `${DEPLOY}%` }}
      />
      {MARKS.map((m) => (
        <span
          aria-hidden
          className={`absolute w-px -translate-y-1/2 ${dim ? 'bg-white/40' : 'bg-white'}`}
          key={m.label}
          style={{
            height: m.at === DEPLOY ? (dim ? 14 : 24) : dim ? 8 : 12,
            left: `${m.at}%`,
            top,
          }}
        />
      ))}
    </>
  );
}

/** After deploy, in other toolchains: the line breaks up and fades out. */
function Fades({ dim, top }: { dim?: boolean; top: string }) {
  return (
    <span
      aria-hidden
      className={`absolute flex gap-2 ${dim ? 'opacity-50' : ''}`}
      style={{ left: `${DEPLOY + 1.5}%`, top }}
    >
      {[0.6, 0.45, 0.32, 0.22, 0.14, 0.08, 0.04].map((o) => (
        <span
          className="block w-4 md:w-8 h-px bg-white"
          key={o}
          style={{ opacity: o }}
        />
      ))}
    </span>
  );
}

/** The BAML line. Its calls land after deploy, unless `children` draws them some other way. */
function BamlLane({
  children,
  count,
  n,
}: {
  children?: ReactNode;
  count?: string;
  n: number;
}) {
  return (
    <div className="relative h-24 md:h-28">
      <p className="absolute left-0 top-2 v2-baml-mark text-[18px] md:text-[22px]">
        BAML
      </p>
      <p className="absolute right-0 top-2 font-mono text-[13px] md:text-[15px] tabular-nums text-clover">
        {count ?? callCount(n)}
      </p>
      <Shipped top="66.667%" />
      {/* After deploy, in BAML: the line goes on, and calls land on it. */}
      <span
        aria-hidden
        className="absolute right-0 top-2/3 h-px bg-clover"
        style={{ left: `${DEPLOY}%` }}
      />
      {children ?? <Calls bottom="33.333%" n={n} />}
    </div>
  );
}

function OtherLane({ label = 'Other toolchains' }: { label?: string }) {
  return (
    <div className="relative h-24 md:h-28">
      <p className="absolute left-0 top-2 text-[14px] md:text-[16px] text-white/50">
        {label}
      </p>
      <Shipped top="66.667%" />
      <Fades top="66.667%" />
    </div>
  );
}

/** A faint line for one other toolchain, fading out after deploy. */
function DimLane({ label }: { label: string }) {
  return (
    <div className="relative h-11 md:h-12">
      <p className="absolute left-0 top-0 text-[12px] md:text-[13px] text-white/35">
        {label}
      </p>
      <Shipped dim top="72%" />
      <Fades dim top="72%" />
    </div>
  );
}

export function HeroLifeline() {
  const n = useCalls();

  return (
    <figure
      aria-label="The life of a function on two lines, with ticks where it was written, checked, tested, and deployed. In other toolchains the line fades out after deploy. In BAML it keeps going, and every call in production lands on it as a tick."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative">
        <MarkLabels />
        {DEPLOY_RULE}
        <OtherLane />
        <BamlLane n={n} />
      </div>
      <p className="mt-8 md:mt-12 text-center text-[15px] md:text-xl text-white/55">
        Other toolchains see a function until it ships. BAML sees it for as long
        as it runs.
      </p>
    </figure>
  );
}

// The lifeline once per language. Python, TypeScript, and Go are faint lines that fade out at deploy,
// and BAML is the one bright line that keeps going.
const LANGUAGES = ['Python', 'TypeScript', 'Go'];

export function HeroLifelineLanguages() {
  const n = useCalls();

  return (
    <figure
      aria-label="The life of a function as four lines, one each for Python, TypeScript, Go, and BAML, with ticks where it was written, checked, tested, and deployed. The Python, TypeScript, and Go lines are faint and fade out after deploy. The BAML line keeps going, and every call in production lands on it as a tick."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative">
        <MarkLabels />
        {DEPLOY_RULE}
        <div className="pt-2">
          {LANGUAGES.map((l) => (
            <DimLane key={l} label={l} />
          ))}
        </div>
        <BamlLane n={n} />
      </div>
      <p className="mt-8 md:mt-12 text-center text-[15px] md:text-xl text-white/55">
        Every language sees a function until it ships. BAML sees it for as long
        as it runs.
      </p>
    </figure>
  );
}

// One line that forks at deploy. The upper branch, for other toolchains, fades out. The lower branch,
// for BAML, keeps going in clover, and every call in production lands on it.
export function HeroLifelineFork() {
  const n = useCalls();
  const x0 = DEPLOY * 10 + 15;

  return (
    <figure
      aria-label="The life of a function as one line, with ticks where it was written, checked, tested, and deployed. At deploy it forks. The upper branch, for other toolchains, fades out. The lower branch, for BAML, keeps going, and every call in production lands on it as a tick."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative">
        <MarkLabels />
        {DEPLOY_RULE}
        <div className="relative h-[220px] md:h-[260px]">
          <p className="absolute left-0 top-[calc(50%-36px)] font-mono text-[14px] md:text-[17px] text-white/80">
            classify
          </p>
          <Shipped top="50%" />
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full overflow-visible"
            preserveAspectRatio="none"
            viewBox="0 0 1000 200"
          >
            <path
              d={`M${DEPLOY * 10} 100 H${x0} C${x0 + 40} 100 ${x0 + 40} 40 ${x0 + 80} 40`}
              fill="none"
              stroke="rgb(255 255 255 / 0.5)"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
            {[0.4, 0.3, 0.22, 0.15, 0.09, 0.05].map((o, i) => (
              <line
                key={o}
                stroke="#fff"
                strokeOpacity={o}
                strokeWidth="1"
                vectorEffect="non-scaling-stroke"
                x1={x0 + 92 + i * 34}
                x2={x0 + 112 + i * 34}
                y1="40"
                y2="40"
              />
            ))}
            <path
              d={`M${DEPLOY * 10} 100 H${x0} C${x0 + 40} 100 ${x0 + 40} 160 ${x0 + 80} 160 H1000`}
              fill="none"
              stroke="var(--color-clover-core)"
              strokeWidth="1"
              vectorEffect="non-scaling-stroke"
            />
          </svg>
          <p
            className="absolute top-[calc(20%-30px)] text-[13px] md:text-[15px] text-white/50 whitespace-nowrap"
            style={{ left: `${(x0 + 92) / 10}%` }}
          >
            Other toolchains
          </p>
          <p
            className="absolute top-[calc(80%+12px)] v2-baml-mark text-[16px] md:text-[20px]"
            style={{ left: `${(x0 + 92) / 10}%` }}
          >
            BAML
          </p>
          <p className="absolute right-0 top-[calc(80%+14px)] font-mono text-[13px] md:text-[15px] tabular-nums text-clover">
            {callCount(n)}
          </p>
          <Calls bottom="20%" from={(x0 + 90) / 10} n={n} />
        </div>
      </div>
      <p className="mt-8 md:mt-12 text-center text-[15px] md:text-xl text-white/55">
        Other toolchains see a function until it ships. BAML sees it for as long
        as it runs.
      </p>
    </figure>
  );
}

// The lifeline for Python, TypeScript, and Go, where seeing production after deploy takes five separate
// integrations: short lines with gaps between them. BAML's line runs straight through.
const INTEGRATIONS = ['Tracing', 'Errors', 'Metrics', 'Analytics', 'Alerts'];

export function HeroLifelineIntegrations() {
  const n = useCalls();
  const span = (100 - DEPLOY - 2) / INTEGRATIONS.length;

  return (
    <figure
      aria-label="The life of a function on two lines, with ticks where it was written, checked, tested, and deployed. For Python, TypeScript, and Go, the line after deploy breaks into five separate pieces: tracing, errors, metrics, analytics, and alerts. For BAML, the line keeps going, and every call in production lands on it as a tick."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative">
        <MarkLabels />
        {DEPLOY_RULE}
        <div className="relative h-28 md:h-32">
          <p className="absolute left-0 top-2 text-[14px] md:text-[16px] text-white/50">
            Python, TypeScript, Go
          </p>
          <p className="absolute right-0 top-2 text-[13px] md:text-[15px] text-white/50">
            5 integrations
            <span className="hidden sm:inline">, wired by hand</span>
          </p>
          <Shipped top="58%" />
          {INTEGRATIONS.map((name, i) => {
            const left = DEPLOY + 2.4 + i * span;
            const width = span - 2.4;
            return (
              <span key={name}>
                <span
                  aria-hidden
                  className="absolute h-px bg-white/45"
                  style={{ left: `${left}%`, top: '58%', width: `${width}%` }}
                />
                {[left, left + width].map((x) => (
                  <span
                    aria-hidden
                    className="absolute w-px h-2 -translate-y-1/2 bg-white/45"
                    key={x}
                    style={{ left: `${x}%`, top: '58%' }}
                  />
                ))}
                <span
                  className="absolute hidden sm:block top-[calc(58%+12px)] text-[12px] md:text-[13px] text-white/45"
                  style={{ left: `${left}%` }}
                >
                  {name}
                </span>
              </span>
            );
          })}
        </div>
        <BamlLane n={n} />
      </div>
    </figure>
  );
}

// The calls after deploy as a field of bars that each light up and fade, at their own pace, like
// traffic on a live system. Every bar is always there, so the shape reads before any of them flash.
export function HeroLifelinePulse() {
  const t = useNow();

  return (
    <figure
      aria-label="The life of a function on two lines, with ticks where it was written, checked, tested, and deployed. In other toolchains the line fades out after deploy. In BAML it keeps going, and the calls on it light up and fade as they happen."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative">
        <MarkLabels />
        {DEPLOY_RULE}
        <OtherLane />
        <BamlLane count={callCount(Math.floor(t / 160))} n={0}>
          {CALLS.map((c, i) => {
            const period = 2200 + ((i * 397) % 2600);
            const age = (t + ((i * 911) % period)) % period;
            const glow = Math.exp(-age / 450);
            return (
              <span
                aria-hidden
                className="absolute w-px"
                // biome-ignore lint/suspicious/noArrayIndexKey: calls keep their order
                key={i}
                style={{
                  background: c.bad
                    ? RED
                    : glow > 0.5
                      ? 'var(--color-clover-100)'
                      : 'var(--color-clover-core)',
                  bottom: '33.333%',
                  height: c.h * (0.6 + 0.7 * glow),
                  left: `${c.x}%`,
                  opacity: 0.3 + 0.7 * glow,
                }}
              />
            );
          })}
        </BamlLane>
      </div>
    </figure>
  );
}

// The calls after deploy as a stream. The right edge is now: each call arrives there and moves left as it
// ages, until it reaches deploy. The stream starts full, so the first frame already shows it.
const RATE = 5; // calls per second
const SPEED = 4; // percent of the width per second
const PREFILL = 20_000;

export function HeroLifelineStream() {
  const t = (useNow() + PREFILL) / 1000;
  const range = 100 - DEPLOY - 1;
  const newest = Math.floor(t * RATE);
  const oldest = Math.ceil((t - range / SPEED) * RATE);
  const bars = [];
  for (let j = oldest; j <= newest; j++) {
    // Calls arrive a little unevenly, as real traffic does.
    const age = t - (j + ((j * 61) % 7) / 8) / RATE;
    if (age < 0) continue;
    const x = 100 - age * SPEED;
    const bad = j % 23 === 7;
    bars.push(
      <span
        aria-hidden
        className="absolute w-px"
        key={j}
        style={{
          background: bad
            ? RED
            : age < 0.4
              ? 'var(--color-clover-100)'
              : 'rgb(from var(--color-clover-core) r g b / 0.7)',
          bottom: '33.333%',
          height: 6 + ((j * 7) % 5) * 3 + (age < 0.4 ? 8 : 0),
          left: `${x}%`,
          opacity: clamp((x - DEPLOY - 1) / 5),
        }}
      />,
    );
  }

  return (
    <figure
      aria-label="The life of a function on two lines, with ticks where it was written, checked, tested, and deployed. In other toolchains the line fades out after deploy. In BAML it keeps going, and calls stream in from the right edge, which is now, and move left as they age."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative">
        <MarkLabels />
        {DEPLOY_RULE}
        <OtherLane />
        <BamlLane count={callCount(newest - PREFILL / 200)} n={0}>
          {bars}
          <p className="absolute right-0 top-[calc(66.667%+10px)] text-[11px] md:text-[12px] text-white/40">
            now
          </p>
        </BamlLane>
      </div>
    </figure>
  );
}

// The loop as versions of one function, staggered on one time axis. Each version runs in production until
// the next one replaces it. An agent queries the calls that failed, fixes the code, and redeploys, so the
// failed calls of each version fan in, through baml query, to the start of the next, and each version
// fails less than the one before.
const VERSIONS = [
  { bad: [16, 20, 24, 27], end: 42, errors: '3.1% failed', start: 0, v: 'v41' },
  { bad: [47, 53], end: 72, errors: '1.2% failed', start: 30, v: 'v42' },
  { bad: [], end: 100, errors: '0.3% failed', start: 60, v: 'v43' },
];
const TESTED_AT = 6;
const DEPLOYED_AT = 12;
const LANE_LINE = 60; // percent down each lane

/** Calls on one version's production stretch, spread evenly with a little jitter. */
function laneCalls(from: number, to: number) {
  const out = [];
  for (let x = from + 1; x < to - 0.5; x += 0.8) {
    const k = Math.round(x * 5);
    out.push({ h: 5 + ((k * 7) % 5) * 3, x: x + ((k * 13) % 5) * 0.08 });
  }
  return out;
}

export function HeroLifelineLoop() {
  const n = useCalls();
  const live = laneCalls(VERSIONS[2].start + DEPLOYED_AT, 100);
  const latest = (n * 7) % live.length;

  return (
    <figure
      aria-label="Three versions of a function, v41, v42, and v43, staggered in time. Each runs in production until the next replaces it. The calls that failed in v41 fan in through baml query to the start of v42, the agent's fix, and v42's failures do the same for v43. Failures fall from 3.1% to 1.2% to 0.3%."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="flex items-center justify-between mb-4 md:mb-6">
        <p className="font-mono text-[14px] md:text-[17px] text-white/80">
          classify
        </p>
        <p className="flex items-center gap-2 text-[12px] md:text-[14px] text-white/50">
          <span aria-hidden className="w-px h-3" style={{ background: RED }} />
          failed call
        </p>
      </div>
      <div className="grid grid-cols-[2.25rem_1fr] md:grid-cols-[3.5rem_1fr]">
        <div>
          {VERSIONS.map((ver) => (
            <p
              className="relative h-24 md:h-28 font-mono text-[12px] md:text-[14px] text-white/50"
              key={ver.v}
            >
              <span
                className="absolute -translate-y-1/2"
                style={{ top: `${LANE_LINE}%` }}
              >
                {ver.v}
              </span>
            </p>
          ))}
        </div>
        <div className="relative">
          {/* The agent's hops: failed calls fan in to the next version's start. */}
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full overflow-visible"
            preserveAspectRatio="none"
            viewBox="0 0 100 300"
          >
            {VERSIONS.slice(0, -1).flatMap((ver, i) => {
              const y1 = i * 100 + LANE_LINE;
              const y2 = y1 + 100;
              const to = VERSIONS[i + 1].start;
              return ver.bad.map((b) => (
                <path
                  d={`M${b} ${y1} C${b} ${y1 + 45} ${to} ${y2 - 45} ${to} ${y2}`}
                  fill="none"
                  key={b}
                  stroke="var(--color-clover-core)"
                  strokeOpacity="0.55"
                  strokeWidth="1"
                  vectorEffect="non-scaling-stroke"
                />
              ));
            })}
          </svg>
          {VERSIONS.slice(0, -1).map((ver, i) => {
            const mid = ver.bad.reduce((sum, b) => sum + b, 0) / ver.bad.length;
            return (
              <span
                className="absolute z-10 -translate-x-1/2 -translate-y-1/2 rounded-full border border-clover/50 bg-black px-2 py-0.5 font-mono text-[11px] md:text-[13px] text-clover whitespace-nowrap"
                key={ver.v}
                style={{
                  left: `${(mid + VERSIONS[i + 1].start) / 2}%`,
                  top: `${(i * 100 + LANE_LINE + 50) / 3}%`,
                }}
              >
                baml query
              </span>
            );
          })}

          {VERSIONS.map((ver, i) => {
            const isLive = i === VERSIONS.length - 1;
            const deployed = ver.start + DEPLOYED_AT;
            const calls = isLive ? live : laneCalls(deployed, ver.end);
            const ticks = [
              {
                at: ver.start,
                h: 10,
                label: i === 0 ? 'written' : 'agent’s fix',
              },
              { at: ver.start + TESTED_AT, h: 10, label: 'tested' },
              { at: deployed, h: 20, label: 'deployed' },
            ];
            return (
              <div className="relative h-24 md:h-28" key={ver.v}>
                <span
                  aria-hidden
                  className="absolute h-px bg-white/80"
                  style={{
                    left: `${ver.start}%`,
                    top: `${LANE_LINE}%`,
                    width: `${DEPLOYED_AT}%`,
                  }}
                />
                {ticks.map((k, j) => (
                  <span key={k.label}>
                    <span
                      aria-hidden
                      className={`absolute w-px -translate-y-1/2 ${j === 0 && i > 0 ? 'bg-clover' : 'bg-white'}`}
                      style={{
                        height: k.h,
                        left: `${k.at}%`,
                        top: `${LANE_LINE}%`,
                      }}
                    />
                    <span
                      className={`absolute -translate-x-1/2 top-[calc(60%+12px)] text-[11px] md:text-[13px] whitespace-nowrap ${j === 0 && i > 0 ? 'text-clover' : 'hidden sm:block text-white/45'}`}
                      style={{ left: `${k.at}%` }}
                    >
                      {k.label}
                    </span>
                  </span>
                ))}
                <span
                  aria-hidden
                  className="absolute h-px bg-clover"
                  style={{
                    left: `${deployed}%`,
                    opacity: isLive ? 1 : 0.5,
                    top: `${LANE_LINE}%`,
                    width: `${ver.end - deployed}%`,
                  }}
                />
                {calls.map((c, j) => (
                  <span
                    aria-hidden
                    className="absolute w-px"
                    // biome-ignore lint/suspicious/noArrayIndexKey: calls keep their order
                    key={j}
                    style={{
                      background:
                        isLive && j === latest
                          ? 'var(--color-clover-100)'
                          : 'rgb(from var(--color-clover-core) r g b / 0.6)',
                      bottom: `${100 - LANE_LINE}%`,
                      height: isLive && j === latest ? c.h + 8 : c.h,
                      left: `${c.x}%`,
                    }}
                  />
                ))}
                {ver.bad.map((b) => (
                  <span
                    aria-hidden
                    className="absolute w-0.5 -translate-x-1/4"
                    key={b}
                    style={{
                      background: RED,
                      bottom: `${100 - LANE_LINE}%`,
                      height: 16,
                      left: `${b}%`,
                    }}
                  />
                ))}
                <p
                  className="absolute top-1 font-mono text-[11px] md:text-[13px] tabular-nums text-white/55 whitespace-nowrap"
                  style={{ right: `${100 - ver.end}%` }}
                >
                  {ver.errors}
                </p>
              </div>
            );
          })}
        </div>
      </div>
      <p className="mt-8 md:mt-12 text-center text-[15px] md:text-xl text-white/55">
        Agents query the calls that failed, fix the code, and redeploy. Each
        version fails less than the one before.
      </p>
    </figure>
  );
}

// The lifeline with one call opened up, so it is plain what the ticks after deploy are: each is one call
// to the function in production, recorded with its input, output, latency, and tokens.
const SAMPLES = [
  {
    input: 'Charged twice for March.',
    ms: '1.31s',
    output: 'billing',
    tokens: 198,
  },
  {
    input: 'The export button does nothing.',
    ms: '1.18s',
    output: 'bug',
    tokens: 187,
  },
  {
    input: 'Can I change my plan?',
    ms: '0.97s',
    output: 'billing',
    tokens: 164,
  },
  {
    bad: true,
    input: 'Third time asking. Refund me.',
    ms: '2.90s',
    output: 'Parse error',
    tokens: 241,
  },
];
const OPENED = 88;

export function HeroLifelineExplained() {
  const n = useCalls();
  const s = SAMPLES[Math.floor(n / 16) % SAMPLES.length];
  const rows: [string, ReactNode][] = [
    ['input', `“${s.input}”`],
    [
      'output',
      <span
        className="text-clover"
        key="o"
        style={{ color: s.bad ? RED : undefined }}
      >
        {s.output}
      </span>,
    ],
    ['latency', s.ms],
    ['tokens', s.tokens],
  ];

  return (
    <figure
      aria-label="The life of a function on two lines, with ticks where it was written, checked, tested, and deployed. In other toolchains the line fades out after deploy. In BAML it keeps going, and each tick after deploy is one call in production. One call is opened up to show its input, output, latency, and tokens."
      className="relative py-6 md:py-12 pb-24"
    >
      <div className="relative">
        <MarkLabels />
        {DEPLOY_RULE}
        <OtherLane />
        <div className="relative h-[220px] md:h-[240px]">
          <p className="absolute left-0 top-2 v2-baml-mark text-[18px] md:text-[22px]">
            BAML
          </p>
          <Shipped top="80%" />
          <span
            aria-hidden
            className="absolute right-0 top-[80%] h-px bg-clover"
            style={{ left: `${DEPLOY}%` }}
          />
          <Calls bottom="20%" n={n} />
          {/* The opened call: a taller tick, a leader, and its record. */}
          <span
            aria-hidden
            className="absolute w-px bg-white/30"
            style={{ bottom: '20%', height: 40, left: `${OPENED}%` }}
          />
          <span
            aria-hidden
            className="absolute w-0.5 -translate-x-1/4"
            style={{
              background: s.bad ? RED : 'var(--color-clover-100)',
              bottom: '20%',
              height: 18,
              left: `${OPENED}%`,
            }}
          />
          <div className="absolute right-0 bottom-[calc(20%+40px)] w-[230px] md:w-[300px] rounded-md border border-white/15 bg-black px-3 py-2.5 font-mono text-[11px] md:text-[13px] leading-[1.7]">
            <p className="flex justify-between text-white/80">
              <span>classify</span>
              <span className="text-white/40 tabular-nums">
                call {(12_408 + n * 2).toLocaleString('en-US')}
              </span>
            </p>
            {rows.map(([k, v]) => (
              <p className="flex gap-3" key={k}>
                <span className="w-14 shrink-0 text-white/40">{k}</span>
                <span className="truncate text-white/80">{v}</span>
              </p>
            ))}
          </div>
          <p
            className="absolute top-[calc(80%+10px)] right-0 text-[12px] md:text-[13px] text-white/50"
            style={{ left: `${DEPLOY + 1.5}%` }}
          >
            Each tick is one call in production. Red ones failed.
          </p>
        </div>
      </div>
      <p className="mt-8 md:mt-12 text-center text-[15px] md:text-xl text-white/55">
        After deploy, BAML records every call: what went in, what came out, how
        long it took, and whether it failed.
      </p>
    </figure>
  );
}

// One ring in two segments, in the manner of a product keynote. The left segment is production and the
// right is build and test. Calls flow all the way around: up through production, across the top into
// build and test, and down through deploy back into production. A call that failed in production (red)
// comes out of build and test fixed (clover), and when it passes deploy the version in the center goes up.
const RING_R = 150;
const RING_W = 22;
const GAP = 7; // degrees left open at the top and bottom, between the two segments
const FLOW_RATE = 4; // dots per second
const LAP = 10; // seconds for a dot to go all the way around
const FAILED_EVERY = 19; // every nth call fails in production
const RING_PREFILL = 30; // seconds, so the ring starts full

/** A point on the ring, `deg` clockwise from the top. */
const onRing = (deg: number, r = RING_R) => {
  const a = (deg * Math.PI) / 180;
  return { x: r * Math.sin(a), y: -r * Math.cos(a) };
};

/** An arc clockwise from `from` to `to` degrees, as an SVG path. */
const arc = (from: number, to: number, r = RING_R) => {
  const a = onRing(from, r);
  const b = onRing(to, r);
  return `M${a.x} ${a.y} A${r} ${r} 0 ${to - from > 180 ? 1 : 0} 1 ${b.x} ${b.y}`;
};

/** How far a dot at `deg` is from the nearest gap, as an opacity, so dots fade through the gaps. */
const awayFromGaps = (deg: number) => {
  const d = Math.min(deg % 180, 180 - (deg % 180));
  return clamp((d - GAP * 0.4) / GAP);
};

export function HeroRingFlow() {
  const t = useNow() / 1000 + RING_PREFILL;
  // Each failed call that has come back around through deploy is one more version.
  const fixesShipped = Math.floor(
    Math.max(0, (t - LAP) * FLOW_RATE) / FAILED_EVERY,
  );
  const version =
    42 +
    fixesShipped -
    Math.floor(((RING_PREFILL - LAP) * FLOW_RATE) / FAILED_EVERY);

  const newest = Math.floor(t * FLOW_RATE);
  const dots = [];
  for (let j = Math.ceil((t - LAP) * FLOW_RATE); j <= newest; j++) {
    const age = t - j / FLOW_RATE;
    if (age < 0 || age >= LAP) continue;
    const deg = (180 + (age / LAP) * 360) % 360; // starts at deploy, at the bottom
    const inProduction = deg >= 180;
    const failed = j % FAILED_EVERY === 0;
    const p = onRing(deg, RING_R + (((j * 7) % 5) - 2) * 2.2);
    dots.push(
      <circle
        cx={p.x}
        cy={p.y}
        fill={
          inProduction
            ? failed
              ? RED
              : '#fff'
            : failed
              ? 'var(--color-clover-core)'
              : 'rgb(0 0 0 / 0.55)'
        }
        key={j}
        opacity={awayFromGaps(deg)}
        r={failed ? 3.6 : 2.6}
      />,
    );
  }

  const ring = {
    fill: 'none',
    strokeLinecap: 'round',
    strokeWidth: RING_W,
  } as const;

  return (
    <figure
      aria-label="A ring in two segments: production on the left and build and test on the right. Calls flow all the way around, up through production, into build and test, and through deploy back into production. A failed call comes out of build and test fixed, and when it passes deploy, the version in the center goes up."
      className="relative pb-24"
    >
      <svg
        aria-hidden="true"
        className="mx-auto block w-full max-w-[480px] aspect-square"
        viewBox="-180 -180 360 360"
      >
        <path {...ring} d={arc(GAP, 180 - GAP)} stroke="#fff" />
        <path
          {...ring}
          d={arc(180 + GAP, 360 - GAP)}
          stroke="rgb(from var(--color-clover-core) r g b / 0.45)"
        />
        {dots}
        <text
          className="fill-white/60 font-mono text-[16px]"
          textAnchor="middle"
          y="-46"
        >
          classify
        </text>
        <text
          className="fill-white font-display font-semibold text-[78px] v3-enter"
          key={version}
          letterSpacing="-0.05em"
          textAnchor="middle"
          y="22"
        >
          v{version}
        </text>
        <text className="fill-white/55 text-[16px]" textAnchor="middle" y="56">
          in production
        </text>
      </svg>
      <div className="mt-6 flex items-center justify-center gap-8 text-[14px] md:text-[16px] text-white/70">
        <p className="flex items-center gap-2.5">
          <span aria-hidden className="h-2 w-5 rounded-full bg-clover/60" />
          Production
        </p>
        <p className="flex items-center gap-2.5">
          <span aria-hidden className="h-2 w-5 rounded-full bg-white" />
          Build and test
        </p>
      </div>
      <p className="mt-6 md:mt-8 text-center text-[15px] md:text-xl text-white/55">
        Calls from production flow back into build and test, and every fix ships
        with the next deploy.
      </p>
    </figure>
  );
}

// Production as a grid of calls, one cell per call, in three stretches: v41, v42, and v43, the live one.
// Failed calls are red. Threads rise from each failed call and gather at the start of the next version,
// where an agent's fix ships, so each stretch has fewer red cells than the one before. Cells in the live
// stretch twinkle like traffic, and one thread at a time lights up.
const CELL = 9;
const CELL_GAP = 2;
const PITCH = CELL + CELL_GAP;
const GRID_ROWS = 11;
const THREAD_ROOM = 96; // room above the grid for the threads to gather
const STRETCHES = [
  { end: 0.38, failed: '3.1% failed', failRate: 0.085, v: 'v41' },
  { end: 0.7, failed: '1.2% failed', failRate: 0.03, v: 'v42' },
  { end: 1, failed: '0.3% failed', failRate: 0.006, v: 'v43' },
];

/** A stable pseudo-random number in [0, 1) for a cell. */
const cellHash = (c: number, r: number, salt = 0) => {
  const x = Math.sin(c * 127.1 + r * 311.7 + salt * 74.7) * 43758.5453;
  return x - Math.floor(x);
};

/** The width of an element, kept current. */
function useWidth<T extends HTMLElement>() {
  const [el, setEl] = useState<T | null>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    if (!el) return;
    const ro = new ResizeObserver(([entry]) =>
      setWidth(entry.contentRect.width),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, [el]);
  return [setEl, width] as const;
}

export function HeroCallGrid() {
  const [ref, width] = useWidth<HTMLDivElement>();
  const n = useCalls();
  const cols = Math.max(24, Math.floor((width + CELL_GAP) / PITCH));
  const gridW = cols * PITCH - CELL_GAP;
  const gridTop = THREAD_ROOM;
  const gridH = GRID_ROWS * PITCH - CELL_GAP;
  const starts = STRETCHES.map((_, i) =>
    i === 0 ? 0 : Math.round(STRETCHES[i - 1].end * cols),
  );

  const cells = [];
  const threads: { c: number; r: number; to: number }[] = [];
  for (let c = 0; c < cols; c++) {
    const k = starts.findLastIndex((s) => c >= s);
    const live = k === STRETCHES.length - 1;
    for (let r = 0; r < GRID_ROWS; r++) {
      const failed = cellHash(c, r) < STRETCHES[k].failRate;
      if (failed && !live) threads.push({ c, r, to: starts[k + 1] });
      const twinkle = live && cellHash(c, r, n) < 0.025;
      cells.push(
        <rect
          fill={
            failed
              ? RED
              : twinkle
                ? 'var(--color-clover-100)'
                : 'var(--color-clover-core)'
          }
          height={CELL}
          key={`${c}-${r}`}
          opacity={failed || twinkle ? 1 : 0.18 + cellHash(c, r, 1) * 0.5}
          rx="1.5"
          width={CELL}
          x={c * PITCH}
          y={gridTop + r * PITCH}
        />,
      );
    }
  }
  const lit = threads.length ? n % threads.length : -1;
  const deployX = (c: number) => c * PITCH - CELL_GAP / 2;

  return (
    <figure
      aria-label="Production as a grid of calls in three stretches, v41, v42, and v43. Failed calls are red. Threads rise from each failed call and gather at the start of the next version, where an agent's fix ships. Each stretch has fewer failed calls than the one before: 3.1%, 1.2%, and 0.3%."
      className="relative py-6 md:py-12 pb-24"
    >
      <div ref={ref}>
        {width > 0 && (
          <svg
            aria-hidden="true"
            className="block overflow-visible"
            height={gridTop + gridH}
            width={gridW}
          >
            {cells}
            {threads.map((th, i) => {
              const x0 = th.c * PITCH + CELL / 2;
              const y0 = gridTop + th.r * PITCH + CELL / 2;
              const x1 = deployX(th.to);
              return (
                <path
                  d={`M${x0} ${y0} C${x0} ${gridTop - 30} ${x1} ${gridTop - 10} ${x1} 12`}
                  fill="none"
                  key={`${th.c}-${th.r}`}
                  stroke={
                    i === lit
                      ? 'var(--color-clover-100)'
                      : 'var(--color-clover-core)'
                  }
                  strokeOpacity={i === lit ? 0.95 : 0.3}
                  strokeWidth="1"
                />
              );
            })}
            {starts.slice(1).map((c) => (
              <g key={c}>
                <line
                  stroke="#fff"
                  strokeOpacity="0.7"
                  x1={deployX(c)}
                  x2={deployX(c)}
                  y1={12}
                  y2={gridTop + gridH + 8}
                />
                <circle
                  cx={deployX(c)}
                  cy={12}
                  fill="var(--color-clover-core)"
                  r="4"
                />
              </g>
            ))}
          </svg>
        )}
        <div className="relative h-10 mt-3">
          {STRETCHES.map((st, i) => (
            <p
              className="absolute flex flex-wrap gap-x-2 text-[11px] md:text-[13px]"
              key={st.v}
              style={{ left: i === 0 ? 0 : deployX(starts[i]) + 8 }}
            >
              <span className="font-mono text-white/70">{st.v}</span>
              <span className="text-white/40">{st.failed}</span>
            </p>
          ))}
        </div>
      </div>
      <p className="mt-6 md:mt-8 text-center text-[15px] md:text-xl text-white/55">
        Agents trace each failed call back to the code, and every release fails
        less.
      </p>
    </figure>
  );
}
