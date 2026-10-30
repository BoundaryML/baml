'use client';

import logoWhite from '@boundaryml/brand/logo/svg/boundary-primary-white.svg';
import { memo, type ReactNode, useEffect, useRef, useState } from 'react';
import { track } from '../track';

// The hero's sentence as a machine. Spans and typed records come in on the left. The chip in the middle
// types a query, and when it finishes the result runs down one trace to the tool that query powers. The
// queries use the tables baml query reads; the small pictures in each tool are illustrative.
type Tool = { key: string; label: string; query: string };

const TOOLS: Tool[] = [
  {
    key: 'alerts',
    label: 'Alerts',
    query: `SELECT span_name, count(*) AS errors
FROM spans
WHERE status = 'user_error'
GROUP BY span_name`,
  },
  {
    key: 'dashboards',
    label: 'Dashboards',
    query: `SELECT span_name, count(*) AS calls,
  avg(duration) / 1e6 AS avg_ms
FROM spans
GROUP BY span_name`,
  },
  {
    key: 'evals',
    label: 'AI evals',
    query: `SELECT input_args, error_value
FROM spans
WHERE span_name = 'user.ExtractInvoice'
  AND status = 'user_error'`,
  },
  {
    key: 'own',
    label: 'Your own tooling',
    query: `SELECT
  context_metadata['customer'] AS customer,
  count(*) AS calls
FROM spans GROUP BY customer`,
  },
];

const FIELDS: [string, string][] = [
  ['typed args', 'file: pdf'],
  ['result', 'Invoice'],
  ['call path', 'Agent > Extract'],
  ['code version', 'a41f9c2'],
];

const ERROR = '#E5484D';
const TYPE_MS = 20;
const HOLD_MS = 3600;

// The frame: 1280×570, with the chip in the middle and every trace on right angles.
const CHIP = { h: 320, w: 340, x: 470, y: 120 };
const CHIP_RIGHT = CHIP.x + CHIP.w;
const SRC_RIGHT = 290;
const TOOL_X = 990;
const TOOL_W = 290;
const TOOL_H = 112;
const TOOL_Y = [40, 166, 292, 418];
const OUT_PINS = [220, 270, 320, 370];
const OUT_LANES = [880, 905, 905, 880];
const IN_PINS = [270, 300, 330, 360];
const IN_LANES = [340, 365, 390, 415];
const FIELD_Y = [324, 362, 400, 438];
const OTEL_PATH = route([
  [SRC_RIGHT, 140],
  [440, 140],
  [440, 220],
  [CHIP.x, 220],
]);

/** A path through right-angle points, with the corners slightly rounded like a trace on a board. */
function route(points: [number, number][], r = 7) {
  let d = `M${points[0][0]} ${points[0][1]}`;
  for (let i = 1; i < points.length - 1; i++) {
    const [x0, y0] = points[i - 1];
    const [x, y] = points[i];
    const [x1, y1] = points[i + 1];
    const inX = Math.sign(x - x0) * r;
    const inY = Math.sign(y - y0) * r;
    const outX = Math.sign(x1 - x) * r;
    const outY = Math.sign(y1 - y) * r;
    d += ` L${x - inX} ${y - inY} Q${x} ${y} ${x + outX} ${y + outY}`;
  }
  const [lx, ly] = points.at(-1) ?? points[0];
  return `${d} L${lx} ${ly}`;
}

const inPath = (i: number) =>
  route([
    [SRC_RIGHT, FIELD_Y[i]],
    [IN_LANES[i], FIELD_Y[i]],
    [IN_LANES[i], IN_PINS[i]],
    [CHIP.x, IN_PINS[i]],
  ]);

const outPath = (i: number) => {
  const y = TOOL_Y[i] + TOOL_H / 2;
  return route([
    [CHIP_RIGHT, OUT_PINS[i]],
    [OUT_LANES[i], OUT_PINS[i]],
    [OUT_LANES[i], y],
    [TOOL_X, y],
  ]);
};

const delay = (s: number) => ({ '--delay': `${s}s` }) as React.CSSProperties;

// Keywords and string literals pick up the same colours as the other query windows.
function highlight(line: string): ReactNode[] {
  return line
    .split(/(\b(?:SELECT|FROM|WHERE|GROUP BY|AND|AS)\b|'[^']*'?)/)
    .map((part, i) => {
      const key = `${i}-${part}`;
      if (part.startsWith("'"))
        return (
          <tspan fill="var(--color-pasture-200)" key={key}>
            {part}
          </tspan>
        );
      return /^(SELECT|FROM|WHERE|GROUP BY|AND|AS)$/.test(part) ? (
        <tspan fill="var(--color-clover-core)" key={key}>
          {part}
        </tspan>
      ) : (
        <tspan key={key}>{part}</tspan>
      );
    });
}

/** Cycles through the tools while on screen, typing each query. Hovering or tapping holds one. */
function useFlow() {
  const [active, setActive] = useState(0);
  const [chars, setChars] = useState(TOOLS[0].query.length);
  const [held, setHeld] = useState(false);
  const [visible, setVisible] = useState(false);
  const [still, setStill] = useState(false);
  const ref = useRef<HTMLElement>(null);

  useEffect(() => {
    setStill(window.matchMedia('(prefers-reduced-motion: reduce)').matches);
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => setVisible(e.isIntersecting), {
      threshold: 0.15,
    });
    io.observe(el);
    return () => io.disconnect();
  }, []);

  const length = TOOLS[active].query.length;

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new tool restarts the typing
  useEffect(() => {
    if (still || !visible) {
      setChars(length);
      return;
    }
    setChars(0);
    const id = setInterval(
      () =>
        setChars((c) => {
          if (c >= length) clearInterval(id);
          return Math.min(c + 1, length);
        }),
      TYPE_MS,
    );
    return () => clearInterval(id);
  }, [active, length, still, visible]);

  const done = chars >= length;

  useEffect(() => {
    if (still || held || !visible || !done) return;
    const id = setTimeout(
      () => setActive((a) => (a + 1) % TOOLS.length),
      HOLD_MS,
    );
    return () => clearTimeout(id);
  }, [done, held, still, visible]);

  const select = (i: number) => {
    if (i !== active) {
      track('flow_tool_selected', { tool: TOOLS[i].key });
      setActive(i);
    }
    setHeld(true);
  };

  return { active, chars, done, ref, release: () => setHeld(false), select };
}

/** Spans of one trace, each redrawing from the left as new calls arrive. */
function Waterfall() {
  const bars: [number, number, number][] = [
    [20, 230, 0],
    [36, 96, 0.3],
    [60, 60, 0.6],
    [136, 90, 0.9],
    [150, 42, 1.2],
  ];
  return (
    <g>
      {bars.map(([x, w, d], i) => (
        <rect
          className="v2-span"
          fill={i === 0 ? 'rgb(255 255 255 / 0.5)' : 'rgb(255 255 255 / 0.24)'}
          height={6}
          key={d}
          rx={1}
          style={delay(d)}
          width={w}
          x={x}
          y={118 + i * 15}
        />
      ))}
    </g>
  );
}

// Nothing here changes with the cycle, so it stays out of the render that types each character.
const Sources = memo(function Sources() {
  return (
    <g>
      <rect
        fill="#000"
        height={140}
        rx={8}
        stroke="var(--v2-line)"
        width={SRC_RIGHT}
        x={0.5}
        y={70.5}
      />
      <text fill="#fff" fontSize={17} x={20} y={99}>
        OTEL traces
      </text>
      <text fill="var(--v2-dim)" fontSize={13} textAnchor="end" x={270} y={99}>
        from any service
      </text>
      <Waterfall />

      <rect
        fill="#000"
        height={240}
        rx={8}
        stroke="var(--v2-baml)"
        strokeOpacity={0.55}
        width={SRC_RIGHT}
        x={0.5}
        y={250.5}
      />
      <text fill="#fff" fontSize={17} x={20} y={285}>
        BAML
      </text>
      <text fill="var(--v2-dim)" fontSize={13} textAnchor="end" x={270} y={285}>
        typed records
      </text>
      {FIELDS.map(([name, value], i) => (
        <g key={name}>
          <line
            stroke="var(--v2-line)"
            x1={20}
            x2={270}
            y1={FIELD_Y[i] - 19}
            y2={FIELD_Y[i] - 19}
          />
          <text fill="var(--v2-dim)" fontSize={13} x={20} y={FIELD_Y[i] + 4.5}>
            {name}
          </text>
          <text
            className="font-mono"
            fill="#fff"
            fontSize={12.5}
            textAnchor="end"
            x={270}
            y={FIELD_Y[i] + 4.5}
          >
            {value}
          </text>
        </g>
      ))}

      <path className="v2-wire" d={OTEL_PATH} pathLength={1000} />
      <path className="v2-pulse" d={OTEL_PATH} pathLength={1000} />
      {FIELDS.map(([name], i) => (
        <g key={name}>
          <path
            className="v2-wire v2-wire-baml"
            d={inPath(i)}
            pathLength={1000}
          />
          <path
            className="v2-pulse v2-pulse-baml"
            d={inPath(i)}
            pathLength={1000}
            style={delay(-0.8 * i - 0.4)}
          />
          <circle
            cx={SRC_RIGHT}
            cy={FIELD_Y[i]}
            fill="var(--v2-baml)"
            r={2.5}
          />
        </g>
      ))}
      <circle cx={SRC_RIGHT} cy={140} fill="var(--v2-signal)" r={2.5} />
    </g>
  );
});

function AlertPicture({ on }: { on: boolean }) {
  return (
    <g>
      <path
        className={on ? 'v2-draw' : ''}
        d="M16 76H96L112 72L128 76H160L176 40L188 66L200 34L216 62L232 70H274"
        fill="none"
        pathLength={1000}
        stroke={ERROR}
        strokeWidth={1.5}
      />
      <text
        className="font-mono"
        fill="var(--v2-dim)"
        fontSize={11.5}
        x={16}
        y={98}
      >
        ExtractInvoice is failing
      </text>
    </g>
  );
}

function BarsPicture({ on }: { on: boolean }) {
  const heights = [
    18, 26, 22, 34, 30, 42, 28, 38, 50, 44, 36, 48, 56, 40, 52, 46, 58, 44, 50,
    54,
  ];
  return (
    <g>
      {heights.map((h, i) => (
        <rect
          className={on ? 'v2-rise' : ''}
          fill="var(--color-pasture-200)"
          fillOpacity={0.3 + (i / heights.length) * 0.6}
          height={h}
          // biome-ignore lint/suspicious/noArrayIndexKey: the bars never reorder
          key={i}
          style={{ ...delay(i * 0.025), transformBox: 'fill-box' }}
          width={9}
          x={16 + i * 13}
          y={100 - h}
        />
      ))}
    </g>
  );
}

function EvalPicture({ on }: { on: boolean }) {
  const failed = new Set([5, 14, 23, 31, 38, 47]);
  return (
    <g>
      {Array.from({ length: 54 }, (_, i) => (
        <rect
          className={on ? 'v2-pop' : ''}
          fill={failed.has(i) ? ERROR : 'var(--color-pasture-300)'}
          fillOpacity={failed.has(i) ? 1 : 0.32}
          height={9}
          // biome-ignore lint/suspicious/noArrayIndexKey: the cells never reorder
          key={i}
          rx={2}
          style={delay(i * 0.012)}
          width={9}
          x={16 + (i % 18) * 14.5}
          y={46 + Math.floor(i / 18) * 18}
        />
      ))}
    </g>
  );
}

function OwnPicture() {
  return (
    <g>
      <text
        className="font-mono"
        fill="var(--v2-dim)"
        fontSize={12.5}
        x={16}
        y={72}
      >
        anything your team needs
      </text>
      <rect
        className="v2-blink"
        fill="var(--color-pasture-200)"
        height={15}
        width={8}
        x={204}
        y={60}
      />
    </g>
  );
}

function picture(key: string, on: boolean) {
  if (key === 'alerts') return <AlertPicture on={on} />;
  if (key === 'dashboards') return <BarsPicture on={on} />;
  if (key === 'evals') return <EvalPicture on={on} />;
  return <OwnPicture />;
}

/** One tool's card, in a 290×112 frame so the wide and stacked diagrams can share it. */
function ToolCard({
  on,
  tool,
  x = 0,
  y = 0,
}: {
  on: boolean;
  tool: Tool;
  x?: number;
  y?: number;
}) {
  return (
    <g
      className="transition-opacity duration-300"
      opacity={on ? 1 : 0.5}
      transform={`translate(${x} ${y})`}
    >
      <rect
        className="transition-[stroke] duration-300"
        fill="#000"
        height={TOOL_H - 1}
        rx={8}
        stroke={on ? 'var(--color-pasture-200)' : 'var(--v2-line)'}
        width={TOOL_W - 1}
        x={0.5}
        y={0.5}
      />
      <text fill="#fff" fontSize={15} x={16} y={27}>
        {tool.label}
      </text>
      {/* Remounting replays the picture each time this tool's query finishes. */}
      <g key={String(on)}>{picture(tool.key, on)}</g>
    </g>
  );
}

function Wide({
  active,
  chars,
  done,
  release,
  select,
}: Omit<ReturnType<typeof useFlow>, 'ref'>) {
  const tool = TOOLS[active];
  const typed = tool.query.slice(0, chars);
  const lines = typed.split('\n');
  return (
    <div className="relative hidden xl:block">
      <svg
        aria-hidden="true"
        className="w-full h-auto overflow-visible"
        viewBox="0 0 1280 570"
      >
        <defs>
          <pattern
            height={20}
            id="v2-dots"
            patternUnits="userSpaceOnUse"
            width={20}
          >
            <circle cx={1} cy={1} fill="rgb(255 255 255 / 0.13)" r={0.8} />
          </pattern>
          <radialGradient id="v2-dots-fade">
            <stop offset="0.25" stopColor="#fff" />
            <stop offset="1" stopColor="#000" />
          </radialGradient>
          <mask id="v2-dots-mask">
            <rect fill="url(#v2-dots-fade)" height={570} width={1280} />
          </mask>
          <radialGradient id="v2-glow">
            <stop
              offset="0"
              stopColor="var(--color-pasture-300)"
              stopOpacity={0.3}
            />
            <stop
              offset="1"
              stopColor="var(--color-pasture-300)"
              stopOpacity={0}
            />
          </radialGradient>
          <clipPath id="v2-chip-top">
            <path
              d={`M${CHIP.x} ${CHIP.y + 76}V${CHIP.y + 10}a10 10 0 0 1 10 -10H${CHIP_RIGHT - 10}a10 10 0 0 1 10 10V${CHIP.y + 76}Z`}
            />
          </clipPath>
        </defs>

        <rect
          fill="url(#v2-dots)"
          height={570}
          mask="url(#v2-dots-mask)"
          width={1280}
        />
        <ellipse cx={640} cy={280} fill="url(#v2-glow)" rx={390} ry={280} />

        <Sources />

        {/* The traces out. The chosen one lights up once its query has finished. */}
        {TOOLS.map((t, i) => (
          <path
            className="v2-wire"
            d={outPath(i)}
            key={t.key}
            pathLength={1000}
          />
        ))}
        {done && (
          <g key={tool.key}>
            <path
              className="v2-draw"
              d={outPath(active)}
              fill="none"
              pathLength={1000}
              stroke="var(--color-pasture-200)"
              strokeWidth={1.25}
            />
            <path
              className="v2-pulse v2-pulse-fast"
              d={outPath(active)}
              pathLength={1000}
            />
          </g>
        )}

        {/* The chip, with the painting on its lid and the query on its face. */}
        <rect
          fill="#050705"
          height={CHIP.h}
          rx={10}
          stroke="var(--color-pasture-300)"
          strokeOpacity={0.7}
          width={CHIP.w}
          x={CHIP.x}
          y={CHIP.y}
        />
        <image
          clipPath="url(#v2-chip-top)"
          height={CHIP.w * 0.62}
          href="/boundary/painting-hills.jpg"
          preserveAspectRatio="xMidYMid slice"
          width={CHIP.w}
          x={CHIP.x}
          y={CHIP.y - 60}
        />
        <rect
          clipPath="url(#v2-chip-top)"
          fill="rgb(0 0 0 / 0.25)"
          height={76}
          width={CHIP.w}
          x={CHIP.x}
          y={CHIP.y}
        />
        <image
          height={30}
          href={logoWhite.src}
          width={174}
          x={CHIP.x + (CHIP.w - 174) / 2}
          y={CHIP.y + 23}
        />
        <line
          stroke="var(--color-pasture-300)"
          strokeOpacity={0.7}
          x1={CHIP.x}
          x2={CHIP_RIGHT}
          y1={CHIP.y + 76}
          y2={CHIP.y + 76}
        />
        <text
          fill="var(--v2-dim)"
          fontSize={13}
          x={CHIP.x + 22}
          y={CHIP.y + 108}
        >
          SQL over every call
        </text>
        <text
          className="font-mono"
          fill="#D8DDD3"
          fontSize={12}
          xmlSpace="preserve"
        >
          {lines.map((line, i) => (
            <tspan
              // biome-ignore lint/suspicious/noArrayIndexKey: lines only ever append
              key={i}
              x={CHIP.x + 22}
              y={CHIP.y + 140 + i * 21}
            >
              {highlight(line)}
            </tspan>
          ))}
        </text>
        {!done && (
          <rect
            fill="var(--color-pasture-200)"
            height={14}
            width={7}
            x={CHIP.x + 22 + (lines.at(-1)?.length ?? 0) * 7.2 + 1}
            y={CHIP.y + 129 + (lines.length - 1) * 21}
          />
        )}
        <line
          stroke="var(--v2-line)"
          x1={CHIP.x + 22}
          x2={CHIP_RIGHT - 22}
          y1={CHIP.y + 250}
          y2={CHIP.y + 250}
        />
        <line
          stroke="var(--color-pasture-200)"
          strokeWidth={1.5}
          x1={CHIP.x + 22}
          x2={
            CHIP.x +
            22 +
            ((CHIP.w - 44) * chars) / Math.max(tool.query.length, 1)
          }
          y1={CHIP.y + 250}
          y2={CHIP.y + 250}
        />
        <circle
          cx={CHIP.x + 26}
          cy={CHIP.y + 282}
          fill={done ? 'var(--color-pasture-200)' : 'var(--v2-line)'}
          r={3.5}
        />
        <text fill="#fff" fontSize={14} x={CHIP.x + 40} y={CHIP.y + 287}>
          {tool.label}
        </text>

        {/* Pins down each side of the chip, lit where a trace lands. */}
        {[220, 245, 270, 300, 330, 360, 395].map((y) => (
          <circle
            cx={CHIP.x}
            cy={y}
            fill={y === 245 || y === 395 ? 'var(--v2-line)' : 'var(--v2-baml)'}
            key={y}
            r={2.5}
          />
        ))}
        {OUT_PINS.map((y, i) => (
          <circle
            cx={CHIP_RIGHT}
            cy={y}
            fill={i === active && done ? 'var(--color-pasture-200)' : '#3A4039'}
            key={y}
            r={2.5}
          />
        ))}

        {TOOLS.map((t, i) => (
          <ToolCard
            key={t.key}
            on={i === active && done}
            tool={t}
            x={TOOL_X}
            y={TOOL_Y[i]}
          />
        ))}
      </svg>

      {/* Real buttons over the drawn cards, so hover, keyboard, and touch all work. */}
      {TOOLS.map((t, i) => (
        <button
          aria-label={`Show the ${t.label} query`}
          aria-pressed={i === active}
          className="focus-ring absolute rounded-lg cursor-pointer"
          key={t.key}
          onBlur={release}
          onClick={() => select(i)}
          onFocus={() => select(i)}
          onMouseEnter={() => select(i)}
          onMouseLeave={release}
          style={{
            height: `${(TOOL_H / 570) * 100}%`,
            left: `${(TOOL_X / 1280) * 100}%`,
            top: `${(TOOL_Y[i] / 570) * 100}%`,
            width: `${(TOOL_W / 1280) * 100}%`,
          }}
          type="button"
        />
      ))}
    </div>
  );
}

function Drop() {
  return (
    <svg
      aria-hidden="true"
      className="block mx-auto w-3 h-10"
      viewBox="0 0 12 40"
    >
      <path className="v2-wire" d="M6 0V40" pathLength={1000} />
      <path className="v2-pulse" d="M6 0V40" pathLength={1000} />
    </svg>
  );
}

function Stacked({
  active,
  chars,
  done,
  select,
}: Omit<ReturnType<typeof useFlow>, 'ref' | 'release'>) {
  const tool = TOOLS[active];
  return (
    <div className="xl:hidden max-w-xl mx-auto">
      <div className="rounded-lg border border-white/12 p-4">
        <div className="flex justify-between text-[15px]">
          OTEL traces
          <span className="text-[13px] text-white/60">from any service</span>
        </div>
      </div>
      <div className="mt-2 rounded-lg border border-clover/55 p-4">
        <div className="flex justify-between text-[15px]">
          BAML
          <span className="text-[13px] text-white/60">typed records</span>
        </div>
        <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-1 text-[13px]">
          {FIELDS.map(([name, value]) => (
            <div className="flex justify-between gap-2 min-w-0" key={name}>
              <dt className="text-white/60 truncate">{name}</dt>
              <dd className="font-mono text-[11px] leading-5 truncate max-sm:hidden">
                {value}
              </dd>
            </div>
          ))}
        </dl>
      </div>
      <Drop />
      <div className="rounded-[10px] border border-pasture-300/70 overflow-hidden bg-[#050705]">
        <div className="relative py-5 bg-[url('/boundary/painting-hills.jpg')] bg-cover bg-center">
          <div className="absolute inset-0 bg-black/25" />
          {/* biome-ignore lint/performance/noImgElement: the brand wordmark is an SVG */}
          <img
            alt="Boundary"
            className="relative h-6 mx-auto"
            src={logoWhite.src}
          />
        </div>
        <div className="p-4">
          <div className="text-[13px] text-white/60">SQL over every call</div>
          {/* The height holds at four lines, so the page doesn't jump as a query types. */}
          <pre className="mt-3 min-h-[80px] overflow-x-auto font-mono text-[11px] sm:text-xs leading-5 text-[#D8DDD3]">
            {tool.query.slice(0, chars)}
          </pre>
        </div>
      </div>
      <Drop />
      <div className="grid grid-cols-2 gap-2">
        {TOOLS.map((t, i) => (
          <button
            aria-pressed={i === active}
            className={`focus-ring rounded-lg border px-3 py-2.5 text-left text-sm cursor-pointer transition-colors ${i === active ? 'border-pasture-200 text-white' : 'border-white/12 text-white/60'}`}
            key={t.key}
            onClick={() => select(i)}
            type="button"
          >
            {t.label}
          </button>
        ))}
      </div>
      <svg
        aria-hidden="true"
        className="mt-2 w-full h-auto"
        viewBox={`0 0 ${TOOL_W} ${TOOL_H}`}
      >
        <ToolCard on={done} tool={tool} />
      </svg>
    </div>
  );
}

export default function Flow() {
  const { ref, ...flow } = useFlow();
  return (
    <figure
      aria-label="OTEL traces and BAML records go into Boundary, where SQL over every call feeds alerts, dashboards, AI evals, and your own tooling."
      ref={ref}
    >
      <Wide {...flow} />
      <Stacked {...flow} />
    </figure>
  );
}
