'use client';

import { useId, useState } from 'react';
import { useAnimateInView } from '../_lib/use-animate-in-view';

/*
 * Cores-in-use over time for the same batch of CPU-bound work — the case
 * Promise.all cannot help with. BAML spawns schedule across every core
 * (square wave at 8, stepping down as the queue drains); Node holds one
 * core for the whole run. The chart draws left-to-right via an animated
 * clip sweep. Illustrative numbers: 12 tasks × 1s each on 8 cores.
 */

const X0 = 64;
const X1 = 620;
const Y0 = 204; // y of 0 cores
const CY = 22; // px per core
const T_END = 12; // seconds shown
const SWEEP_S = 4.6; // wall-clock seconds for the full sweep

const x = (t: number) => X0 + ((X1 - X0) / T_END) * t;
const y = (c: number) => Y0 - c * CY;

// 12 tasks, 8 cores: 8 busy for 1s, then 4 busy, then done at t=2.
const BAML_PATH = [
  `M ${x(0)} ${y(0)}`,
  `L ${x(0)} ${y(8)}`,
  `L ${x(1)} ${y(8)}`,
  `L ${x(1)} ${y(4)}`,
  `L ${x(2)} ${y(4)}`,
  `L ${x(2)} ${y(0)}`,
  `L ${x(T_END)} ${y(0)}`,
].join(' ');

// One event loop: 1 core, the whole window, still not done.
const NODE_PATH = [
  `M ${x(0)} ${y(0)}`,
  `L ${x(0)} ${y(1)}`,
  `L ${x(T_END)} ${y(1)}`,
].join(' ');

const delayAt = (t: number) => (SWEEP_S * t) / T_END;

export function CoreUsage() {
  const [runId, setRunId] = useState(0);
  const { ref, holdClass } = useAnimateInView();
  const uid = useId();
  const clipId = `cu-clip-${uid}`;

  return (
    <div className={`cu${holdClass}`} ref={ref}>
      <div className="cu-head">
        <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[#8A8580]">
          cores in use · 12 cpu-bound tasks × 1s · 8 cores
        </span>
        <button
          type="button"
          className="l2-btn"
          onClick={() => setRunId((n) => n + 1)}
        >
          Run again ↻
        </button>
      </div>
      <div key={runId}>
        <svg viewBox="0 0 640 236" className="cu-chart" aria-hidden>
          <title>Cores in use over time</title>
          <clipPath id={clipId}>
            <rect className="cu-sweep" x={X0 - 2} y={0} height={236} />
          </clipPath>

          {/* grid + axes */}
          {[0, 2, 4, 6, 8].map((c) => (
            <g key={c}>
              <line x1={X0} y1={y(c)} x2={X1} y2={y(c)} className="cu-grid" />
              <text
                x={X0 - 10}
                y={y(c) + 4}
                textAnchor="end"
                className="cu-tick"
              >
                {c}
              </text>
            </g>
          ))}
          {[0, 4, 8, 12].map((t) => (
            <text
              key={t}
              x={x(t)}
              y={Y0 + 20}
              textAnchor="middle"
              className="cu-tick"
            >
              {t}s
            </text>
          ))}
          <text
            x={20}
            y={y(4)}
            className="cu-axis"
            transform={`rotate(-90 20 ${y(4)})`}
            textAnchor="middle"
          >
            cores
          </text>

          {/* the two series, revealed by the sweep */}
          <g clipPath={`url(#${clipId})`}>
            <path d={NODE_PATH} className="cu-line cu-line--node" />
            <path d={BAML_PATH} className="cu-line cu-line--baml" />
          </g>

          {/* annotations appear when the sweep reaches them */}
          <text
            x={x(2) + 10}
            y={y(8) + 4}
            className="cu-note cu-note--baml"
            style={{ animationDelay: `${delayAt(2)}s` }}
          >
            ✓ done at t=2s — all 12 tasks
          </text>
          <text
            x={x(11.8)}
            y={y(1) - 8}
            textAnchor="end"
            className="cu-note cu-note--node"
            style={{ animationDelay: `${delayAt(11.5)}s` }}
          >
            one core, still running…
          </text>
        </svg>
        <div className="cu-legend font-mono">
          <span className="cu-key cu-key--baml">baml · spawn, every core</span>
          <span className="cu-key cu-key--node">node · one event loop</span>
          <span className="cu-key cu-key--dim">
            illustrative · native runtime, not this browser tab
          </span>
        </div>
      </div>
    </div>
  );
}
