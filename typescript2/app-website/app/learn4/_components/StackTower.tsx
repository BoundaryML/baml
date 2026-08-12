'use client';

import { useState } from 'react';
import { useAnimateInView } from '../../learn3/_lib/use-animate-in-view';

interface Layer {
  name: string;
  who: string;
  /** Final resting tilt — the tower slumps into this on animation. */
  tilt?: string;
  /** Extra gap below this layer (the tower pulls apart near the top). */
  gap?: number;
}

// Top-first. The base is steady; everything stacked above it has drifted.
const LAYERS: Layer[] = [
  {
    name: 'evals & dashboards',
    who: 'promptfoo · braintrust',
    tilt: 'rotate(-2.6deg) translateX(-14px)',
    gap: 30,
  },
  {
    name: 'observability',
    who: 'langsmith · otel glue',
    tilt: 'rotate(1.9deg) translateX(11px)',
    gap: 27,
  },
  {
    name: 'orchestration',
    who: 'langchain · ai sdk',
    tilt: 'rotate(-1.3deg) translateX(-6px)',
    gap: 24,
  },
  {
    name: 'model SDKs',
    who: 'openai · anthropic',
    tilt: 'rotate(0.8deg) translateX(4px)',
    gap: 22,
  },
  {
    name: 'language',
    who: 'python 1991 · typescript 2012',
    tilt: 'rotate(-0.3deg)',
    gap: 20,
  },
  { name: 'runtimes', who: 'cpython · node · v8 · bun · deno' },
];

// What falls into the cracks, one annotation per seam (top-first).
const SEAMS = [
  'bound to code by strings',
  'traces leave your types behind',
  'a graph you maintain by hand',
  'json in, any out',
  'types erased at runtime',
];

const BAML_COLUMN: Layer[] = [
  { name: 'syntax & compiler', who: 'one dialect' },
  { name: 'BEX runtime', who: 'the execution engine' },
  { name: 'AI stdlib', who: 'model calls are functions' },
  { name: 'tests & evals', who: 'test · testset · asserts' },
  { name: 'observability', who: 'traces · Boundary Studio' },
  { name: 'tooling', who: 'describe · playground · pack' },
  { name: 'embeds', who: 'python · typescript · wasm' },
];

/**
 * The stack as a hand-drawn tower that slumps as it animates: the base holds,
 * the upper layers drift crooked, and annotations point at what falls into
 * the seams. Next to it, the single BAML column — same sketchbook style,
 * standing straight. CSS keyframes only; replay remounts.
 */
export function StackTower() {
  const [runId, setRunId] = useState(0);
  const { ref, holdClass } = useAnimateInView();
  const seamStart = 0.5 + LAYERS.length * 0.28;

  return (
    <div key={runId} className={`l4-stack${holdClass}`} ref={ref}>
      <div>
        <div className="l4-stack-col-head">
          <span className="l4-stack-col-title">the stack today</span>
          <button
            type="button"
            className="l2-btn"
            onClick={() => setRunId((n) => n + 1)}
          >
            Replay ↻
          </button>
        </div>
        <div className="l4-tower">
          {LAYERS.map((layer, i) => (
            <div key={layer.name}>
              <div
                className={`l4-layer${layer.tilt ? ' l4-layer--slump' : ''}${
                  i % 2 ? ' l4-sketch-b' : ' l4-sketch-a'
                }`}
                style={
                  layer.tilt
                    ? ({
                        ['--tilt' as string]: layer.tilt,
                        // the base settles first, the top slumps last
                        animationDelay: `${0.5 + (LAYERS.length - 1 - i) * 0.28}s`,
                      } as React.CSSProperties)
                    : undefined
                }
              >
                <span className="l4-layer-name">{layer.name}</span>
                <span className="l4-layer-who">{layer.who}</span>
              </div>
              {i < SEAMS.length ? (
                <div
                  className="l4-seam"
                  style={{
                    height: layer.gap,
                    animationDelay: `${seamStart + i * 0.4}s`,
                  }}
                >
                  <span
                    className="l4-seam-label"
                    style={{
                      transform: `rotate(${i % 2 ? 0.8 : -0.9}deg)`,
                    }}
                  >
                    {SEAMS[i]}
                  </span>
                </div>
              ) : null}
            </div>
          ))}
        </div>
      </div>
      <div>
        <div className="l4-stack-col-head">
          <span className="l4-stack-col-title">baml</span>
        </div>
        <div className="l4-baml-col l4-sketch-a">
          {BAML_COLUMN.map((row) => (
            <div key={row.name} className="l4-baml-row">
              <span className="l4-baml-row-name">{row.name}</span>
              <span className="l4-baml-row-what">{row.who}</span>
            </div>
          ))}
        </div>
      </div>
      <p className="l4-stack-caption">
        {
          'One toolchain, open at both ends: it embeds into the Python or TypeScript app you already have, and pack ships a plain binary.'
        }
      </p>
    </div>
  );
}
