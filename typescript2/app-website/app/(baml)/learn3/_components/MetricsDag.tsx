'use client';

import { useState } from 'react';
import { useAnimateInView } from '../_lib/use-animate-in-view';

/*
 * The metrics design as a live dependency graph. The root node is the
 * function call itself; a `metric` block attaches to it; measurement
 * nodes fire as their inputs become available — first when the function
 * returns, then again hours later when the human label arrives.
 * Pure CSS/SVG animation with computed delays; replay remounts.
 */

interface Node {
  id: string;
  x: number;
  y: number;
  w: number;
  label: string;
  sub?: string;
  kind?: 'root' | 'slot';
  /** Seconds until this node fires. */
  at: number;
}

const H = 34;

const NODES: Node[] = [
  {
    id: 'root',
    x: 14,
    y: 52,
    w: 176,
    label: 'extract_resume(doc)',
    kind: 'root',
    at: 0.5,
  },
  { id: 'field_count', x: 256, y: 30, w: 122, label: 'field_count', at: 1.1 },
  {
    id: 'judge',
    x: 256,
    y: 102,
    w: 122,
    label: 'judge',
    sub: '1 llm call',
    at: 1.35,
  },
  { id: 'quality', x: 478, y: 72, w: 100, label: 'quality', at: 1.9 },
  {
    id: 'faithfulness',
    x: 466,
    y: 142,
    w: 124,
    label: 'faithfulness',
    at: 2.1,
  },
  {
    id: 'expected',
    x: 14,
    y: 240,
    w: 124,
    label: 'expected',
    sub: 'arrives t+4h',
    kind: 'slot',
    at: 3.3,
  },
  { id: 'precision', x: 256, y: 210, w: 122, label: 'precision', at: 3.8 },
  { id: 'recall', x: 256, y: 282, w: 100, label: 'recall', at: 4.0 },
  { id: 'f1', x: 478, y: 246, w: 64, label: 'f1', at: 4.5 },
];

interface Edge {
  from: string;
  to: string;
  /** De-emphasised edge (output flowing a long way). */
  soft?: boolean;
}

const EDGES: Edge[] = [
  { from: 'root', to: 'field_count' },
  { from: 'root', to: 'judge' },
  { from: 'judge', to: 'quality' },
  { from: 'judge', to: 'faithfulness' },
  { from: 'root', to: 'precision', soft: true },
  { from: 'root', to: 'recall', soft: true },
  { from: 'expected', to: 'precision' },
  { from: 'expected', to: 'recall' },
  { from: 'precision', to: 'f1' },
  { from: 'recall', to: 'f1' },
];

function byId(id: string): Node {
  const n = NODES.find((n) => n.id === id);
  if (!n) throw new Error(id);
  return n;
}

function edgePath(e: Edge): string {
  const a = byId(e.from);
  const b = byId(e.to);
  const x1 = a.x + a.w;
  const y1 = a.y + H / 2;
  const x2 = b.x;
  const y2 = b.y + H / 2;
  const dx = Math.max(34, (x2 - x1) / 2);
  return `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`;
}

export function MetricsDag() {
  const [runId, setRunId] = useState(0);
  const { ref, holdClass } = useAnimateInView();

  return (
    <div className={`l3-dag${holdClass}`} ref={ref}>
      <div className="l3-dag-head">
        <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[#8A8580]">
          metrics fire as their data arrives
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
        <svg viewBox="0 0 660 330" className="mdg" aria-hidden>
          {/* the metric block, attaching itself around the measurements */}
          <g className="mdg-block" style={{ ['--d' as string]: '0.15s' }}>
            <rect
              x={238}
              y={6}
              width={414}
              height={318}
              rx={12}
              className="mdg-block-rect"
            />
            <text x={252} y={24} className="mdg-block-label">
              metric extract_resume {'{'}
            </text>
            <text x={632} y={318} className="mdg-block-label">
              {'}'}
            </text>
          </g>

          {EDGES.map((e) => {
            const at = byId(e.to).at;
            return (
              <g key={`${e.from}-${e.to}`}>
                <path
                  className={`mdg-edge${e.soft ? ' mdg-edge--soft' : ''}`}
                  d={edgePath(e)}
                />
                <path
                  className="mdg-edge-hot"
                  style={{ ['--d' as string]: `${at - 0.25}s` }}
                  d={edgePath(e)}
                />
              </g>
            );
          })}

          {NODES.map((n) => (
            <g key={n.id}>
              <rect
                x={n.x}
                y={n.y}
                width={n.w}
                height={H}
                rx={9}
                className={`mdg-pend${n.kind ? ` mdg-pend--${n.kind}` : ''}`}
              />
              <rect
                x={n.x}
                y={n.y}
                width={n.w}
                height={H}
                rx={9}
                className={`mdg-fired${n.kind ? ` mdg-fired--${n.kind}` : ''}`}
                style={{ ['--d' as string]: `${n.at}s` }}
              />
              <text
                x={n.x + 12}
                y={n.y + (n.sub ? 16 : 21)}
                className="mdg-label"
              >
                {n.label}
              </text>
              {n.sub ? (
                <text x={n.x + 12} y={n.y + 28} className="mdg-sub">
                  {n.sub}
                </text>
              ) : null}
              <text
                x={n.x + n.w - 9}
                y={n.y + 21}
                textAnchor="end"
                className="mdg-check"
                style={{ ['--d' as string]: `${n.at}s` }}
              >
                ✓
              </text>
            </g>
          ))}
        </svg>
        <p className="l3-dag-event" style={{ animationDelay: '0.4s' }}>
          {'1 · call a baml function'}
        </p>
        <p className="l3-dag-event" style={{ animationDelay: '1.2s' }}>
          {'2 · declared metrics get data automatically'}
        </p>
        <p
          className="l3-dag-event l3-dag-event--later"
          style={{ animationDelay: '3.3s' }}
        >
          {
            '3 · data that arrives later — set_expected(ground_truth), four hours on — fires the rest of the graph'
          }
        </p>
      </div>
    </div>
  );
}
