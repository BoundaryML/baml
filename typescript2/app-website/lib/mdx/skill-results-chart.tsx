'use client';

import {
  Bar,
  BarChart,
  Cell,
  LabelList,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

const CONDITIONS = [
  { color: '#5c5c5a', name: 'No skill' },
  { color: 'var(--color-clover-600)', name: 'Old skill' },
  { color: 'var(--color-clover-400)', name: 'New skill' },
  { color: 'var(--color-clover-200)', name: 'New skill + codequality' },
];

const TASKS = ['Regex', 'Chess', 'Conduit', 'GTFS'] as const;
type Task = (typeof TASKS)[number];
type Values = Record<Task, number[]>;

// Means of ten runs, except NPath: medians on a log scale, because a few runs
// reach values billions of times larger than the rest.
const METRICS: Record<string, { log?: boolean; name: string; values: Values }> =
  {
    cognitive: {
      name: 'Cognitive complexity',
      values: {
        Chess: [134, 43, 29, 14],
        Conduit: [145, 65, 16, 10],
        GTFS: [50, 30, 28, 9],
        Regex: [70, 39, 28, 17],
      },
    },
    cyclomatic: {
      name: 'Cyclomatic complexity',
      values: {
        Chess: [46, 28, 20, 13],
        Conduit: [69, 47, 26, 19],
        GTFS: [29, 14, 22, 11],
        Regex: [34, 32, 34, 28],
      },
    },
    exits: {
      name: 'Exits',
      values: {
        Chess: [12.4, 5, 8.4, 5.5],
        Conduit: [24, 15.1, 6.7, 5],
        GTFS: [5.5, 4.7, 6.6, 5.5],
        Regex: [9.8, 6, 4, 3.9],
      },
    },
    nesting: {
      name: 'Nesting',
      values: {
        Chess: [6.6, 5.3, 3.9, 2.8],
        Conduit: [5.8, 4.5, 3.2, 3],
        GTFS: [5.3, 4.8, 3.7, 3.1],
        Regex: [5.2, 4.5, 4.3, 3.5],
      },
    },
    npath: {
      log: true,
      name: 'NPath',
      values: {
        Chess: [2154060, 5654, 2592, 158],
        Conduit: [39132823680, 361117440, 200, 67],
        GTFS: [195360, 315, 33360, 117],
        Regex: [114048, 2376, 242, 35],
      },
    },
  };

const compact = new Intl.NumberFormat('en', {
  maximumFractionDigits: 0,
  notation: 'compact',
});

interface Point {
  condition: string;
  height: number;
  label: string;
  value: number;
}

function ChartTooltip({
  active,
  payload,
}: {
  active?: boolean;
  payload?: { payload: Point }[];
}) {
  const point = payload?.[0]?.payload;
  if (!active || !point) return null;
  return (
    <div className="rounded-md border border-hair bg-[#17191b] px-2.5 py-1.5 text-xs text-ink-2">
      {point.condition}:{' '}
      <span className="text-ink">{point.value.toLocaleString('en')}</span>
    </div>
  );
}

export function SkillResultsLegend() {
  return (
    <ul className="not-prose my-5 flex flex-wrap gap-x-5 gap-y-1.5 text-[13px] text-ink-2">
      {CONDITIONS.map(({ color, name }) => (
        <li className="flex items-center gap-2" key={name}>
          <span
            aria-hidden
            className="size-2.5 rounded-sm"
            style={{ background: color }}
          />
          {name}
        </li>
      ))}
    </ul>
  );
}

// Each panel has its own scale: the question is how far each task drops, not
// how the tasks compare, and GTFS would look flat beside Conduit otherwise.
export function SkillResultsChart({
  metric,
}: {
  metric: keyof typeof METRICS;
}) {
  const definition = METRICS[metric];
  if (!definition) return null;
  const { log, name, values } = definition;
  return (
    <figure className="not-prose my-5">
      <figcaption className="mb-2 text-sm font-medium text-ink">
        {name} of the worst function, {log ? 'median' : 'mean'} of 10 runs
        {log && ', log scale'}
      </figcaption>
      <div className="grid grid-cols-2 gap-x-6 gap-y-3 sm:grid-cols-4">
        {TASKS.map((task) => {
          const data: Point[] = values[task].map((value, index) => ({
            condition: CONDITIONS[index]?.name ?? '',
            height: log ? Math.log10(value) : value,
            label: log ? compact.format(value) : String(value),
            value,
          }));
          return (
            <div
              aria-label={`${task}, ${name} of worst function: ${data
                .map(({ condition, value }) => `${condition} ${value}`)
                .join(', ')}`}
              key={task}
              role="img"
            >
              <div className="mb-1 text-xs text-ink-2">{task}</div>
              <ResponsiveContainer height={100} width="100%">
                <BarChart
                  barCategoryGap={2}
                  data={data}
                  margin={{ bottom: 0, left: 0, right: 0, top: 16 }}
                >
                  <XAxis dataKey="condition" hide />
                  <YAxis
                    domain={[0, Math.max(...data.map((d) => d.height))]}
                    hide
                  />
                  <Tooltip
                    content={<ChartTooltip />}
                    cursor={{ fill: 'rgba(255, 255, 255, 0.05)' }}
                    isAnimationActive={false}
                  />
                  <Bar
                    dataKey="height"
                    isAnimationActive={false}
                    radius={[3, 3, 0, 0]}
                  >
                    {data.map(({ condition }, index) => (
                      <Cell fill={CONDITIONS[index]?.color} key={condition} />
                    ))}
                    <LabelList
                      dataKey="label"
                      fill="var(--ink-2)"
                      fontSize={11}
                      position="top"
                    />
                  </Bar>
                </BarChart>
              </ResponsiveContainer>
            </div>
          );
        })}
      </div>
    </figure>
  );
}
