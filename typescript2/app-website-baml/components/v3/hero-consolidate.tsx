'use client';

import { useEffect, useRef, useState } from 'react';

// A fragmented workflow above a unified one. The top row starts as thirteen separate tools strung
// together with long hand-offs. Every so often one tool that the toolchain makes unnecessary fades out,
// and its neighbors close the gap, until the row matches the five steps below it. The final arrangement
// holds, then the row resets. The tools are kinds of tools rather than products.
type Glyph =
  | 'bell'
  | 'braces'
  | 'chart'
  | 'check'
  | 'cycle'
  | 'editor'
  | 'flag'
  | 'grid'
  | 'list'
  | 'play'
  | 'stack'
  | 'up'
  | 'waterfall';

const TOOLS: { glyph: Glyph; label: string }[] = [
  { glyph: 'editor', label: 'Editor' },
  { glyph: 'stack', label: 'Prompt store' },
  { glyph: 'braces', label: 'Schema library' },
  { glyph: 'check', label: 'Type checker' },
  { glyph: 'play', label: 'Test runner' },
  { glyph: 'grid', label: 'Eval sheet' },
  { glyph: 'cycle', label: 'CI' },
  { glyph: 'up', label: 'Deploy scripts' },
  { glyph: 'flag', label: 'Feature flags' },
  { glyph: 'waterfall', label: 'Tracing' },
  { glyph: 'list', label: 'Log search' },
  { glyph: 'chart', label: 'Metrics' },
  { glyph: 'bell', label: 'Alerts' },
];
// The tools that go, in the order they go.
const ORDER = [2, 10, 5, 1, 12, 6, 11, 8];
const STEPS: { command: string; glyph: Glyph; label: string }[] = [
  { command: '.baml', glyph: 'editor', label: 'Write' },
  { command: 'baml check', glyph: 'check', label: 'Check' },
  { command: 'baml test', glyph: 'play', label: 'Test' },
  { command: 'baml pack', glyph: 'up', label: 'Ship' },
  { command: 'baml query', glyph: 'waterfall', label: 'Observe' },
];

const HOLD_MS = 2400;
const EVERY_MS = 1500;
const FADE_MS = 550;
const END = HOLD_MS + ORDER.length * EVERY_MS + 4200;
const LAP = END + 1600;

type Box = { o: number; w: number; x: number; y: number };

/** Where every tool should be at time t, and which tools are still in the row. */
function layout(t: number) {
  const fading =
    t < END ? ORDER.filter((_, i) => t >= HOLD_MS + i * EVERY_MS) : [];
  const gone =
    t < END
      ? ORDER.filter((_, i) => t >= HOLD_MS + i * EVERY_MS + FADE_MS)
      : [];
  const row = TOOLS.map((_, i) => i).filter((i) => !gone.includes(i));
  const k = row.length;
  // The row zigzags while it's long, and straightens as it shortens.
  const amp = (30 * (k - STEPS.length)) / (TOOLS.length - STEPS.length);
  const boxes: Record<number, Box> = {};
  row.forEach((tool, j) => {
    boxes[tool] = {
      o: fading.includes(tool) ? 0 : 1,
      w: (100 / k) * 0.72,
      x: ((j + 0.5) / k) * 100,
      y: 50 + (j % 2 ? 1 : -1) * amp * [1, 0.65, 0.85][j % 3],
    };
  });
  return { boxes, row };
}

function Icon({ glyph }: { glyph: Glyph }) {
  const paths: Record<Glyph, React.ReactNode> = {
    bell: <path d="M5 13V8a4 4 0 0 1 8 0v5l1.5 1.5h-11L5 13ZM7.5 16h3" />,
    braces: (
      <path d="M7 3H6a2 2 0 0 0-2 2v2.5L2.5 9 4 10.5V13a2 2 0 0 0 2 2h1M11 3h1a2 2 0 0 1 2 2v2.5L15.5 9 14 10.5V13a2 2 0 0 1-2 2h-1" />
    ),
    chart: <path d="M2.5 15.5h13M3.5 12l3.5-4 3 2.5 4.5-6" />,
    check: (
      <>
        <rect height="12" rx="2" width="12" x="3" y="3" />
        <path d="m6 9 2 2 4-4" />
      </>
    ),
    cycle: (
      <path d="M14.5 7.5A6 6 0 0 0 4 5.5M3.5 10.5A6 6 0 0 0 14 12.5M4 2.5v3h3M14 15.5v-3h-3" />
    ),
    editor: (
      <>
        <rect height="12" rx="2" width="14" x="2" y="3" />
        <path d="M5 7h5M5 9.5h7M5 12h3" />
      </>
    ),
    flag: <path d="M4.5 16V3M4.5 3.5h9l-2 3 2 3h-9" />,
    grid: (
      <>
        <rect height="12" rx="1.5" width="14" x="2" y="3" />
        <path d="M2 7h14M2 11h14M7 3v12M11.5 3v12" />
      </>
    ),
    list: <path d="M3 4.5h1M6 4.5h9M3 9h1M6 9h9M3 13.5h1M6 13.5h6" />,
    play: <path d="M5.5 3.5v11l9-5.5-9-5.5Z" />,
    stack: (
      <path d="M9 2.5 2.5 6 9 9.5 15.5 6 9 2.5ZM2.5 9 9 12.5 15.5 9M2.5 12 9 15.5 15.5 12" />
    ),
    up: <path d="M9 13V3.5M5 7.5l4-4 4 4M3 15.5h12" />,
    waterfall: <path d="M2.5 4h6M5 7.5h7M7.5 11h5M10 14.5h5.5" />,
  };
  return (
    <svg
      aria-hidden="true"
      className="size-4 md:size-[18px] shrink-0"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={1.3}
      viewBox="0 0 18 18"
    >
      {paths[glyph]}
    </svg>
  );
}

export default function HeroConsolidate() {
  const [frame, setFrame] = useState(() => {
    const { boxes, row } = layout(0);
    return { boxes, row };
  });
  const current = useRef<Record<number, Box>>(layout(0).boxes);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const begin = performance.now();
    let last = begin;
    let id = 0;
    const tick = (now: number) => {
      const dt = Math.min(now - last, 50);
      last = now;
      const { boxes, row } = layout(Math.max(0, now - begin) % LAP);
      // Each tool eases toward its slot; one that has left the row keeps its place and stays hidden.
      const move = 1 - Math.exp(-dt / 260);
      const fade = 1 - Math.exp(-dt / 160);
      const next: Record<number, Box> = {};
      TOOLS.forEach((_, i) => {
        const from = current.current[i] ?? boxes[i];
        const to = boxes[i] ?? { ...from, o: 0 };
        next[i] = {
          o: from.o + (to.o - from.o) * fade,
          w: from.w + (to.w - from.w) * move,
          x: from.x + (to.x - from.x) * move,
          y: from.y + (to.y - from.y) * move,
        };
      });
      current.current = next;
      setFrame({ boxes: next, row });
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(id);
  }, []);

  const { boxes, row } = frame;
  const k = row.length;

  return (
    <figure
      aria-label="Above, a workflow of thirteen separate tools strung together with long hand-offs. One by one, the tools BAML makes unnecessary fade out and their neighbors close the gap, until five remain. Below, the same workflow in BAML: write, check, test, ship, and observe."
      className="relative pb-16"
    >
      <div className="flex items-baseline justify-between gap-4 pb-3 border-b border-white/10">
        <span className="text-[14px] md:text-[15px] text-white/85">
          Separate tools
        </span>
        <span className="font-mono text-[12px] md:text-[13px] text-white/45 tabular-nums">
          {k} tools, {k - 1} hand-offs
        </span>
      </div>

      <div className="relative h-[200px] md:h-[280px]">
        <svg
          aria-hidden="true"
          className="absolute inset-0 size-full"
          preserveAspectRatio="none"
          viewBox="0 0 100 100"
        >
          <polyline
            fill="none"
            points={row.map((i) => `${boxes[i].x},${boxes[i].y}`).join(' ')}
            stroke="rgb(255 255 255 / 0.2)"
            strokeWidth={1}
            vectorEffect="non-scaling-stroke"
          />
        </svg>
        {TOOLS.map((tool, i) => {
          const b = boxes[i];
          return (
            <div
              className="absolute flex flex-col items-center justify-center gap-1.5 h-[34px] md:h-16 px-1 -translate-x-1/2 -translate-y-1/2 rounded-[5px] border border-white/15 bg-black text-white/70"
              key={tool.label}
              style={{
                left: `${b.x}%`,
                opacity: b.o,
                top: `${b.y}%`,
                width: `max(24px, ${b.w}%)`,
              }}
              title={tool.label}
            >
              <Icon glyph={tool.glyph} />
              <span className="hidden md:block max-w-full truncate text-[11.5px] leading-none text-white/55">
                {tool.label}
              </span>
            </div>
          );
        })}
      </div>

      <div className="flex items-baseline justify-between gap-4 pt-6 pb-3 border-b border-white/10">
        <span className="font-mono text-[13px] md:text-[14px] text-clover">
          BAML
        </span>
        <span className="font-mono text-[12px] md:text-[13px] text-white/45">
          5 steps, one toolchain
        </span>
      </div>

      <div className="relative h-[110px] md:h-[130px]">
        <span
          aria-hidden
          className="absolute top-1/2 h-px bg-white/20"
          style={{ left: '10%', right: '10%' }}
        />
        {STEPS.map((step, j) => (
          <div
            className="absolute flex flex-col items-center justify-center gap-1 h-[52px] md:h-16 px-1 -translate-x-1/2 -translate-y-1/2 rounded-[5px] border border-clover/40 bg-black"
            key={step.label}
            style={{
              left: `${((j + 0.5) / STEPS.length) * 100}%`,
              top: '50%',
              width: `max(56px, ${(100 / STEPS.length) * 0.72}%)`,
            }}
          >
            <span className="flex items-center gap-1.5 text-[12px] md:text-[13px] text-white">
              <span className="hidden md:inline text-clover">
                <Icon glyph={step.glyph} />
              </span>
              {step.label}
            </span>
            <span className="hidden sm:block max-w-full truncate font-mono text-[11px] md:text-[12px] text-white/45">
              {step.command}
            </span>
          </div>
        ))}
      </div>
    </figure>
  );
}
