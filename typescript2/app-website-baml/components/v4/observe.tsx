import type { ReactNode } from 'react';
import { RED } from '../v3/story';
import { Phase, Points } from './phase';

// Section 6, Observe: four dashboard tiles, each drawn from the query printed above it, then the three
// commands that read the same rows.

const CLOVER = 'var(--color-clover)';
const PASTURE = 'var(--color-pasture-200)';

/** Noise in [0, 1) for point i, so the charts draw the same way on every render. */
function noise(i: number, seed = 1) {
  const x = Math.sin(i * 12.9898 + seed * 78.233) * 43758.5453;
  return x - Math.floor(x);
}

const W = 300;
const H = 110;

/** Points spread across the chart, with y in [0, 1] measured up from the bottom. `from` and `to` draw
 *  just part of the line, in the same place it sits in the whole. */
const toPath = (ys: number[], from = 0, to = ys.length - 1) =>
  ys
    .slice(from, to + 1)
    .map(
      (y, k) =>
        `${k ? 'L' : 'M'}${(((from + k) / (ys.length - 1)) * W).toFixed(1)},${(H - y * H).toFixed(1)}`,
    )
    .join('');

const CALLS = Array.from(
  { length: 48 },
  (_, i) => 0.5 + 0.18 * Math.sin(i / 4) + 0.22 * (noise(i) - 0.5),
);

function CallsChart() {
  const line = toPath(CALLS);
  return (
    <svg aria-hidden="true" className="w-full h-auto" viewBox={`0 0 ${W} ${H}`}>
      <path d={`${line}L${W},${H}L0,${H}Z`} fill={CLOVER} opacity={0.12} />
      <path d={line} fill="none" stroke={CLOVER} strokeWidth={1.5} />
    </svg>
  );
}

// Each hour's labels as shares of that hour's calls. `other` grows in the last few hours.
const LABELS = Array.from({ length: 16 }, (_, i) => {
  const other = i < 11 ? 0.012 + 0.006 * noise(i, 2) : 0.03 + 0.012 * (i - 11);
  const bug = 0.24 + 0.06 * noise(i, 3);
  return { billing: 1 - other - bug, bug, other };
});

function LabelsChart() {
  const gap = 4;
  const bw = (W - gap * (LABELS.length - 1)) / LABELS.length;
  return (
    <svg aria-hidden="true" className="w-full h-auto" viewBox={`0 0 ${W} ${H}`}>
      {LABELS.map((l, i) => {
        const x = i * (bw + gap);
        const scale = H * (0.78 + 0.18 * noise(i, 4));
        let y = H;
        return (['billing', 'bug', 'other'] as const).map((k) => {
          const h = l[k] * scale;
          y -= h;
          return (
            <rect
              fill={k === 'other' ? RED : CLOVER}
              height={h}
              key={`${i}-${k}`}
              opacity={k === 'billing' ? 0.6 : k === 'bug' ? 0.28 : 1}
              width={bw}
              x={x}
              y={y}
            />
          );
        });
      })}
    </svg>
  );
}

const LATENCY = [
  { name: 'route', p50: 1.4, p95: 2.9 },
  { name: 'classify', p50: 1.3, p95: 2.4 },
];

function LatencyChart() {
  return (
    <div className="grid gap-4 font-mono text-[12px] min-h-[110px] content-center">
      {LATENCY.map((f) => (
        <div key={f.name}>
          <p className="text-white/75">{f.name}</p>
          {(
            [
              ['p50', f.p50, 'bg-clover/60'],
              ['p95', f.p95, 'bg-clover/25'],
            ] as const
          ).map(([label, value, fill]) => (
            <div className="mt-1 flex items-center gap-2" key={label}>
              <span className="flex-1 min-w-0">
                <span
                  className={`block h-2.5 rounded-[2px] ${fill}`}
                  style={{ width: `${(value / 3) * 100}%` }}
                />
              </span>
              <span className="w-[68px] shrink-0 text-white/55">
                {label} {value}s
              </span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

// The share labelled other, against a 2% threshold. The y axis runs to 8%.
const SHARE = LABELS.map((l) => l.other / 0.08);
const THRESHOLD = 0.02 / 0.08;

function ShareChart() {
  const ty = H - THRESHOLD * H;
  const cross = SHARE.findIndex((s) => s > THRESHOLD);
  const cx = (cross / (SHARE.length - 1)) * W;
  return (
    <svg aria-hidden="true" className="w-full h-auto" viewBox={`0 0 ${W} ${H}`}>
      <rect fill={RED} height={H} opacity={0.08} width={W - cx} x={cx} y={0} />
      <line
        stroke="white"
        strokeDasharray="4 4"
        strokeOpacity={0.45}
        x1={0}
        x2={W}
        y1={ty}
        y2={ty}
      />
      <text fill="white" fillOpacity={0.5} fontSize={10} x={4} y={ty - 5}>
        2%
      </text>
      <path
        d={toPath(SHARE, 0, cross - 1)}
        fill="none"
        stroke={PASTURE}
        strokeWidth={1.5}
      />
      <path
        d={toPath(SHARE, cross - 1)}
        fill="none"
        stroke={RED}
        strokeWidth={1.5}
      />
    </svg>
  );
}

function Tile({
  chart,
  footer,
  query,
  title,
}: {
  chart: ReactNode;
  footer?: ReactNode;
  query: string;
  title: string;
}) {
  return (
    <figure className="flex flex-col rounded-xl border border-white/12 bg-[#0b0b0c] p-5">
      <figcaption className="font-display font-medium text-[16px] text-white">
        {title}
      </figcaption>
      <p className="mt-2 font-mono text-[11.5px] leading-[1.6] text-clover/90 break-words">
        {query}
      </p>
      <div className="mt-auto pt-5">{chart}</div>
      {footer && (
        <div className="mt-3 font-mono text-[11.5px] text-white/45">
          {footer}
        </div>
      )}
    </figure>
  );
}

const POINTS = [
  {
    body: 'Every call is stored with its typed arguments, its result, its latency, and the version of the code that ran.',
    command: 'baml query',
    title: 'Look up any call.',
  },
  {
    body: 'A dashboard is a query that keeps running.',
    command: 'baml dashboard',
    title: 'Chart any query, live.',
  },
  {
    body: "An alert is a query with a threshold, and it fires on the function it's about, next to its code.",
    command: 'baml alerts',
    title: 'Alert on any query.',
  },
];

export default function Observe() {
  return (
    <Phase
      heading="Create dashboards and alerts from your data."
      id="observe"
      intro="Your calls land in Boundary Cloud as typed rows. Read them with SQL from the terminal, chart them, or put a threshold on them."
      name="Observe"
    >
      <div className="grid sm:grid-cols-2 xl:grid-cols-4 gap-4">
        <Tile
          chart={<CallsChart />}
          footer="last 4 hours"
          query="SELECT count(*) FROM spans GROUP BY minute"
          title="Calls per minute"
        />
        <Tile
          chart={<LabelsChart />}
          footer={
            <span className="flex gap-4">
              <span className="text-clover">billing</span>
              <span className="text-clover/60">bug</span>
              <span style={{ color: RED }}>other</span>
            </span>
          }
          query="SELECT output_value, count(*) FROM spans WHERE span_name = 'classify' GROUP BY hour, output_value"
          title="Labels"
        />
        <Tile
          chart={<LatencyChart />}
          query="SELECT span_name, p50(duration), p95(duration) FROM spans GROUP BY span_name"
          title="Latency by function"
        />
        <Tile
          chart={<ShareChart />}
          footer={
            <span className="flex items-center gap-2" style={{ color: RED }}>
              <span className="size-1.5 rounded-full bg-current" />
              alert firing on classify
            </span>
          }
          query="SELECT avg(output_value = 'other') FROM spans WHERE span_name = 'classify' GROUP BY hour"
          title="Share labelled other"
        />
      </div>
      <div className="mt-12 md:mt-16">
        <Points columns={3} points={POINTS} />
      </div>
    </Phase>
  );
}
