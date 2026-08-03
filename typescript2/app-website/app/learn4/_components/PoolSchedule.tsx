'use client';

import { useState } from 'react';
import { useAnimateInView } from '../../learn3/_lib/use-animate-in-view';

/**
 * Six tasks through a TaskGroup with limit = 2: two run, the rest queue
 * FIFO and start as slots free up. Timeline per task: queued (amber) until
 * its start time, then a fill, then a check. CSS delays only; replay
 * remounts. Illustrative timing — every task takes one tick.
 */
const TASKS = 6;
const LIMIT = 2;
const TICK = 0.9; // seconds per task

export function PoolSchedule() {
  const [runId, setRunId] = useState(0);
  const { ref, holdClass } = useAnimateInView();
  return (
    <div className={`l4-pool${holdClass}`} ref={ref}>
      <div className="l4-pool-head">
        <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[#8A8580]">
          TaskGroup.new(2) · six spawns
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
        {Array.from({ length: TASKS }).map((_, i) => {
          const wait = Math.floor(i / LIMIT) * TICK;
          const start = 0.3 + wait;
          return (
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length, order-stable
            <div key={i} className="l4-pool-row">
              <span className="l4-pool-name font-mono">shard-{i}</span>
              <span className="l4-pool-track">
                <span
                  className="l4-pool-fill"
                  style={{
                    animationDelay: `${start}s`,
                    animationDuration: `${TICK}s`,
                  }}
                />
              </span>
              <span className="l4-pool-status font-mono">
                <span
                  className="l4-pool-queued"
                  style={{ animationDelay: `${start}s` }}
                >
                  {wait > 0 ? 'queued' : 'running'}
                </span>
                <span
                  className="l4-pool-done"
                  style={{ animationDelay: `${start + TICK}s` }}
                >
                  ✓
                </span>
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
