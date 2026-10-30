'use client';

import { type ReactNode, useEffect, useRef, useState } from 'react';
import { track } from '../track';
import Seen from './seen';

// The hero's sentence, running. The program executes a line at a time, each execution lands in the
// record as one pixel, a query types itself and picks out the pixels it reads, and the result hops
// down one wire to what it powers. The queries use the tables baml query reads; the pixels are illustrative.
type Output = {
  key: string;
  label: string;
  matches: (i: number) => boolean;
  query: string;
};

const ERROR = '#E5484D';
const COLS = 16;
const ROWS = 9;
const FAILED = new Set([2, 27, 45, 52, 70, 85, 101, 118, 139]);

const OUTPUTS: Output[] = [
  {
    key: 'evals',
    label: 'Evals',
    matches: (i) => Math.floor(i / COLS) % 3 === 1,
    query: `SELECT input_args, output_value
FROM spans
WHERE span_name = 'user.classify'`,
  },
  {
    key: 'cost_alerts',
    label: 'Cost alerts',
    matches: (i) => (i * 7) % 5 === 0,
    query: `SELECT span_name,
  sum(total_usage[0]['input_tokens'])
    AS tokens
FROM spans GROUP BY span_name`,
  },
  {
    key: 'error_tracking',
    label: 'Error tracking',
    matches: (i) => FAILED.has(i),
    query: `SELECT span_name, count(*) AS errors
FROM spans
WHERE status = 'user_error'
GROUP BY span_name`,
  },
];

const TYPE_MS = 22;
const HOLD_MS = 3400;

// Each stage is drawn in a 280×330 frame, so the wide row and the stacked column share the drawing.
const STAGE_W = 280;
const STAGE_H = 330;
const GAP = (1280 - STAGE_W * 4) / 3;
const stageX = (i: number) => i * (STAGE_W + GAP);
const WIRE_Y = 150;
const ROW_Y = [62, 108, 154, 200];
const ROW_H = 38;

/** A path through right-angle points, with square corners like the pixels it carries. */
const route = (points: [number, number][]) =>
  points.map(([x, y], i) => `${i ? 'L' : 'M'}${x} ${y}`).join(' ');

const fanPath = (row: number) => {
  const left = stageX(3);
  const lane = left - GAP / 2;
  const y = ROW_Y[row] + ROW_H / 2;
  return route([
    [left - GAP, WIRE_Y],
    [lane, WIRE_Y],
    [lane, y],
    [left, y],
  ]);
};

function highlight(line: string): ReactNode[] {
  return line
    .split(/(\b(?:SELECT|FROM|WHERE|GROUP BY|AS)\b|'[^']*'?)/)
    .map((part, i) => {
      const key = `${i}-${part}`;
      if (part.startsWith("'"))
        return (
          <tspan fill="var(--color-pasture-200)" key={key}>
            {part}
          </tspan>
        );
      return /^(SELECT|FROM|WHERE|GROUP BY|AS)$/.test(part) ? (
        <tspan fill="var(--v2-signal)" key={key}>
          {part}
        </tspan>
      ) : (
        <tspan key={key}>{part}</tspan>
      );
    });
}

/** Cycles through the outputs while on screen, typing each query. Hovering or tapping holds one. */
function useFlow() {
  const [active, setActive] = useState(0);
  const [chars, setChars] = useState(OUTPUTS[0].query.length);
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

  const length = OUTPUTS[active].query.length;

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new output restarts the typing
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
      () => setActive((a) => (a + 1) % OUTPUTS.length),
      HOLD_MS,
    );
    return () => clearTimeout(id);
  }, [done, held, still, visible]);

  const select = (i: number) => {
    if (i !== active) {
      track('flow_output_selected', { output: OUTPUTS[i].key });
      setActive(i);
    }
    setHeld(true);
  };

  return { active, chars, done, ref, release: () => setHeld(false), select };
}

type FlowState = Omit<ReturnType<typeof useFlow>, 'ref'>;

function Frame({
  children,
  lines,
  mono,
  title,
}: {
  children: ReactNode;
  lines: string[];
  mono?: boolean;
  title: string;
}) {
  return (
    <g>
      <rect
        fill="#000"
        height={STAGE_H - 1}
        rx={3}
        stroke="var(--v2-line)"
        width={STAGE_W - 1}
        x={0.5}
        y={0.5}
      />
      <text
        className={mono ? 'font-mono' : 'v2-baml-mark'}
        fill="#fff"
        fontSize={mono ? 18 : 21}
        x={18}
        y={37}
      >
        {title}
      </text>
      {children}
      <line
        stroke="var(--v2-line)"
        x1={18}
        x2={STAGE_W - 18}
        y1={258}
        y2={258}
      />
      {lines.map((l, i) => (
        <text
          fill="var(--v2-dim)"
          fontSize={14}
          key={l}
          x={18}
          y={284 + i * 21}
        >
          {l}
        </text>
      ))}
    </g>
  );
}

function Program() {
  const code: ReactNode[] = [
    <>
      <tspan fill="var(--v2-signal)">function</tspan> classify(t: string)
    </>,
    <>
      {'    -> '}
      <tspan fill="var(--color-pasture-200)">Verdict</tspan>
      {' {'}
    </>,
    <>
      {'  client: '}
      <tspan fill="var(--color-pasture-200)">"openai/gpt-4o"</tspan>
    </>,
    <>
      {'  prompt: '}
      <tspan fill="var(--color-pasture-200)">`Classify ...`</tspan>
    </>,
    '}',
  ];
  return (
    <Frame
      lines={['Agents and AI functions,', 'written in BAML.']}
      title="Your program"
    >
      <rect
        className="v2-exec"
        fill="var(--v2-signal)"
        fillOpacity={0.22}
        height={24}
        width={STAGE_W - 2}
        x={1}
        y={82}
      />
      <text
        className="font-mono"
        fill="#D8DDD3"
        fontSize={12}
        xmlSpace="preserve"
      >
        {code.map((line, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: the lines never reorder
          <tspan key={i} x={18} y={98 + i * 24}>
            {line}
          </tspan>
        ))}
      </text>
    </Frame>
  );
}

function Record({ active, done }: { active: number; done: boolean }) {
  const output = OUTPUTS[active];
  return (
    <Frame
      lines={['Every execution,', 'by the runtime itself.']}
      title="Recorded"
    >
      {Array.from({ length: COLS * ROWS }, (_, i) => {
        const hit = done && output.matches(i);
        const failed = FAILED.has(i);
        return (
          <rect
            className="v2-pix"
            fill={
              hit && failed && output.key === 'error_tracking'
                ? ERROR
                : hit
                  ? 'var(--v2-signal)'
                  : 'rgb(255 255 255)'
            }
            fillOpacity={hit ? 1 : done ? 0.1 : 0.2}
            height={12}
            // biome-ignore lint/suspicious/noArrayIndexKey: one pixel per fixed position
            key={i}
            style={{ '--i': i } as React.CSSProperties}
            width={12}
            x={21 + (i % COLS) * 15}
            y={70 + Math.floor(i / COLS) * 15 + 15}
          />
        );
      })}
    </Frame>
  );
}

function Query({ active, chars, done }: FlowState) {
  const query = OUTPUTS[active].query;
  const lines = query.slice(0, chars).split('\n');
  return (
    <Frame lines={['SQL, locally', 'or in the cloud.']} mono title="baml query">
      <text
        className="font-mono"
        fill="#D8DDD3"
        fontSize={11}
        xmlSpace="preserve"
      >
        {lines.map((line, i) => (
          // biome-ignore lint/suspicious/noArrayIndexKey: lines only ever append
          <tspan key={i} x={18} y={88 + i * 22}>
            {highlight(line)}
          </tspan>
        ))}
      </text>
      {!done && (
        <rect
          fill="var(--v2-signal)"
          height={13}
          width={6.6}
          x={18 + (lines.at(-1)?.length ?? 0) * 6.6 + 1}
          y={77 + (lines.length - 1) * 22}
        />
      )}
      <line
        stroke="var(--v2-line)"
        x1={18}
        x2={STAGE_W - 18}
        y1={226}
        y2={226}
      />
      <line
        stroke="var(--v2-signal)"
        strokeWidth={2}
        x1={18}
        x2={18 + ((STAGE_W - 36) * chars) / query.length}
        y1={226}
        y2={226}
      />
    </Frame>
  );
}

function Anything({ active, done }: { active: number; done: boolean }) {
  return (
    <Frame
      lines={['Evals, cost alerts,', 'error tracking, and more.']}
      title="Anything"
    >
      {[...OUTPUTS.map((o) => o.label), 'and more'].map((label, i) => {
        const on = done && i === active;
        return (
          <g key={label}>
            <rect
              className="transition-[stroke] duration-200"
              fill={on ? 'rgb(255 255 255 / 0.05)' : 'none'}
              height={ROW_H}
              rx={2}
              stroke={on ? 'var(--v2-signal)' : 'var(--v2-line)'}
              width={STAGE_W - 36}
              x={18}
              y={ROW_Y[i]}
            />
            <text
              fill={on ? '#fff' : 'var(--v2-dim)'}
              fontSize={14}
              x={32}
              y={ROW_Y[i] + 24}
            >
              {label}
            </text>
            {i < OUTPUTS.length &&
              [0, 1, 2].map((p) => (
                <rect
                  className={on ? 'v2-lamp' : ''}
                  fill={on ? 'var(--v2-signal)' : 'rgb(255 255 255 / 0.14)'}
                  height={7}
                  key={p}
                  style={{ '--i': p } as React.CSSProperties}
                  width={7}
                  x={STAGE_W - 58 + p * 10}
                  y={ROW_Y[i] + 15.5}
                />
              ))}
          </g>
        );
      })}
    </Frame>
  );
}

/** Real buttons over the drawn rows, so hover, keyboard, and touch all work. */
function RowButtons({
  height,
  release,
  select,
  width,
  x,
}: Pick<FlowState, 'release' | 'select'> & {
  height: number;
  width: number;
  x: number;
}) {
  return OUTPUTS.map((o, i) => (
    <button
      aria-label={`Show the ${o.label} query`}
      className="focus-ring absolute cursor-pointer"
      key={o.key}
      onBlur={release}
      onClick={() => select(i)}
      onFocus={() => select(i)}
      onMouseEnter={() => select(i)}
      onMouseLeave={release}
      style={{
        height: `${(ROW_H / height) * 100}%`,
        left: `${((x + 18) / width) * 100}%`,
        top: `${(ROW_Y[i] / height) * 100}%`,
        width: `${((STAGE_W - 36) / width) * 100}%`,
      }}
      type="button"
    />
  ));
}

function Wide(flow: FlowState) {
  const { active, done, release, select } = flow;
  const stages = [
    <Program key="program" />,
    <Record active={active} done={done} key="record" />,
    <Query {...flow} key="query" />,
    <Anything active={active} done={done} key="anything" />,
  ];
  const links = [0, 1].map((i) =>
    route([
      [stageX(i) + STAGE_W, WIRE_Y],
      [stageX(i + 1), WIRE_Y],
    ]),
  );
  return (
    <div className="relative hidden xl:block">
      <svg
        aria-hidden="true"
        className="w-full h-auto overflow-visible"
        viewBox={`0 0 1280 ${STAGE_H}`}
      >
        {links.map((d) => (
          <g key={d}>
            <path className="v2-wire" d={d} />
            <path className="v2-march" d={d} />
          </g>
        ))}
        {OUTPUTS.map((o, i) => (
          <path className="v2-wire" d={fanPath(i)} key={o.key} />
        ))}
        {done && <path className="v2-march" d={fanPath(active)} />}
        {stages.map((stage, i) => (
          <g key={stage.key} transform={`translate(${stageX(i)} 0)`}>
            {stage}
          </g>
        ))}
      </svg>
      <RowButtons
        height={STAGE_H}
        release={release}
        select={select}
        width={1280}
        x={stageX(3)}
      />
    </div>
  );
}

function Stacked(flow: FlowState) {
  const { active, done, release, select } = flow;
  const box = `0 0 ${STAGE_W} ${STAGE_H}`;
  const stage = (node: ReactNode) => (
    <svg aria-hidden="true" className="w-full h-auto" viewBox={box}>
      {node}
    </svg>
  );
  return (
    <div className="xl:hidden grid md:grid-cols-2 gap-x-4 max-md:gap-y-0 md:gap-y-4">
      {stage(<Program />)}
      <Hop />
      {stage(<Record active={active} done={done} />)}
      <Hop />
      {stage(<Query {...flow} />)}
      <Hop />
      <div className="relative">
        {stage(<Anything active={active} done={done} />)}
        <RowButtons
          height={STAGE_H}
          release={release}
          select={select}
          width={STAGE_W}
          x={0}
        />
      </div>
    </div>
  );
}

/** The wire between stages when they stack in one column. */
function Hop() {
  return (
    <svg
      aria-hidden="true"
      className="md:hidden block mx-auto w-3 h-8"
      viewBox="0 0 12 32"
    >
      <path className="v2-wire" d="M6 0V32" />
      <path className="v2-march" d="M6 0V32" />
    </svg>
  );
}

export default function Flow() {
  const { ref, ...flow } = useFlow();
  return (
    <figure
      aria-label="Your program is recorded by the runtime, baml query reads the recording, and you build anything on the result: evals, cost alerts, error tracking, and more."
      ref={ref}
    >
      <Seen>
        <Wide {...flow} />
        <Stacked {...flow} />
      </Seen>
    </figure>
  );
}
