import { SKILL_RESULTS } from './skill-results-data';

const CONDITIONS = [
  { color: '#8a8a86', name: 'No skill' },
  { color: 'var(--color-clover-500)', name: 'Old skill' },
  { color: 'var(--color-clover-300)', name: 'New skill' },
  { color: 'var(--color-clover-100)', name: 'New skill + codequality' },
];

const TASKS = ['Regex', 'Chess', 'Conduit', 'GTFS'] as const;

type Metric = keyof typeof SKILL_RESULTS;

// NPath uses medians on a log scale, because a few runs reach values billions
// of times larger than the rest.
const METRICS: Record<Metric, { log?: boolean; name: string }> = {
  cognitive: { name: 'Cognitive complexity' },
  cyclomatic: { name: 'Cyclomatic complexity' },
  exits: { name: 'Exits' },
  nesting: { name: 'Nesting' },
  npath: { log: true, name: 'NPath' },
};

const compact = new Intl.NumberFormat('en', {
  maximumFractionDigits: 0,
  notation: 'compact',
});

const mean = (values: number[]) =>
  values.reduce((sum, value) => sum + value, 0) / values.length;

const median = (values: number[]) => {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = sorted.length / 2;
  return Number.isInteger(middle)
    ? mean(sorted.slice(middle - 1, middle + 1))
    : (sorted[Math.floor(middle)] ?? 0);
};

const DOT_PERCENT = 2.5;
const LANE_OFFSETS = [0, -4, 4, -8, 8];

// Moves dots that would overlap into a free lane of the row. Values are sorted,
// so only each lane's last dot can collide.
function laneOffsets(positions: number[]): number[] {
  const lastInLane = LANE_OFFSETS.map(() => Number.NEGATIVE_INFINITY);
  return positions.map((position) => {
    const free = lastInLane.findIndex((last) => position - last >= DOT_PERCENT);
    const lane = free === -1 ? positions.indexOf(position) % 5 : free;
    lastInLane[lane] = position;
    return LANE_OFFSETS[lane] ?? 0;
  });
}

// Round axis values from 0 that end at or above max.
function linearTicks(max: number): number[] {
  const rough = max / 4;
  const magnitude = 10 ** Math.floor(Math.log10(rough));
  const step =
    [1, 2, 5, 10].map((n) => n * magnitude).find((n) => n >= rough) ?? rough;
  return Array.from(
    { length: Math.ceil(max / step) + 1 },
    (_, index) => index * step,
  );
}

// Powers of ten from 1 that end at or above max.
function logTicks(max: number): number[] {
  const top = Math.ceil(Math.log10(max));
  const step = Math.ceil(top / 4);
  return Array.from(
    { length: Math.ceil(top / step) + 1 },
    (_, index) => 10 ** (index * step),
  );
}

const tickLabel = (tick: number) =>
  tick >= 1e15 ? tick.toExponential(0) : compact.format(tick);

function Legend() {
  return (
    <ul className="mb-3 flex flex-wrap gap-x-5 gap-y-1.5 text-[13px] text-ink-2">
      {CONDITIONS.map(({ color, name }) => (
        <li className="flex items-center gap-2" key={name}>
          <span
            aria-hidden
            className="size-2.5 rounded-full"
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
export function SkillResultsChart({ metric }: { metric: Metric }) {
  const definition = METRICS[metric];
  if (!definition) return null;
  const { log, name } = definition;
  const average = log ? median : mean;
  const rounded = (value: number) =>
    log
      ? compact.format(value)
      : String(
          Number(
            value.toFixed(metric === 'nesting' || metric === 'exits' ? 1 : 0),
          ),
        );

  return (
    <figure className="not-prose my-5">
      <figcaption className="mb-2 text-sm font-medium text-ink">
        {name} of the worst function in each run, tick at the{' '}
        {log ? 'median' : 'mean'}
        {log && ', log scale'}. Lower scores are simpler by this metric.
      </figcaption>
      <Legend />
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {TASKS.map((task) => {
          const rows: readonly number[][] = SKILL_RESULTS[metric][task];
          const max = Math.max(...rows.flat());
          const ticks = log ? logTicks(max) : linearTicks(max);
          const top = ticks.at(-1) ?? 1;
          const position = (value: number) =>
            log
              ? (Math.log10(Math.max(value, 1)) / Math.log10(top)) * 100
              : (value / top) * 100;
          return (
            <div
              className="rounded-lg border border-white/10 bg-white/[0.03] p-3"
              key={task}
            >
              <div className="mb-2 text-xs text-ink-2">{task}</div>
              <div
                aria-label={`${task}, ${name} of the worst function: ${rows
                  .map(
                    (values, index) =>
                      `${CONDITIONS[index]?.name} ${rounded(average(values))}`,
                  )
                  .join(', ')}`}
                className="relative mx-2"
                role="img"
              >
                {ticks.map((tick) => (
                  <div
                    className="absolute inset-y-0 w-px bg-white/15"
                    key={tick}
                    style={{ left: `${position(tick)}%` }}
                  />
                ))}
                {rows.map((values, index) => {
                  const condition = CONDITIONS[index];
                  const positions = values.map(position);
                  const offsets = laneOffsets(positions);
                  return (
                    <div className="relative h-6" key={condition?.name}>
                      {values.map((value, run) => (
                        <span
                          className="group absolute top-1/2 size-3.5 -translate-x-1/2 -translate-y-1/2 hover:z-10"
                          // biome-ignore lint/suspicious/noArrayIndexKey: runs have no identity
                          key={run}
                          style={{
                            left: `${positions[run]}%`,
                            marginTop: offsets[run],
                          }}
                        >
                          <span
                            className="absolute inset-1 rounded-full opacity-85 group-hover:opacity-100"
                            style={{ background: condition?.color }}
                          />
                          <span
                            className={`absolute bottom-full z-10 mb-1 hidden whitespace-nowrap rounded-md border border-white/10 bg-[#17191b] px-2 py-1 text-xs text-ink-2 group-hover:block ${
                              (positions[run] ?? 0) > 50 ? 'right-0' : 'left-0'
                            }`}
                          >
                            {condition?.name}:{' '}
                            <span className="text-ink">
                              {value.toLocaleString('en')}
                            </span>
                          </span>
                        </span>
                      ))}
                      <span
                        className="pointer-events-none absolute inset-y-0.5 w-0.5 -translate-x-1/2 bg-ink"
                        style={{ left: `${position(average(values))}%` }}
                      />
                    </div>
                  );
                })}
              </div>
              <div className="relative mx-2 mt-1 h-4 text-[11px] text-ink-2 tabular-nums">
                {ticks.map((tick) => (
                  <span
                    className="absolute -translate-x-1/2"
                    key={tick}
                    style={{ left: `${position(tick)}%` }}
                  >
                    {tickLabel(tick)}
                  </span>
                ))}
              </div>
            </div>
          );
        })}
      </div>
    </figure>
  );
}
