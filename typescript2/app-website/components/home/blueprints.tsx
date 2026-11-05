'use client';

import { useId } from 'react';
import { ramp } from './scroll';

// Section 2's drawing, driven by scroll. Raptor 1 is on the sheet first; Raptor 2 and Raptor 3 join it,
// each with fewer external parts than the last. Then the engines give way to the BAML stack, which
// builds from the virtual machine up, one layer at a time. `progress` runs from 0 to 1 over the
// section; at 1 the stack is whole.

const RAPTORS_ALT =
  'Raptor 1, Raptor 2, and Raptor 3 engines side by side, drawn as a blueprint. Each one has fewer external parts than the last.';
const STACK_ALT =
  'The layers of the BAML stack drawn as a blueprint, from the virtual machine at the bottom to dashboards, alerts, and feature flags at the top.';

// The three engines' columns on the 960-wide sheet, as clip insets from the left and right edges.
// Raptor 1 is there from the start.
const ENGINES = [
  { from: 0, inset: '0 63.5% 0 0', to: 0 },
  { from: 0.02, inset: '0 36.5% 0 36.5%', to: 0.14 },
  { from: 0.13, inset: '0 0 0 63.5%', to: 0.25 },
];
// The engines leave while the first layers of the stack arrive.
const RAPTORS_OUT = [0.36, 0.48] as const;
const FIRST_LAYER = 0.38;
const LAYER_STEP = 0.07;
const LAYER_RAMP = 0.1;
// How far a layer falls as it arrives, in sheet units.
const DROP = 44;

// The stack, bottom to top. Each layer is an isometric slab whose top face starts at `y`.
const LAYERS = [
  'VIRTUAL MACHINE',
  'COMPILER',
  'LANGUAGE',
  'TESTS',
  'DATA LAKE',
  'QUERY',
  'DASHBOARDS, ALERTS, FLAGS',
].map((title, i) => ({
  thick: i === 0 ? 16 : 12,
  title,
  y: 549 - 80 * i,
}));

type Slab = { thick: number; y: number };

/** The slab's outline, used to hide the lines of lower slabs behind it. */
const silhouette = ({ thick: d, y }: Slab) =>
  `M272,${y}L424,${y + 45}L424,${y + 45 + d}L272,${y + 90 + d}L120,${y + 45 + d}L120,${y + 45}Z`;

/** How far above its resting place a layer sits when it is `q` of the way in. */
const drop = (q: number) => -(1 - q) * DROP;

/** The BAML stack as isometric slabs. `shown` holds each layer's arrival, from 0 to 1. */
function Stack({ shown }: { shown: number[] }) {
  const id = useId();
  return (
    <svg
      aria-label={STACK_ALT}
      className="block w-full h-auto"
      role="img"
      viewBox="0 0 960 720"
    >
      <defs>
        {LAYERS.map((layer, i) => (
          <mask
            height="720"
            id={`${id}-${i}`}
            key={layer.title}
            maskUnits="userSpaceOnUse"
            width="960"
            x="0"
            y="0"
          >
            <rect fill="#fff" height="720" width="960" />
            {LAYERS.slice(i + 1).map((above, k) => {
              const q = shown[i + 1 + k];
              return q > 0 ? (
                <path
                  d={silhouette(above)}
                  fill="#000"
                  fillOpacity={q}
                  key={above.title}
                  transform={`translate(0 ${drop(q)})`}
                />
              ) : null;
            })}
          </mask>
        ))}
      </defs>
      <g
        fill="none"
        stroke="#fff"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="1"
      >
        {LAYERS.map((layer, i) => {
          const q = shown[i];
          if (q <= 0) return null;
          const { thick: d, y } = layer;
          const mid = i === 0 ? 602 : y + 51;
          return (
            <g key={layer.title} opacity={q}>
              <g
                mask={`url(#${id}-${i})`}
                transform={`translate(0 ${drop(q)})`}
              >
                <path
                  d={`M272,${y}L424,${y + 45}L272,${y + 90}L120,${y + 45}Z`}
                  fill="#fff"
                  fillOpacity="0.06"
                  stroke="none"
                />
                <path
                  d={`M120,${y + 45}L272,${y + 90}L272,${y + 90 + d}L120,${y + 45 + d}Z`}
                  fill="#fff"
                  fillOpacity="0.13"
                  stroke="none"
                />
                <path
                  d={`M272,${y + 90}L424,${y + 45}L424,${y + 45 + d}L272,${y + 90 + d}Z`}
                  fill="#fff"
                  fillOpacity="0.08"
                  stroke="none"
                />
                <path
                  d={`M272,${y}L424,${y + 45}L272,${y + 90}L120,${y + 45}ZM120,${y + 45}V${y + 45 + d}L272,${y + 90 + d}L424,${y + 45 + d}V${y + 45}`}
                  strokeOpacity="0.92"
                  strokeWidth="1.4"
                />
                <path d={`M272,${y + 90}V${y + 90 + d}`} strokeOpacity="0.6" />
              </g>
              <path d={`M424,${mid}H518`} strokeOpacity="0.6" />
              <text
                fill="#fff"
                fontFamily="ui-monospace,SFMono-Regular,Menlo,Consolas,monospace"
                fontSize="17"
                letterSpacing="2.5"
                stroke="none"
                x="530"
                y={mid + 5.5}
              >
                {layer.title}
              </text>
            </g>
          );
        })}
      </g>
    </svg>
  );
}

/** Section 2's drawing: the three Raptor engines, then the BAML stack, at a given scroll progress. */
export default function Blueprints({ progress }: { progress: number }) {
  const out = 1 - ramp(progress, ...RAPTORS_OUT);
  const layers = LAYERS.map((_, i) => {
    const from = FIRST_LAYER + i * LAYER_STEP;
    return ramp(progress, from, from + LAYER_RAMP);
  });
  const stackShown = layers[0] > 0;

  return (
    <figure className="v4-blueprint-sheet grid rounded-2xl overflow-hidden border border-rule">
      {ENGINES.map((e, k) => {
        const q = (e.to > e.from ? ramp(progress, e.from, e.to) : 1) * out;
        return (
          // biome-ignore lint/performance/noImgElement: a static SVG drawing, shown at its own size
          <img
            alt={k === 0 && !stackShown ? RAPTORS_ALT : ''}
            aria-hidden={k !== 0 || stackShown}
            className="col-start-1 row-start-1 block w-full h-auto"
            height={720}
            key={e.inset}
            src="/home/raptors.svg"
            style={{
              clipPath: `inset(${e.inset})`,
              opacity: q,
              transform: `translateY(${(1 - q) * 10}px)`,
            }}
            width={960}
          />
        );
      })}
      <div aria-hidden={!stackShown} className="col-start-1 row-start-1">
        <Stack shown={layers} />
      </div>
    </figure>
  );
}
