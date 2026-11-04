import { Panel, Phase } from './phase';

// Section 7, Improve: a fix goes out to a share of traffic before everyone. One chart shows v44, the
// fix, taking over from v43 in three steps over three days.

const CLOVER = 'var(--color-clover)';

// Hours across the chart, Monday 00:00 to Thursday 00:00.
const HOURS = 72;
// v44's share of traffic, as [hour, share] corners. Each step ramps up over an hour and a half.
const ROLLOUT: [number, number][] = [
  [0, 0],
  [10, 0],
  [11.5, 0.1],
  [34, 0.1],
  [35.5, 0.5],
  [58, 0.5],
  [59.5, 1],
  [HOURS, 1],
];

const W = 720;
const H = 200;
const x = (hour: number) => (hour / HOURS) * W;
const y = (share: number) => H - share * H;

const EDGE = ROLLOUT.map(
  ([h, s], i) => `${i ? 'L' : 'M'}${x(h).toFixed(1)},${y(s).toFixed(1)}`,
).join('');

// The step labels sit just above each step of v44's area, in the middle of the step.
const STEPS = [
  { hour: 22.75, label: '10%', share: 0.1 },
  { hour: 46.75, label: '50%', share: 0.5 },
  { hour: 65.75, label: '100%', share: 1 },
];

const DAYS = ['Mon', 'Tue', 'Wed'];

const VERSIONS = [
  {
    name: 'v43',
    note: '3.1% labelled other',
    swatch: 'rgb(from var(--color-ink) r g b / 0.16)',
  },
  { name: 'v44, the fix', note: '0.3% labelled other', swatch: CLOVER },
];

function RolloutChart() {
  return (
    <div className="p-5 md:p-6">
      <div className="flex flex-wrap gap-x-8 gap-y-2 text-[13px] md:text-[14px]">
        {VERSIONS.map((v) => (
          <p className="flex items-center gap-2.5" key={v.name}>
            <span
              className="size-2.5 rounded-[2px]"
              style={{ background: v.swatch }}
            />
            <span className="text-ink">{v.name}</span>
            <span className="text-muted">{v.note}</span>
          </p>
        ))}
      </div>

      <div className="mt-6 grid grid-cols-[auto_minmax(0,1fr)] gap-x-3">
        {/* Share of traffic, recessive: just the ends and the middle, as labels with no gridlines. */}
        <div className="relative w-8 h-44 md:h-56 font-mono text-[11px] text-muted">
          {['100%', '50%', '0'].map((label, i) => (
            <span
              className="absolute right-0 -translate-y-1/2"
              key={label}
              style={{ top: `${i * 50}%` }}
            >
              {label}
            </span>
          ))}
        </div>

        <div className="relative h-44 md:h-56">
          <svg
            aria-hidden="true"
            className="absolute inset-0 size-full"
            preserveAspectRatio="none"
            viewBox={`0 0 ${W} ${H}`}
          >
            <path
              d={`${EDGE}L${W},0L0,0Z`}
              fill="rgb(from var(--color-ink) r g b / 0.16)"
            />
            <path
              d={`${EDGE}L${W},${H}L0,${H}Z`}
              fill={CLOVER}
              opacity={0.55}
            />
            {/* A gap in the page's surface colour between the two areas. */}
            <path
              d={EDGE}
              fill="none"
              stroke="var(--color-raised)"
              strokeWidth={2}
              vectorEffect="non-scaling-stroke"
            />
          </svg>

          <span className="absolute left-3 top-3 text-[13px] text-ink">
            v43
          </span>
          {STEPS.map((s) => (
            <span
              className="absolute -translate-x-1/2 font-mono text-[12px] md:text-[13px] text-ink"
              key={s.label}
              style={{
                bottom:
                  s.share === 1
                    ? undefined
                    : `calc(${s.share * 100}% + 0.4rem)`,
                left: `${(s.hour / HOURS) * 100}%`,
                top: s.share === 1 ? '0.75rem' : undefined,
              }}
            >
              {s.label}
            </span>
          ))}
        </div>

        <div />
        <div className="relative h-6 mt-2 font-mono text-[11px] text-muted">
          {DAYS.map((d, i) => (
            <span
              className="absolute top-0"
              key={d}
              style={{ left: `${(i / DAYS.length) * 100}%` }}
            >
              {d}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

export default function Improve() {
  return (
    <Phase
      heading="Test and roll out fixes on real traffic."
      id="improve"
      intro="Ship a fix to a small share of traffic first, watch how it does on real calls, and raise its share as it proves itself."
      name="Improve"
      soon
    >
      <Panel title="v44's share of traffic">
        <RolloutChart />
      </Panel>
    </Phase>
  );
}
