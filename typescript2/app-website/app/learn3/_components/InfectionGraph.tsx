'use client';

import { useState } from 'react';
import { cn } from '@/lib/utils';
import { useAnimateInView } from '../_lib/use-animate-in-view';

interface Node {
  label: string;
  /** Indentation depth in the call tree. */
  depth: number;
  /** Animation order for the infection (0 = the source); undefined = stays clean. */
  hot?: number;
  /** The nondeterministic source itself. */
  source?: boolean;
}

// A call tree: one branch is pure, the other ends in a model call. The
// infection animates from the source upward — the order matters.
const NODES: Node[] = [
  { label: 'main()', depth: 0, hot: 3 },
  { label: 'load_config()', depth: 1 },
  { label: 'run_pipeline()', depth: 1, hot: 2 },
  { label: 'summarize()', depth: 2, hot: 1 },
  { label: 'llm.summarize_chunk()', depth: 3, hot: 0, source: true },
];

const T0 = 0.5; // when the source lights up
const STEP = 0.65; // per-hop propagation delay

/**
 * The thesis visual: one nondeterministic call, and every ancestor on its
 * path becomes nondeterministic too. Pure CSS animation; replay remounts.
 */
export function InfectionGraph() {
  const [runId, setRunId] = useState(0);
  const { ref, holdClass } = useAnimateInView();
  const settled = T0 + 3 * STEP + 0.6;

  return (
    <div className={`l3-inf${holdClass}`} ref={ref}>
      <div className="l3-inf-head">
        <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[#8A8580]">
          one stochastic call
        </span>
        <button
          type="button"
          className="l2-btn"
          onClick={() => setRunId((n) => n + 1)}
        >
          Replay ↻
        </button>
      </div>
      <div key={runId}>
        <div className="l3-inf-rows font-mono">
          {NODES.map((n) => {
            const delay = n.hot != null ? T0 + n.hot * STEP : 0;
            return (
              <div className="l3-inf-row" key={n.label}>
                <span className="l3-inf-tree" aria-hidden>
                  {n.depth === 0 ? '' : `${'   '.repeat(n.depth - 1)}└─ `}
                </span>
                <span
                  className={cn(
                    'l3-inf-node',
                    n.hot == null && 'l3-inf-node--clean',
                    n.hot != null && 'l3-inf-node--hot',
                  )}
                  style={
                    n.hot != null ? { animationDelay: `${delay}s` } : undefined
                  }
                >
                  {n.label}
                </span>
                {n.source ? (
                  <span
                    className="l3-inf-tag l3-inf-tag--hot"
                    style={{ animationDelay: `${T0}s` }}
                  >
                    ⚡ same input → different output
                  </span>
                ) : n.hot != null ? (
                  <span
                    className="l3-inf-tag l3-inf-tag--hot"
                    style={{ animationDelay: `${T0 + n.hot * STEP}s` }}
                  >
                    output can now vary
                  </span>
                ) : (
                  <span
                    className="l3-inf-tag l3-inf-tag--ok"
                    style={{ animationDelay: `${settled}s` }}
                  >
                    still deterministic
                  </span>
                )}
              </div>
            );
          })}
        </div>
        <p className="l3-inf-caption" style={{ animationDelay: `${settled}s` }}>
          <strong>4 of 5 functions</strong> can no longer be tested with{' '}
          <code>assert output == expected</code> — and nothing in the language
          marks them.
        </p>
      </div>
    </div>
  );
}
