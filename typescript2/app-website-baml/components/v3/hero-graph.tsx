'use client';

import { type PointerEvent, useEffect, useRef, useState } from 'react';

// The same nine capabilities drawn twice as a sparse graph. On the left they're separate tools, joined
// by hand where they're joined at all; some links are dashed and some are broken. On the right they sit
// around one runtime, each with a single clean link to it. Nodes drift by a pixel or two, links brighten
// near the pointer, and now and then a data point travels along a link. On the left it can stop at a
// break. The tools are kinds of tools rather than products.
type Node = { id: string; x: number; y: number };
type Edge = { a: string; b: string; kind: 'solid' | 'dashed' | 'broken' };

const TOOLS = [
  'TypeScript',
  'Prompt files',
  'CI',
  'Evals',
  'Feature flags',
  'Deployment',
  'Tracing',
  'Metrics',
  'Profiling',
];

// Positions are percentages of the graph's box.
const SCATTERED: Node[] = [
  { id: 'TypeScript', x: 18, y: 20 },
  { id: 'Prompt files', x: 48, y: 11 },
  { id: 'CI', x: 78, y: 22 },
  { id: 'Evals', x: 27, y: 50 },
  { id: 'Feature flags', x: 58, y: 44 },
  { id: 'Deployment', x: 82, y: 57 },
  { id: 'Tracing', x: 17, y: 83 },
  { id: 'Metrics', x: 47, y: 87 },
  { id: 'Profiling', x: 79, y: 84 },
];
const WIRED: Edge[] = [
  { a: 'TypeScript', b: 'Prompt files', kind: 'solid' },
  { a: 'Prompt files', b: 'CI', kind: 'dashed' },
  { a: 'TypeScript', b: 'Evals', kind: 'broken' },
  { a: 'Prompt files', b: 'Feature flags', kind: 'dashed' },
  { a: 'CI', b: 'Deployment', kind: 'solid' },
  { a: 'Evals', b: 'Feature flags', kind: 'broken' },
  { a: 'Feature flags', b: 'Deployment', kind: 'dashed' },
  { a: 'Deployment', b: 'Profiling', kind: 'broken' },
  { a: 'Tracing', b: 'Metrics', kind: 'solid' },
  { a: 'Evals', b: 'Tracing', kind: 'dashed' },
  { a: 'Metrics', b: 'Profiling', kind: 'dashed' },
];

const HUB = 'BAML runtime';
const AROUND: Node[] = [
  { id: HUB, x: 50, y: 50 },
  ...TOOLS.map((id, i) => {
    const angle = -Math.PI / 2 + (i * 2 * Math.PI) / TOOLS.length;
    return { id, x: 50 + 35 * Math.cos(angle), y: 50 + 36 * Math.sin(angle) };
  }),
];
const SPOKES: Edge[] = TOOLS.map((id) => ({ a: HUB, b: id, kind: 'solid' }));

// Where a broken link breaks, as fractions of its length.
const BREAK = [0.42, 0.58];
const TRAVEL_MS = 1500;
const NEAR_PX = 90;

const clamp = (n: number) => Math.min(1, Math.max(0, n));
const ease = (t: number) => (t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2);

/** A repeatable pseudo-random choice, so each packet's link depends only on its number. */
function pick(n: number, length: number) {
  const r = Math.sin(n * 12.9898 + length) * 43758.5453;
  return Math.floor((r - Math.floor(r)) * length);
}

/** A slow drift of a fraction of a percent, different for every node. */
function drift(i: number, t: number) {
  return {
    dx: 0.28 * Math.sin(t / 2700 + i * 1.7),
    dy: 0.34 * Math.cos(t / 3300 + i * 2.3),
  };
}

function toSegment(
  p: { x: number; y: number },
  a: { x: number; y: number },
  b: { x: number; y: number },
) {
  const vx = b.x - a.x;
  const vy = b.y - a.y;
  const u = clamp(((p.x - a.x) * vx + (p.y - a.y) * vy) / (vx * vx + vy * vy));
  return Math.hypot(p.x - (a.x + u * vx), p.y - (a.y + u * vy));
}

function Graph({
  edges,
  hub,
  nodes,
  note,
  period,
  still,
  t,
  title,
}: {
  edges: Edge[];
  hub?: string;
  nodes: Node[];
  note: string;
  period: number;
  still: boolean;
  t: number;
  title: string;
}) {
  const [pointer, setPointer] = useState<{
    h: number;
    w: number;
    x: number;
    y: number;
  }>();

  const at = (id: string) => {
    const i = nodes.findIndex((n) => n.id === id);
    const { dx, dy } = drift(i, t);
    return { x: nodes[i].x + dx, y: nodes[i].y + dy };
  };
  const along = (e: Edge, f: number) => {
    const a = at(e.a);
    const b = at(e.b);
    return { x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f };
  };

  // Near the pointer, a link brightens: 1 on top of it, 0 at NEAR_PX away or more.
  const nearness = (e: Edge) => {
    if (!pointer) return 0;
    const px = (p: { x: number; y: number }) => ({
      x: (p.x / 100) * pointer.w,
      y: (p.y / 100) * pointer.h,
    });
    return clamp(1 - toSegment(pointer, px(at(e.a)), px(at(e.b))) / NEAR_PX);
  };

  // The packet in flight: around the hub it goes out along a spoke and comes back, and between separate
  // tools it crosses one link, or stops at the break and fades.
  const n = Math.floor(t / period);
  const edge = edges[pick(n, edges.length)];
  const u = (t - n * period) / TRAVEL_MS;
  let packet: { o: number; x: number; y: number } | undefined;
  let reached: string | undefined;
  if (!still && edge) {
    if (hub) {
      if (u < 2) {
        packet = { o: 1, ...along(edge, u < 1 ? ease(u) : 1 - ease(u - 1)) };
        if (u > 0.95 && u < 1.25) reached = edge.b;
      }
    } else if (edge.kind === 'broken') {
      if (u < 1.4) {
        packet = {
          o: clamp(1 - (u - 0.6) / 0.8),
          ...along(edge, Math.min(ease(clamp(u)), BREAK[0])),
        };
      }
    } else if (u < 1) {
      packet = { o: 1, ...along(edge, ease(u)) };
    } else if (u < 1.3) {
      reached = edge.b;
    }
  }

  const onMove = (e: PointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    setPointer({
      h: box.height,
      w: box.width,
      x: e.clientX - box.left,
      y: e.clientY - box.top,
    });
  };

  return (
    <div className="min-w-0 rounded-[6px] border border-white/10">
      <div className="flex items-baseline justify-between gap-4 px-4 h-11 pt-3 border-b border-white/10">
        <span className="text-[14px] md:text-[15px] text-white/85">
          {title}
        </span>
        <span className="font-mono text-[11.5px] md:text-[12.5px] text-white/40 truncate">
          {note}
        </span>
      </div>
      <div
        className="relative aspect-[6/5] touch-pan-y"
        onPointerLeave={() => setPointer(undefined)}
        onPointerMove={onMove}
      >
        <svg
          aria-hidden="true"
          className="absolute inset-0 size-full"
          preserveAspectRatio="none"
          viewBox="0 0 100 100"
        >
          {edges.map((e) => {
            const glow = nearness(e);
            const stroke = hub
              ? `rgb(from var(--color-clover-core) r g b / ${0.22 + 0.6 * glow})`
              : `rgb(255 255 255 / ${0.16 + 0.5 * glow})`;
            const line = (from: number, to: number) => {
              const a = along(e, from);
              const b = along(e, to);
              return (
                <line
                  key={`${from}`}
                  stroke={stroke}
                  strokeDasharray={e.kind === 'dashed' ? '4 5' : undefined}
                  strokeWidth={1}
                  vectorEffect="non-scaling-stroke"
                  x1={a.x}
                  x2={b.x}
                  y1={a.y}
                  y2={b.y}
                />
              );
            };
            return (
              <g key={`${e.a}-${e.b}`}>
                {e.kind === 'broken'
                  ? [line(0, BREAK[0]), line(BREAK[1], 1)]
                  : line(0, 1)}
              </g>
            );
          })}
        </svg>

        {packet && (
          <span
            aria-hidden
            className={`absolute size-[5px] -ml-[2.5px] -mt-[2.5px] rounded-full ${hub ? 'bg-clover' : 'bg-white'}`}
            style={{
              left: `${packet.x}%`,
              opacity: packet.o,
              top: `${packet.y}%`,
            }}
          />
        )}

        {nodes.map((node) => {
          const { x, y } = at(node.id);
          const center = node.id === hub;
          const lit = node.id === reached;
          return (
            <span
              className={`absolute -translate-x-1/2 -translate-y-1/2 rounded-[4px] border bg-black whitespace-nowrap transition-colors duration-300 ${center ? 'px-2.5 md:px-3 py-1 md:py-1.5 text-[12px] md:text-[14px] border-clover/60 text-white' : `px-1.5 md:px-2 py-[3px] font-mono text-[10.5px] md:text-[12.5px] ${lit ? (hub ? 'border-clover text-white' : 'border-white/50 text-white') : 'border-white/15 text-white/65'}`}`}
              key={node.id}
              style={{ left: `${x}%`, top: `${y}%` }}
            >
              {node.id}
            </span>
          );
        })}
      </div>
    </div>
  );
}

export default function HeroGraph() {
  const [t, setT] = useState(0);
  const [still, setStill] = useState(false);
  const begin = useRef(0);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setStill(true);
      return;
    }
    begin.current = performance.now();
    let frame = 0;
    const tick = (now: number) => {
      setT(Math.max(0, now - begin.current));
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <figure
      aria-label="Two graphs of the same nine capabilities: TypeScript, prompt files, CI, evals, feature flags, deployment, tracing, metrics, and profiling. On the left they're separate tools, some joined by hand and some not joined at all. On the right each one links to a central BAML runtime."
      className="relative pb-16"
    >
      <div className="grid md:grid-cols-2 gap-4 md:gap-6">
        <Graph
          edges={WIRED}
          nodes={SCATTERED}
          note="9 tools, joined by hand"
          period={2600}
          still={still}
          t={t}
          title="Separate tools"
        />
        <Graph
          edges={SPOKES}
          hub={HUB}
          nodes={AROUND}
          note="The same 9, one runtime"
          period={2100}
          still={still}
          t={t}
          title="Around the BAML runtime"
        />
      </div>
      <p className="mt-5 max-w-[46rem] text-[14px] md:text-[15px] leading-[1.55] text-white/55">
        The same capabilities on both sides. On the right, each one connects to
        the runtime that runs your code, so they all see the same calls.
      </p>
    </figure>
  );
}
