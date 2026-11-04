'use client';

import { useEffect, useRef, useState } from 'react';

// The development loop as one closed line: left to right through each stage, then down and back along
// the bottom to the start. A soft comet travels it, and reports which stage it has reached. The stage
// after the last one means the comet is on the way back.
const CORNER = 28;

// Longest and faintest first. Laid over each other, these read as one tail that brightens toward the head.
const TAIL = [
  { length: 280, opacity: 0.1 },
  { length: 180, opacity: 0.14 },
  { length: 100, opacity: 0.22 },
  { length: 44, opacity: 0.5 },
  { length: 14, opacity: 1 },
];

function dash(length: number) {
  return `${length} ${1000 - length}`;
}

export default function LoopLine({
  active,
  inset = 12,
  lapMs = 7000,
  nodes = true,
  onStage,
  stations,
  top,
}: {
  /** The stage to light. */
  active: number;
  /** The gap between the return line and the bottom of the box. */
  inset?: number;
  lapMs?: number;
  /** Draws a dot at each stage, for layouts where the stage isn't already a card. */
  nodes?: boolean;
  onStage: (stage: number) => void;
  /** Each stage's horizontal position, as a fraction of the width. */
  stations: number[];
  /** The forward line's distance from the top of the box. */
  top: number;
}) {
  const box = useRef<SVGSVGElement>(null);
  const shape = useRef<SVGPathElement>(null);
  const comet = useRef<SVGGElement>(null);
  const report = useRef(onStage);
  report.current = onStage;
  const [size, setSize] = useState({ height: 0, width: 0 });

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const observer = new ResizeObserver(() => {
      const { height, width } = el.getBoundingClientRect();
      setSize({ height, width });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const xs = stations.map((s) => s * size.width);
  const first = xs[0];
  const last = xs.at(-1) ?? 0;
  const bottom = size.height - inset;
  const d = size.width
    ? `M${first} ${top}H${last}V${bottom - CORNER}Q${last} ${bottom} ${last - CORNER} ${bottom}H${first + CORNER}Q${first} ${bottom} ${first} ${bottom - CORNER}Z`
    : '';

  // biome-ignore lint/correctness/useExhaustiveDependencies: the path's shape follows from the same inputs
  useEffect(() => {
    const path = shape.current;
    const layers = comet.current?.querySelectorAll('path');
    if (!path || !layers || !d) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const total = path.getTotalLength();
    const stops = xs.map((x) => (x - first) / total);
    let frame = 0;
    let shown = -1;
    const start = performance.now();
    const tick = (now: number) => {
      const head = ((now - start) / lapMs) % 1;
      for (const layer of layers) {
        const length = Number(layer.dataset.length);
        layer.setAttribute('stroke-dashoffset', String(length - head * 1000));
      }
      // Each stage holds until the next one. Past the last stage by a little, the comet is on its way back.
      let stage = 0;
      for (let i = 0; i < stops.length; i++) if (head >= stops[i]) stage = i;
      if (head > (last - first) / total + 0.05) stage = stops.length;
      if (stage !== shown) {
        shown = stage;
        report.current(stage);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [d, lapMs]);

  return (
    <svg
      aria-hidden="true"
      className="absolute inset-0 size-full overflow-visible pointer-events-none"
      ref={box}
    >
      {d && (
        <>
          <path
            d={d}
            fill="none"
            ref={shape}
            stroke="rgb(255 255 255 / 0.17)"
            strokeWidth={1.5}
          />
          {/* Which way the line runs, midway along the way back. */}
          <path
            d={`M${first - 6} ${(top + bottom) / 2 + 4}l6 -7l6 7`}
            fill="none"
            stroke="rgb(255 255 255 / 0.5)"
            strokeWidth={1.5}
          />
          <g ref={comet}>
            <g style={{ filter: 'blur(7px)' }}>
              <path
                d={d}
                data-length={120}
                fill="none"
                pathLength={1000}
                stroke="var(--color-clover-core)"
                strokeDasharray={dash(120)}
                strokeLinecap="round"
                strokeOpacity={0.9}
                strokeWidth={7}
              />
            </g>
            {TAIL.map(({ length, opacity }) => (
              <path
                d={d}
                data-length={length}
                fill="none"
                key={length}
                pathLength={1000}
                stroke={
                  length < 50
                    ? 'var(--color-clover-100)'
                    : 'var(--color-clover-core)'
                }
                strokeDasharray={dash(length)}
                strokeLinecap="round"
                strokeOpacity={opacity}
                strokeWidth={2.25}
              />
            ))}
          </g>
          {nodes &&
            xs
              .slice(1)
              .map((x, i) => (
                <path
                  d={`M${(xs[i] + x) / 2 - 4} ${top - 6}l6 6l-6 6`}
                  fill="none"
                  key={x}
                  stroke="rgb(255 255 255 / 0.5)"
                  strokeWidth={1.5}
                />
              ))}
          {nodes &&
            xs.map((x, i) => (
              <circle
                className={`v3-node ${active === i ? 'v3-node-on' : ''}`}
                cx={x}
                cy={top}
                key={x}
                r={6}
              />
            ))}
        </>
      )}
    </svg>
  );
}
