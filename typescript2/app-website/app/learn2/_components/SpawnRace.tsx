'use client';

import { useState } from 'react';

/**
 * Simulated throughput race: BAML `spawn` (green threads on one runtime) finishing
 * a batch of I/O-bound calls concurrently vs Node async/await running them with the
 * usual event-loop overhead. Numbers are illustrative for now (see checkpoint note).
 *
 * Animation is 100% CSS keyframes (see learn2.css). "Run again" bumps a key to
 * remount the lanes and replay — no effects, no timers.
 */
const TASKS = 8;

interface LaneProps {
  label: string;
  sublabel: string;
  /** Total wall-clock seconds the whole batch takes (animation length). */
  total: number;
  color: string;
  concurrent?: boolean;
}

function Lane({ label, sublabel, total, color, concurrent }: LaneProps) {
  return (
    <div className="l2-lane">
      <div className="l2-lane-meta">
        <span className="l2-lane-label font-mono">{label}</span>
        <span className="l2-lane-sub">{sublabel}</span>
      </div>
      <div className="l2-lane-bars">
        {Array.from({ length: TASKS }).map((_, i) => {
          const duration = concurrent ? total : total / TASKS;
          const delay = concurrent ? 0 : i * (total / TASKS);
          return (
            <div
              className="l2-bar"
              // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length, order-stable
              key={i}
            >
              <span
                className="l2-bar-fill"
                style={{
                  background: color,
                  animationDuration: `${duration}s`,
                  animationDelay: `${delay}s`,
                }}
              />
            </div>
          );
        })}
      </div>
      <div
        className="l2-lane-clock font-mono"
        style={{ animationDelay: `${total}s`, color }}
      >
        {total.toFixed(2)}s
      </div>
    </div>
  );
}

export function SpawnRace() {
  const [runId, setRunId] = useState(0);
  return (
    <div className="l2-race">
      <div className="l2-race-head">
        <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[#8A8580]">
          {TASKS} concurrent LLM calls
        </span>
        <button
          type="button"
          className="l2-btn"
          onClick={() => setRunId((n) => n + 1)}
        >
          Run again ↻
        </button>
      </div>
      {/* key={runId} remounts the lanes so the CSS animations replay */}
      <div key={runId} className="l2-race-lanes">
        <Lane
          label="BAML · spawn"
          sublabel="green threads, one runtime, no coloring"
          total={0.5}
          color="#047857"
          concurrent
        />
        <Lane
          label="Node · async/await"
          sublabel="event loop, colored functions"
          total={2.0}
          color="#B45309"
        />
      </div>
    </div>
  );
}
