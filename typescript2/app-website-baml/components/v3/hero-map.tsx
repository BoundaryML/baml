'use client';

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { RED } from './story';

// A codebase drawn as a map, somewhere between an editor's minimap and a profiler's flame graph. Every
// function is a box sized by its length. Boxes blink as functions run, and when a trace passes through,
// its path lights up across the map while the flame graph below plays it back. The point is that an agent
// changing this code can see how it behaves. The files, functions, and traces are example data.
type Fn = {
  id: string;
  lines: number;
  name: string;
  spark?: boolean;
  status?: 'ok' | 'error';
};

const FILES: { fns: Fn[]; name: string }[] = [
  {
    fns: [
      { id: 'handle', lines: 6, name: 'handle_ticket', spark: true },
      { id: 'enqueue', lines: 2, name: 'enqueue' },
      { id: 'notify', lines: 3, name: 'notify_team' },
    ],
    name: 'tickets.baml',
  },
  {
    fns: [
      { id: 'classify', lines: 6, name: 'classify', spark: true, status: 'ok' },
      { id: 'verdict', lines: 3, name: 'class Verdict' },
      { id: 'route', lines: 3, name: 'route' },
    ],
    name: 'triage.baml',
  },
  {
    fns: [
      { id: 'draft', lines: 7, name: 'draft_reply', spark: true },
      {
        id: 'tone',
        lines: 3,
        name: 'check_tone',
        spark: true,
        status: 'error',
      },
      { id: 'retrieve', lines: 3, name: 'retrieve' },
    ],
    name: 'reply.baml',
  },
  {
    fns: [
      { id: 'kb', lines: 4, name: 'query_kb', spark: true },
      { id: 'rerank', lines: 3, name: 'rerank' },
      { id: 'embed', lines: 2, name: 'embed' },
    ],
    name: 'search.baml',
  },
  {
    fns: [
      {
        id: 'refund',
        lines: 4,
        name: 'refund_eligible',
        spark: true,
        status: 'ok',
      },
      { id: 'invoice', lines: 5, name: 'parse_invoice' },
    ],
    name: 'billing.baml',
  },
  {
    fns: [
      { id: 'gpt', lines: 2, name: 'client GPT4o' },
      { id: 'claude', lines: 2, name: 'client Claude' },
      { id: 'fallback', lines: 3, name: 'client Fallback' },
    ],
    name: 'clients.baml',
  },
];
const FNS = FILES.flatMap((f) => f.fns);

// Each span is a function, its depth in the call stack, and where it starts and ends, in percent of the trace.
type Span = {
  depth: number;
  end: number;
  error?: boolean;
  fn: string;
  start: number;
};

const TRACES: { id: string; input: string; ms: number; spans: Span[] }[] = [
  {
    id: '7f3a9c',
    input: '"Third time asking. Refund me."',
    ms: 1840,
    spans: [
      { depth: 0, end: 100, fn: 'handle', start: 0 },
      { depth: 1, end: 34, fn: 'classify', start: 2 },
      { depth: 2, end: 32, fn: 'gpt', start: 5 },
      { depth: 1, end: 42, fn: 'route', start: 36 },
      { depth: 1, end: 60, fn: 'refund', start: 44 },
      { depth: 2, end: 58, fn: 'claude', start: 47 },
      { depth: 1, end: 98, fn: 'draft', start: 62 },
      { depth: 2, end: 76, fn: 'retrieve', start: 64 },
      { depth: 3, end: 75, fn: 'kb', start: 65 },
      { depth: 2, end: 96, fn: 'gpt', start: 78 },
    ],
  },
  {
    id: 'b21e04',
    input: '"La app no carga desde ayer"',
    ms: 2310,
    spans: [
      { depth: 0, end: 100, fn: 'handle', start: 0 },
      { depth: 1, end: 30, fn: 'classify', start: 2 },
      { depth: 2, end: 28, fn: 'fallback', start: 4 },
      { depth: 1, end: 36, fn: 'route', start: 32 },
      { depth: 1, end: 78, fn: 'draft', start: 38 },
      { depth: 2, end: 54, fn: 'retrieve', start: 40 },
      { depth: 3, end: 52, fn: 'kb', start: 41 },
      { depth: 2, end: 76, fn: 'claude', start: 56 },
      { depth: 1, end: 94, error: true, fn: 'tone', start: 80 },
      { depth: 1, end: 99, fn: 'notify', start: 95 },
    ],
  },
];

const TICK_MS = 50;
const TRACE_MS = 5600;
const HOLD_MS = 1800;
const IDLE_MS = 2600;
const CYCLE = TRACE_MS + HOLD_MS + IDLE_MS;
const BLIP_MS = 700;
const DEPTHS = 4;

// Deterministic, so the server and the browser draw the same minimap lines.
const width = (fn: string, line: number) =>
  35 + (((fn.length * 31 + line * 17) % 11) / 11) * 60;

function Spark({ values }: { values: number[] }) {
  const max = Math.max(...values);
  const d = values
    .map(
      (v, i) =>
        `${i ? 'L' : 'M'}${(i / (values.length - 1)) * 40} ${12 - (v / max) * 10}`,
    )
    .join('');
  return (
    <svg
      aria-hidden="true"
      className="shrink-0"
      height="12"
      viewBox="0 0 40 12"
      width="40"
    >
      <path d={d} fill="none" stroke="currentColor" strokeWidth="1" />
    </svg>
  );
}

export default function HeroMap() {
  const [t, setT] = useState(0);
  const [blips, setBlips] = useState<Record<string, number>>({});
  const [sparks, setSparks] = useState<Record<string, number[]>>(() =>
    Object.fromEntries(
      FNS.filter((f) => f.spark).map((f, i) => [
        f.id,
        Array.from({ length: 12 }, (_, j) => 4 + ((i * 7 + j * 5) % 9)),
      ]),
    ),
  );
  const map = useRef<HTMLDivElement>(null);
  const nodes = useRef<Record<string, HTMLDivElement | null>>({});
  const [centers, setCenters] = useState<
    Record<string, { x: number; y: number }>
  >({});

  useLayoutEffect(() => {
    const measure = () => {
      const box = map.current?.getBoundingClientRect();
      if (!box) return;
      setCenters(
        Object.fromEntries(
          Object.entries(nodes.current).flatMap(([id, el]) => {
            const r = el?.getBoundingClientRect();
            return r
              ? [
                  [
                    id,
                    {
                      x: r.left + r.width / 2 - box.left,
                      y: r.top + r.height / 2 - box.top,
                    },
                  ],
                ]
              : [];
          }),
        ),
      );
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (map.current) observer.observe(map.current);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setT(TRACE_MS * 0.7);
      return;
    }
    const begin = performance.now();
    let lastBlip = 0;
    const id = setInterval(() => {
      const now = performance.now() - begin;
      setT(now);
      // Elsewhere in the codebase, other functions run on their own now and then.
      if (now - lastBlip > 260) {
        lastBlip = now;
        const fn = FNS[Math.floor(Math.random() * FNS.length)];
        setBlips((b) => ({ ...b, [fn.id]: now }));
        if (fn.spark) {
          setSparks((s) => ({
            ...s,
            [fn.id]: [...s[fn.id].slice(1), 3 + Math.random() * 10],
          }));
        }
      }
    }, TICK_MS);
    return () => clearInterval(id);
  }, []);

  const lap = Math.floor(t / CYCLE);
  const local = t % CYCLE;
  const trace = TRACES[lap % TRACES.length];
  const showing = local < TRACE_MS + HOLD_MS;
  const head = Math.min(100, (local / TRACE_MS) * 100);

  const active = new Set<string>();
  const visited: string[] = [];
  let failed = false;
  if (showing) {
    for (const s of [...trace.spans].sort((a, b) => a.start - b.start)) {
      if (head < s.start) continue;
      if (!visited.includes(s.fn)) visited.push(s.fn);
      if (head <= s.end && local < TRACE_MS) active.add(s.fn);
      if (s.error && head >= s.end) failed = true;
    }
  }
  const path = visited
    .map((id) => centers[id])
    .filter(Boolean)
    .map((p, i) => `${i ? 'L' : 'M'}${p.x} ${p.y}`)
    .join('');

  return (
    <figure
      aria-label="A map of a codebase in which every function is a box sized by its length. Boxes blink as functions run. When a trace of handle_ticket passes through, its path lights up across classify, route, draft_reply, query_kb, and the model clients, while a flame graph below plays it back."
      className="relative pb-16"
    >
      <div className="v3-window overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-1 px-4 md:px-5 py-3 border-b border-white/8 font-mono text-[12px] md:text-[13px]">
          <span className="text-white/70">triage, 6 files, 17 functions</span>
          <span className="flex items-center gap-4 text-white/45">
            <span className="flex items-center gap-1.5">
              <span className="size-2 rounded-[2px] bg-clover" />
              on the trace
            </span>
            <span className="flex items-center gap-1.5">
              <span className="size-2 rounded-[2px] bg-white/30" />
              running
            </span>
          </span>
        </div>

        <div className="relative p-3 md:p-5" ref={map}>
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-3 md:gap-4">
            {FILES.map((file) => (
              <div className="min-w-0" key={file.name}>
                <p className="mb-1.5 font-mono text-[11px] md:text-[12px] text-white/40 truncate">
                  {file.name}
                </p>
                <div className="flex flex-col gap-1">
                  {file.fns.map((fn) => {
                    const on = active.has(fn.id);
                    const seen = visited.includes(fn.id);
                    const blip = t - (blips[fn.id] ?? -9999) < BLIP_MS;
                    const broken = fn.id === 'tone' && failed;
                    return (
                      <div
                        className="relative px-2 py-1.5 rounded-[3px] border transition-[background-color,border-color] duration-200"
                        key={fn.id}
                        ref={(el) => {
                          nodes.current[fn.id] = el;
                        }}
                        style={{
                          background: on
                            ? 'rgb(from var(--color-clover-core) r g b / 0.28)'
                            : blip
                              ? 'rgb(255 255 255 / 0.07)'
                              : 'rgb(255 255 255 / 0.02)',
                          borderColor: broken
                            ? RED
                            : on
                              ? 'var(--color-clover-core)'
                              : seen
                                ? 'rgb(from var(--color-clover-core) r g b / 0.5)'
                                : 'rgb(255 255 255 / 0.09)',
                          minHeight: 22 + fn.lines * 5,
                        }}
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span
                            className={`font-mono text-[10.5px] md:text-[11.5px] truncate ${on || seen ? 'text-white' : 'text-white/60'}`}
                          >
                            {fn.name}
                          </span>
                          {fn.status && (
                            <span
                              className="size-1.5 shrink-0 rounded-full"
                              style={{
                                background:
                                  fn.status === 'error'
                                    ? RED
                                    : 'var(--color-clover-core)',
                              }}
                            />
                          )}
                        </div>
                        {/* The function's source, as an editor's minimap draws it. */}
                        <div className="mt-1 flex flex-col gap-[3px]">
                          {Array.from({ length: fn.lines }, (_, i) => (
                            <span
                              className="h-[2px] rounded-full bg-white/[0.13]"
                              // biome-ignore lint/suspicious/noArrayIndexKey: fixed decoration
                              key={i}
                              style={{
                                marginLeft: i % 3 ? 8 : 0,
                                width: `${width(fn.name, i)}%`,
                              }}
                            />
                          ))}
                        </div>
                        {fn.spark && (
                          <span
                            className={`absolute right-2 bottom-1.5 hidden sm:block ${on ? 'text-clover-100' : 'text-white/35'}`}
                          >
                            <Spark values={sparks[fn.id]} />
                          </span>
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>

          {/* The trace's path across the map, in the order it reached each function. */}
          {path && (
            <svg
              aria-hidden="true"
              className="absolute inset-0 size-full pointer-events-none overflow-visible"
            >
              <path
                d={path}
                fill="none"
                stroke="rgb(from var(--color-clover-core) r g b / 0.7)"
                strokeDasharray="3 3"
                strokeWidth="1"
              />
              {visited.map((id) =>
                centers[id] ? (
                  <circle
                    cx={centers[id].x}
                    cy={centers[id].y}
                    fill="var(--color-clover-100)"
                    key={id}
                    r="2.5"
                  />
                ) : null,
              )}
            </svg>
          )}
        </div>

        {/* The same trace as a flame graph, played back under a moving playhead. */}
        <div className="px-3 md:px-5 pt-3 pb-4 border-t border-white/8">
          <div className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1 mb-2 font-mono text-[11.5px] md:text-[12.5px]">
            <span className="min-w-0 truncate text-white/70">
              trace {trace.id}, handle_ticket({trace.input})
            </span>
            <span
              className={failed ? '' : 'text-white/45'}
              style={failed ? { color: '#FF9A9E' } : undefined}
            >
              {failed
                ? 'check_tone failed'
                : `${(trace.ms / 1000).toFixed(2)}s`}
            </span>
          </div>
          <div
            className="relative transition-opacity duration-500"
            style={{ height: DEPTHS * 20, opacity: showing ? 1 : 0.25 }}
          >
            {trace.spans.map((s, i) => {
              const fn = FNS.find((f) => f.id === s.fn);
              const reached = showing && head >= s.start;
              const on = active.has(s.fn) && head <= s.end && head >= s.start;
              return (
                <div
                  className="absolute h-[17px] px-1.5 rounded-[2px] border overflow-hidden font-mono text-[10px] md:text-[11px] leading-[15px] whitespace-nowrap transition-[background-color,border-color,color] duration-200"
                  // biome-ignore lint/suspicious/noArrayIndexKey: a span's place in its trace
                  key={i}
                  style={{
                    background: on
                      ? 'rgb(from var(--color-clover-core) r g b / 0.4)'
                      : reached
                        ? 'rgb(from var(--color-clover-core) r g b / 0.14)'
                        : 'rgb(255 255 255 / 0.03)',
                    borderColor:
                      s.error && reached && head >= s.end
                        ? RED
                        : reached
                          ? 'rgb(from var(--color-clover-core) r g b / 0.55)'
                          : 'rgb(255 255 255 / 0.1)',
                    color: reached ? '#fff' : 'rgb(255 255 255 / 0.35)',
                    left: `${s.start}%`,
                    top: s.depth * 20,
                    width: `${s.end - s.start}%`,
                  }}
                >
                  {fn?.name}
                </div>
              );
            })}
            {showing && local < TRACE_MS && (
              <span
                aria-hidden
                className="absolute -top-1 -bottom-1 w-px bg-white"
                style={{ left: `${head}%` }}
              />
            )}
          </div>
        </div>
      </div>
      <p className="mt-5 max-w-[40rem] text-[14px] md:text-[15px] leading-[1.55] text-white/55">
        Functions light up as they run. When a trace passes through, its whole
        path lights up, so an agent changing{' '}
        <span className="font-mono text-white/80">classify</span> can see what
        calls it and how it behaves.
      </p>
    </figure>
  );
}
