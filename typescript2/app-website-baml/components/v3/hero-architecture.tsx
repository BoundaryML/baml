'use client';

import { useEffect, useRef, useState } from 'react';

// The toolchain drawn as an internal architecture document: six components on a dot grid, joined by
// hairline connectors. One connection lights at a time, following information around the loop from the
// agent to production and back. Hovering or focusing a component pauses the sequence and lights the
// component and everything it connects to. The drawing uses one coordinate space, 1200 by 560, for both
// the boxes and the lines, so they stay aligned at any width.
const W = 1200;
const H = 560;

type Id = 'agent' | 'compiler' | 'runtime' | 'production' | 'store' | 'query';

const NODES: {
  about: string;
  id: Id;
  items: string[];
  name: string;
  tag: string;
  x: number;
  y: number;
}[] = [
  {
    about:
      'The agent edits code, runs tests, and reads production through baml query.',
    id: 'agent',
    items: ['reads code', 'edits', 'runs tests'],
    name: 'Agent',
    tag: 'any agent',
    x: 60,
    y: 70,
  },
  {
    about:
      'The compiler checks every edit and hands the runtime one compiled program. Its types also describe every span.',
    id: 'compiler',
    items: ['parser', 'type checker', 'codegen'],
    name: 'Compiler',
    tag: 'baml check',
    x: 480,
    y: 70,
  },
  {
    about:
      'The runtime makes the model calls, with clients, retries, and parsing built in.',
    id: 'runtime',
    items: ['clients', 'retries', 'parsing'],
    name: 'Runtime',
    tag: 'baml pack',
    x: 900,
    y: 70,
  },
  {
    about:
      'The query engine runs SQL over spans, so the agent can ask what happened.',
    id: 'query',
    items: ['SQL over spans', 'typed fields'],
    name: 'Query engine',
    tag: 'baml query',
    x: 60,
    y: 350,
  },
  {
    about:
      'The trace store keeps every call’s inputs, outputs, and timing, typed by the compiler’s schemas.',
    id: 'store',
    items: ['spans', 'inputs', 'outputs'],
    name: 'Trace store',
    tag: 'every call',
    x: 480,
    y: 350,
  },
  {
    about:
      'Your app calls BAML functions in production. Every call becomes a span.',
    id: 'production',
    items: ['your app', 'live traffic'],
    name: 'Production',
    tag: 'v42',
    x: 900,
    y: 350,
  },
];
const BOX = { h: 140, w: 240 };

type Edge = {
  about?: string;
  from: Id;
  label: string;
  loop: boolean;
  points: [number, number][];
  to: Id;
};

// The loop, in order, then the dependencies that sit outside it.
const EDGES: Edge[] = [
  {
    about:
      'The agent’s edits go to the compiler, which checks types before anything runs.',
    from: 'agent',
    label: 'edits',
    loop: true,
    points: [
      [300, 140],
      [480, 140],
    ],
    to: 'compiler',
  },
  {
    about:
      'The compiler hands the runtime one program, with the prompts, clients, and types it needs.',
    from: 'compiler',
    label: 'compiled program',
    loop: true,
    points: [
      [720, 140],
      [900, 140],
    ],
    to: 'runtime',
  },
  {
    about: 'The runtime serves your app’s calls in production.',
    from: 'runtime',
    label: 'calls',
    loop: true,
    points: [
      [1020, 210],
      [1020, 350],
    ],
    to: 'production',
  },
  {
    about:
      'Every call in production is recorded as a span, with its inputs and outputs.',
    from: 'production',
    label: 'spans',
    loop: true,
    points: [
      [900, 420],
      [720, 420],
    ],
    to: 'store',
  },
  {
    about:
      'The query engine reads spans with SQL, typed by the same schemas the compiler checked.',
    from: 'store',
    label: 'typed spans',
    loop: true,
    points: [
      [480, 420],
      [300, 420],
    ],
    to: 'query',
  },
  {
    about:
      'The agent asks production what happened with baml query, and starts the next edit.',
    from: 'query',
    label: 'baml query',
    loop: true,
    points: [
      [180, 350],
      [180, 210],
    ],
    to: 'agent',
  },
  {
    from: 'compiler',
    label: 'schemas',
    loop: false,
    points: [
      [600, 210],
      [600, 350],
    ],
    to: 'store',
  },
  {
    from: 'agent',
    label: 'baml test',
    loop: false,
    points: [
      [180, 70],
      [180, 36],
      [1020, 36],
      [1020, 70],
    ],
    to: 'runtime',
  },
];
const LOOP = EDGES.filter((e) => e.loop);
const STEP_MS = 2400;

function length(points: [number, number][]) {
  let total = 0;
  for (let i = 1; i < points.length; i++) {
    total +=
      Math.abs(points[i][0] - points[i - 1][0]) +
      Math.abs(points[i][1] - points[i - 1][1]);
  }
  return total;
}

/** A point part of the way along an orthogonal path. */
function along(points: [number, number][], u: number): [number, number] {
  let left = length(points) * u;
  for (let i = 1; i < points.length; i++) {
    const [ax, ay] = points[i - 1];
    const [bx, by] = points[i];
    const seg = Math.abs(bx - ax) + Math.abs(by - ay);
    if (left <= seg) {
      const k = seg ? left / seg : 0;
      return [ax + (bx - ax) * k, ay + (by - ay) * k];
    }
    left -= seg;
  }
  return points.at(-1) ?? points[0];
}

/** Where an edge's label sits: the middle of its longest segment. */
function labelAt(points: [number, number][]) {
  let best = 1;
  let most = -1;
  for (let i = 1; i < points.length; i++) {
    const seg =
      Math.abs(points[i][0] - points[i - 1][0]) +
      Math.abs(points[i][1] - points[i - 1][1]);
    if (seg > most) {
      most = seg;
      best = i;
    }
  }
  const [ax, ay] = points[best - 1];
  const [bx, by] = points[best];
  return { vertical: ax === bx, x: (ax + bx) / 2, y: (ay + by) / 2 };
}

const pct = (n: number, of: number) => `${(n / of) * 100}%`;

export default function HeroArchitecture() {
  const [step, setStep] = useState(0);
  const [hover, setHover] = useState<Id | null>(null);
  const marker = useRef<SVGCircleElement>(null);
  const paused = useRef(false);
  paused.current = hover !== null;

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const begin = performance.now();
    let offset = 0;
    let stoppedAt = 0;
    let last = -1;
    let id = 0;
    const tick = (now: number) => {
      if (paused.current) {
        if (!stoppedAt) stoppedAt = now;
      } else {
        if (stoppedAt) offset += now - stoppedAt;
        stoppedAt = 0;
        // A frame's timestamp can be a little earlier than the time the effect started.
        const t = Math.max(0, now - begin - offset);
        const s = Math.floor(t / STEP_MS) % LOOP.length;
        if (s !== last) {
          last = s;
          setStep(s);
        }
        const u = Math.min(1, ((t % STEP_MS) / STEP_MS) * 1.4);
        const [x, y] = along(LOOP[s].points, u);
        marker.current?.setAttribute('cx', String(x));
        marker.current?.setAttribute('cy', String(y));
      }
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(id);
  }, []);

  const current = LOOP[step];
  const lit = (e: Edge) =>
    hover ? e.from === hover || e.to === hover : e === current;
  const near = (id: Id) =>
    hover
      ? id === hover ||
        EDGES.some(
          (e) =>
            (e.from === hover && e.to === id) ||
            (e.to === hover && e.from === id),
        )
      : id === current.from || id === current.to;
  const focus = NODES.find((n) => n.id === hover);

  return (
    <figure
      aria-label="An architecture drawing of the BAML toolchain. The agent's edits go to the compiler, the compiler hands the runtime a compiled program, the runtime serves calls in production, every call is recorded in the trace store, the query engine reads the spans, and the agent queries them with baml query. The compiler also gives the trace store its schemas, and the agent runs baml test against the runtime."
      className="relative pb-16"
    >
      <div
        className="overflow-hidden rounded-md border border-white/12 bg-[#09090a]"
        style={{
          backgroundImage:
            'radial-gradient(rgb(255 255 255 / 0.11) 1px, transparent 1px)',
          backgroundSize: '18px 18px',
        }}
      >
        {/* The document's header and legend. */}
        <div className="flex flex-wrap items-center justify-between gap-x-8 gap-y-2 px-4 md:px-6 py-3 border-b border-white/12 bg-[#09090a] font-mono text-[11.5px] md:text-[12px] text-white/50">
          <p className="flex flex-wrap gap-x-5 gap-y-1">
            <span className="text-white/85">BAML system overview</span>
            <span>Fig. 2</span>
            <span>Rev 42</span>
          </p>
          <ul className="flex flex-wrap items-center gap-x-5 gap-y-1">
            <li className="flex items-center gap-2">
              <span className="w-5 h-px bg-white/45" />
              data flow
            </li>
            <li className="flex items-center gap-2">
              <span className="w-5 border-t border-dashed border-white/45" />
              dependency
            </li>
            <li className="flex items-center gap-2">
              <span className="size-1.5 rounded-full bg-clover" />
              active
            </li>
          </ul>
        </div>

        {/* Wide screens: the drawing. */}
        <div className="relative hidden md:block mx-6 my-8 aspect-[1200/560]">
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full overflow-visible"
            viewBox={`0 0 ${W} ${H}`}
          >
            <defs>
              {[
                ['arrow', 'rgb(255 255 255 / 0.4)'],
                ['arrow-on', 'var(--color-clover-core)'],
              ].map(([id, color]) => (
                <marker
                  id={`v3-${id}`}
                  key={id}
                  markerHeight="8"
                  markerWidth="8"
                  orient="auto"
                  refX="7"
                  refY="4"
                  viewBox="0 0 8 8"
                >
                  <path d="M0 0L8 4L0 8z" fill={color} />
                </marker>
              ))}
            </defs>
            {EDGES.map((e) => {
              const on = lit(e);
              return (
                <polyline
                  fill="none"
                  key={e.label}
                  markerEnd={`url(#v3-${on ? 'arrow-on' : 'arrow'})`}
                  points={e.points.map((p) => p.join(',')).join(' ')}
                  stroke={
                    on ? 'var(--color-clover-core)' : 'rgb(255 255 255 / 0.3)'
                  }
                  strokeDasharray={e.loop ? undefined : '4 4'}
                  strokeWidth={on ? 1.5 : 1}
                  style={{ transition: 'stroke 200ms' }}
                />
              );
            })}
            {!hover && (
              <circle
                cx={current.points[0][0]}
                cy={current.points[0][1]}
                fill="var(--color-clover-100)"
                r="4"
                ref={marker}
                stroke="#09090a"
                strokeWidth="2"
              />
            )}
          </svg>

          {EDGES.map((e) => {
            const at = labelAt(e.points);
            return (
              <span
                className={`absolute px-1.5 bg-[#09090a] font-mono text-[11px] lg:text-[12px] whitespace-nowrap transition-colors duration-200 ${lit(e) ? 'text-clover' : 'text-white/45'} ${at.vertical ? 'translate-x-2 -translate-y-1/2' : '-translate-x-1/2 -translate-y-[calc(100%+3px)]'}`}
                key={e.label}
                style={{ left: pct(at.x, W), top: pct(at.y, H) }}
              >
                {e.label}
              </span>
            );
          })}

          {NODES.map((n) => {
            const on = near(n.id);
            const dim = hover !== null && !on;
            return (
              <button
                aria-label={`${n.name}: ${n.about}`}
                className={`focus-ring absolute flex flex-col text-left rounded-[3px] border bg-[#0d0d0f] transition-[border-color,opacity] duration-200 cursor-default ${n.id === hover ? 'border-clover' : on ? 'border-white/45' : 'border-white/16'} ${dim ? 'opacity-40' : ''}`}
                key={n.id}
                onBlur={() => setHover(null)}
                onFocus={() => setHover(n.id)}
                onMouseEnter={() => setHover(n.id)}
                onMouseLeave={() => setHover(null)}
                style={{
                  height: pct(BOX.h, H),
                  left: pct(n.x, W),
                  top: pct(n.y, H),
                  width: pct(BOX.w, W),
                }}
                type="button"
              >
                <span className="flex items-baseline justify-between gap-2 px-3 py-2 border-b border-white/10">
                  <span className="text-[14px] lg:text-[15px] font-medium text-white">
                    {n.name}
                  </span>
                  <span
                    className={`font-mono text-[11px] lg:text-[12px] transition-colors duration-200 ${on ? 'text-clover' : 'text-white/40'}`}
                  >
                    {n.tag}
                  </span>
                </span>
                <span className="flex flex-col gap-0.5 px-3 py-2 font-mono text-[11px] lg:text-[12px] text-white/50">
                  {n.items.map((item) => (
                    <span key={item}>{item}</span>
                  ))}
                </span>
              </button>
            );
          })}
        </div>

        {/* Phones: the same loop as a column, read top to bottom. */}
        <ol className="md:hidden px-4 py-6">
          {LOOP.map((e, i) => {
            const n = NODES.find((m) => m.id === e.from);
            if (!n) return null;
            const on = e === current;
            return (
              <li key={n.id}>
                <div
                  className={`rounded-[3px] border bg-[#0d0d0f] transition-colors duration-200 ${on || current.to === n.id ? 'border-white/45' : 'border-white/16'}`}
                >
                  <p className="flex items-baseline justify-between gap-2 px-3 py-2 border-b border-white/10">
                    <span className="text-[14px] font-medium text-white">
                      {n.name}
                    </span>
                    <span className="font-mono text-[11px] text-white/40">
                      {n.tag}
                    </span>
                  </p>
                  <p className="px-3 py-2 font-mono text-[11px] text-white/50">
                    {n.items.join(', ')}
                  </p>
                </div>
                <div className="flex items-center gap-3 h-10 pl-6">
                  <span
                    className={`relative w-px h-full transition-colors duration-200 ${on ? 'bg-clover' : 'bg-white/30'}`}
                  >
                    <span
                      className={`absolute -left-[3px] bottom-0 size-0 border-x-[3.5px] border-x-transparent border-t-[5px] transition-colors duration-200 ${on ? 'border-t-clover' : 'border-t-white/40'}`}
                    />
                  </span>
                  <span
                    className={`font-mono text-[11.5px] transition-colors duration-200 ${on ? 'text-clover' : 'text-white/45'}`}
                  >
                    {e.label}
                    {i === LOOP.length - 1 ? ', back to the agent' : ''}
                  </span>
                </div>
              </li>
            );
          })}
        </ol>

        <p
          aria-live="polite"
          className="v3-enter flex gap-4 px-4 md:px-6 py-3 border-t border-white/12 bg-[#09090a] font-mono text-[12px] md:text-[13px] text-white/75"
          key={hover ?? step}
        >
          <span className="shrink-0 text-white/35">
            {focus ? focus.name : `${step + 1}/${LOOP.length}`}
          </span>
          {focus ? focus.about : current.about}
        </p>
      </div>
    </figure>
  );
}
