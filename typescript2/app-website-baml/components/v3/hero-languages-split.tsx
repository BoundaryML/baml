'use client';

import {
  type ReactNode,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import {
  Beam,
  Card,
  Check,
  Cross,
  Frame,
  Lamb,
  RED,
  useCycle,
} from '../diagram-styles/kit';
import { Commands } from './hero-languages-styled';

// Variations on "Languages, styled" where deploy is a line, not a stage. Build and Test sit on the left of
// it and production on the right. Other toolchains stop at the line; BAML's row crosses it into a panel
// that shows what production is doing, and a line runs from that panel back to Build. Each variation
// fills the panel differently. All of them take a style from /diagram-styles, and all data is example data.

const ROWS: {
  commands: string[];
  name: string;
  short?: string;
  year: string;
}[] = [
  {
    commands: ['tsc', 'vitest'],
    name: 'TypeScript',
    short: 'TS',
    year: '2012',
  },
  {
    commands: ['go build', 'go test'],
    name: 'Go',
    year: '2009',
  },
  {
    commands: ['mypy', 'pytest'],
    name: 'Python',
    year: '1991',
  },
];

const GRID =
  'grid grid-cols-[48px_minmax(0,1fr)_minmax(0,1fr)_14px_minmax(0,2.8fr)] @min-[640px]:grid-cols-[120px_minmax(0,1fr)_minmax(0,1fr)_40px_minmax(0,2fr)] gap-x-1.5 @min-[640px]:gap-x-2';
const ROW_HEIGHT = 'h-11 @min-[640px]:h-12';
const PANEL_HEIGHT = 'h-32 @min-[640px]:h-40 @min-[960px]:h-48';
const TALL_PANEL_HEIGHT = 'h-40 @min-[640px]:h-52 @min-[960px]:h-56';
// Mixed into the card color rather than laid over it, so a panel that grows over other cells hides them.
const TINT = {
  backgroundColor: 'color-mix(in srgb, var(--ds-accent) 10%, var(--ds-card))',
};
const MUTED = 'text-(--ds-ink)/50';

/** A deterministic series in [0, 1], so every render draws the same chart. */
function series(n: number, seed: number) {
  return Array.from({ length: n }, (_, i) => {
    const x = Math.sin((i + 1) * 12.9898 + seed * 78.233) * 43758.5453;
    const noise = x - Math.floor(x);
    return 0.45 + 0.25 * Math.sin(i / 3 + seed) + 0.3 * (noise - 0.5);
  });
}

function linePath(values: number[]) {
  return values
    .map((v, i) => {
      const x = (i / (values.length - 1)) * 100;
      return `${i ? 'L' : 'M'}${x.toFixed(2)} ${(30 - v * 28).toFixed(2)}`;
    })
    .join('');
}

/** A line chart that stretches to its box, with a pulse on the newest point. */
function Spark({
  color = 'var(--ds-accent)',
  values,
}: {
  color?: string;
  values: number[];
}) {
  const last = values.at(-1) ?? 0;
  return (
    <div className="relative size-full">
      <svg
        aria-hidden="true"
        className="absolute inset-0 size-full overflow-visible"
        fill="none"
        preserveAspectRatio="none"
        viewBox="0 0 100 30"
      >
        <path
          d={linePath(values)}
          stroke={color}
          strokeLinejoin="round"
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
      <span
        className="absolute right-0 size-1.5 translate-x-1/2 translate-y-1/2 rounded-full"
        style={{ background: color, bottom: `${(last * 28 * 100) / 30}%` }}
      >
        <span
          className="ds-ping absolute inset-0 rounded-full"
          style={{ background: color }}
        />
      </span>
    </div>
  );
}

/** Vertical bars along the bottom of their box. Bars listed in `failed` are drawn in red. */
function Bars({
  failed = [],
  values,
}: {
  failed?: number[];
  values: number[];
}) {
  return (
    <div className="flex items-end gap-px size-full">
      {values.map((v, i) => (
        <span
          className="flex-1 min-w-0 rounded-t-[1px]"
          // biome-ignore lint/suspicious/noArrayIndexKey: a bar is a fixed slot in the chart
          key={i}
          style={{
            background: failed.includes(i)
              ? RED
              : 'rgb(from var(--ds-accent) r g b / 0.55)',
            height: `${Math.max(8, v * 100)}%`,
          }}
        />
      ))}
    </div>
  );
}

function Tile({
  children,
  className = '',
  label,
  value,
}: {
  children: ReactNode;
  className?: string;
  label: string;
  value: string;
}) {
  return (
    <div
      className={`flex flex-col min-w-0 p-2 @min-[960px]:p-3 border border-(--ds-line) rounded-[max(0px,calc(var(--ds-card-radius)_-_4px))] ${className}`}
    >
      <p className={`text-[10.5px] @min-[640px]:text-[12px] ${MUTED} truncate`}>
        {label}
      </p>
      <p className="font-display font-medium text-[15px] @min-[640px]:text-[20px] @min-[960px]:text-[24px] leading-tight tracking-[-0.02em]">
        {value}
      </p>
      <div className="flex-1 min-h-0 mt-1.5">{children}</div>
    </div>
  );
}

const CALLS = series(28, 1);
const LATENCY = series(18, 4).map((v) => v * 0.8);
const ERRORS = series(20, 7).map((v) => v * 0.35);

/** Three metric tiles: call rate, failures, and latency. On a phone only the first shows. */
function Metrics() {
  return (
    <div className="grid grid-cols-1 @min-[560px]:grid-cols-3 gap-1.5 @min-[640px]:gap-2 size-full">
      <Tile label="calls / min" value="214">
        <Spark values={CALLS} />
      </Tile>
      <Tile className="hidden @min-[560px]:flex" label="failed" value="0.4%">
        <Bars failed={[6, 15]} values={ERRORS} />
      </Tile>
      <Tile
        className="hidden @min-[560px]:flex"
        label="p50 latency"
        value="1.42s"
      >
        <Bars values={LATENCY} />
      </Tile>
    </div>
  );
}

/** Where the deploy line crosses the BAML row: a short stretch of BAML's own line. */
function Crossing() {
  return (
    <span
      aria-hidden
      className="self-center h-[1.5px] w-full"
      style={{ background: 'var(--ds-accent)' }}
    />
  );
}

/** The line from the production panel back to Build and Test, with a beam running along it. */
function Return({ label, short }: { label: string; short: string }) {
  const row = useRef<HTMLDivElement>(null);
  const build = useRef<HTMLSpanElement>(null);
  const prod = useRef<HTMLSpanElement>(null);
  const [geo, setGeo] = useState({ from: 0, h: 0, to: 0 });

  useLayoutEffect(() => {
    const el = row.current;
    if (!el) return;
    const measure = () => {
      const box = el.getBoundingClientRect();
      const center = (r: HTMLElement | null) => {
        const b = r?.getBoundingClientRect();
        return b ? b.left + b.width / 2 - box.left : 0;
      };
      setGeo({
        from: center(prod.current),
        h: box.height,
        to: center(build.current),
      });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const { from, h, to } = geo;
  const y = h / 2;
  const r = Math.min(12, y);
  const d = `M${from} 0V${y - r}Q${from} ${y} ${from - r} ${y}H${to + r}Q${to} ${y} ${to} ${y - r}V2`;

  return (
    <div className={`${GRID} relative h-12 @min-[640px]:h-16`} ref={row}>
      <span />
      <span className="col-span-2" ref={build} />
      <span />
      <span ref={prod} />
      {from > 0 && (
        <svg
          aria-hidden="true"
          className="absolute inset-0 size-full overflow-visible"
          fill="none"
        >
          <path d={d} stroke="var(--ds-line)" strokeWidth="1.5" />
          <path
            d={`M${to - 4} 6L${to} 2L${to + 4} 6`}
            stroke="var(--ds-accent)"
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth="1.5"
          />
          <Beam d={d} dur={3.6} />
        </svg>
      )}
      <p
        className="absolute top-1/2 -translate-x-1/2 -translate-y-1/2 px-3 bg-(--ds-ground) text-[12px] @min-[640px]:text-[14px] @min-[960px]:text-[15px] text-(--ds-accent-ink) whitespace-nowrap"
        style={{ left: (from + to) / 2 || '50%' }}
      >
        <span className="@min-[640px]:hidden">{short}</span>
        <span className="hidden @min-[640px]:inline">{label}</span>
      </p>
    </div>
  );
}

/** The shared layout. `panel` is what BAML's row shows in production. */
function Split({
  grow = 0,
  label,
  panel,
  style = 'midnight',
  tall,
}: {
  /** Pixels the panel reaches above its row. */
  grow?: number;
  label: string;
  panel: ReactNode;
  style?: string;
  tall?: boolean;
}) {
  return (
    <div className={`ds-theme ds-theme-${style} rounded-[28px] p-2 sm:p-3`}>
      <Frame label={label}>
        <div className="relative p-4 @min-[640px]:p-6 @min-[960px]:px-8 @min-[960px]:pt-6">
          <div className="relative">
            {/* The deploy line, drawn once over every row in the same grid, so it lines up with the gap. */}
            <div
              aria-hidden
              className={`${GRID} absolute inset-x-0 top-7 @min-[640px]:top-8 bottom-0 pointer-events-none`}
            >
              <span className="col-start-4 justify-self-center w-0 h-full border-l border-dashed border-(--ds-ink)/35" />
            </div>

            <div className={`${GRID} h-6 @min-[640px]:h-7`}>
              <span
                className={`col-start-4 justify-self-center font-mono text-[11px] @min-[640px]:text-[13px] ${MUTED} whitespace-nowrap`}
              >
                deploy
              </span>
            </div>

            <div className={`${GRID} mb-2 @min-[640px]:mb-3`}>
              <span />
              {['Build', 'Test'].map((c) => (
                <p
                  className="text-center font-display font-medium text-[14px] @min-[640px]:text-[20px] @min-[960px]:text-[24px] leading-none tracking-[-0.03em]"
                  key={c}
                >
                  {c}
                </p>
              ))}
              <span />
              <p className="text-center font-display font-medium text-[14px] @min-[640px]:text-[20px] @min-[960px]:text-[24px] leading-none tracking-[-0.03em]">
                Production
              </p>
            </div>

            <div className="flex flex-col gap-1.5 @min-[640px]:gap-2">
              <div className="relative flex flex-col gap-1.5 @min-[640px]:gap-2">
                {/* Past deploy every other language is in the same place, so one cell spans all their rows. */}
                <div className={`${GRID} absolute inset-0 pointer-events-none`}>
                  <p className="col-start-5 flex items-center justify-center min-w-0 px-1 border border-dashed border-(--ds-line) rounded-(--ds-card-radius) font-mono text-[11px] @min-[640px]:text-[13px] @min-[960px]:text-[14px] text-(--ds-ink)/45 text-center">
                    Separate products
                  </p>
                </div>
                {ROWS.map((row) => (
                  <div className={`${GRID} ${ROW_HEIGHT}`} key={row.name}>
                    <div className="self-center min-w-0">
                      <p className="text-[14px] @min-[640px]:text-[17px] text-(--ds-ink)/75">
                        {row.short ? (
                          <>
                            <span className="@min-[640px]:hidden">
                              {row.short}
                            </span>
                            <span className="hidden @min-[640px]:inline">
                              {row.name}
                            </span>
                          </>
                        ) : (
                          row.name
                        )}
                      </p>
                      <p className="hidden @min-[640px]:block font-mono text-[12px] text-(--ds-ink)/35">
                        {row.year}
                      </p>
                    </div>
                    <Card className="col-span-2 min-w-0">
                      <Commands commands={row.commands} />
                    </Card>
                  </div>
                ))}
              </div>

              <div
                className={`${GRID} ${tall ? TALL_PANEL_HEIGHT : PANEL_HEIGHT} mt-1 @min-[640px]:mt-1.5`}
              >
                <div className="flex items-center gap-2 min-w-0">
                  <span className="hidden @min-[640px]:block">
                    <Lamb size={22} />
                  </span>
                  <p className="font-display font-medium text-[14px] @min-[640px]:text-[19px] tracking-[-0.02em]">
                    BAML
                  </p>
                </div>
                <Card
                  className={`col-span-2 self-center min-w-0 ${ROW_HEIGHT}`}
                  lit
                  style={TINT}
                >
                  <Commands commands={['baml check', 'baml test']} lit />
                </Card>
                <Crossing />
                {/* The panel sits on the row's floor, so when it grows it grows upward, over the rows above. */}
                <div className="relative min-w-0">
                  <Card
                    className="absolute inset-x-0 bottom-0 z-10 p-1.5 @min-[640px]:p-2.5 transition-[height] duration-700 ease-[cubic-bezier(0.22,1,0.36,1)]"
                    lit
                    style={{ ...TINT, height: `calc(100% + ${grow}px)` }}
                  >
                    {panel}
                  </Card>
                </div>
              </div>
            </div>

            <Return label="Improve" short="Improve" />
          </div>
        </div>
      </Frame>
    </div>
  );
}

const ROWS_LABEL =
  'How far each language’s toolchain reaches, with deploy as a line between build and test on the left and production on the right. TypeScript, Go, and Python cover build and test, and past deploy each takes separate products. BAML covers build and test, carries on past deploy into production';

/** The command a panel shows the output of, on its first line. */
function Prompt({ children, extra }: { children: string; extra?: string }) {
  return (
    <p className="shrink-0 truncate px-1 @min-[640px]:px-2 font-mono text-[10px] @min-[640px]:text-[12.5px] text-(--ds-ink)/80">
      <span className="text-(--ds-accent-ink)">$</span> {children}
      {extra && (
        <span className={`hidden @min-[560px]:inline ${MUTED}`}> {extra}</span>
      )}
    </p>
  );
}

/** A panel: its command on top, and what came back under it. */
function Output({
  children,
  command,
  extra,
}: {
  children: ReactNode;
  command: string;
  extra?: string;
}) {
  return (
    <div className="flex flex-col gap-1.5 @min-[640px]:gap-2 size-full">
      <Prompt extra={extra}>{command}</Prompt>
      <div className="flex-1 min-h-0">{children}</div>
    </div>
  );
}

export function HeroSplitMetrics({ style }: { style?: string }) {
  return (
    <Split
      label={`${ROWS_LABEL}, where a baml query shows calls per minute, the share that failed, and latency, and a line runs from production back to build.`}
      panel={
        <Output command="baml query" extra="classify --last 1h">
          <Metrics />
        </Output>
      }
      style={style}
    />
  );
}

const SCENES = [
  { command: 'baml query', name: 'Dashboard' },
  { command: 'baml alerts', extra: '--list', name: 'Alerts' },
  { command: 'baml query', name: 'Latency' },
];

const ALERTS = [
  {
    firing: true,
    name: 'classify errors above 2%',
    note: '31 calls',
    short: 'errors > 2%',
  },
  { name: 'triage p95 above 5s', short: 'p95 > 5s' },
  { name: 'cost per call above $0.01', short: 'cost > $0.01' },
];

/** The output of `baml alerts --list`: one alert firing, and the rest quiet. */
function Alerts() {
  return (
    <div className="flex flex-col justify-center gap-1 @min-[640px]:gap-1.5 size-full px-1 @min-[640px]:px-2">
      {ALERTS.map((a) => (
        <p
          className={`flex items-center gap-1.5 @min-[640px]:gap-2 min-w-0 text-[10.5px] @min-[640px]:text-[13px] @min-[960px]:text-[14px] ${a.firing ? '' : MUTED}`}
          key={a.name}
        >
          {a.firing ? (
            <Cross className="size-3 @min-[640px]:size-4" />
          ) : (
            <Check className="size-3 @min-[640px]:size-4" />
          )}
          <span className="min-w-0 truncate">
            <span className="@min-[560px]:hidden">{a.short}</span>
            <span className="hidden @min-[560px]:inline">{a.name}</span>
          </span>
          <span
            className="hidden @min-[560px]:inline ml-auto shrink-0 font-mono text-[11px] @min-[640px]:text-[12px]"
            style={a.firing ? { color: RED } : undefined}
          >
            {a.firing ? `firing, ${a.note}` : 'ok'}
          </span>
        </p>
      ))}
    </div>
  );
}

function Latency() {
  return (
    <div className="flex flex-col size-full px-1 @min-[640px]:px-2">
      <div className="flex items-baseline gap-3 text-[11px] @min-[640px]:text-[13px]">
        <span className="font-display font-medium text-[15px] @min-[640px]:text-[20px] @min-[960px]:text-[24px] tracking-[-0.02em]">
          1.42s
        </span>
        <span className={MUTED}>p50</span>
        <span className={`hidden @min-[560px]:inline ${MUTED}`}>
          <span className="text-(--ds-ink)/75">3.10s</span> p95
        </span>
      </div>
      <div className="relative flex-1 min-h-0 mt-1.5">
        <div className="absolute inset-0 opacity-45">
          <Spark
            color="var(--ds-ink)"
            values={series(24, 9).map((v) => v * 0.5 + 0.45)}
          />
        </div>
        <div className="absolute inset-0">
          <Spark values={series(24, 9).map((v) => v * 0.4 + 0.08)} />
        </div>
      </div>
    </div>
  );
}

/** Production as a dashboard, then an alert, then a latency graph, one at a time. */
export function HeroSplitRotating({ style }: { style?: string }) {
  const scene = useCycle(SCENES.length, 3400, 0);
  return (
    <Split
      label={`${ROWS_LABEL}, where it shows a baml query dashboard of calls, failures, and latency, then baml alerts --list with classify errors above 2% firing on 31 calls, then a baml query latency graph. A line runs from production back to build.`}
      panel={
        <div className="relative size-full">
          {[<Metrics key="d" />, <Alerts key="a" />, <Latency key="l" />].map(
            (s, i) => (
              <div
                className={`absolute inset-0 pb-4 transition-opacity duration-700 ${scene === i ? 'opacity-100' : 'opacity-0'}`}
                key={SCENES[i].name}
              >
                <Output command={SCENES[i].command} extra={SCENES[i].extra}>
                  {s}
                </Output>
              </div>
            ),
          )}
          <div className="absolute inset-x-0 bottom-0 flex items-center justify-center gap-1.5">
            {SCENES.map((s, i) => (
              <span
                className={`flex items-center gap-1 text-[10px] @min-[640px]:text-[11px] transition-colors duration-500 ${scene === i ? 'text-(--ds-accent-ink)' : 'text-(--ds-ink)/30'}`}
                key={s.name}
              >
                <span className="size-1 rounded-full bg-current" />
                <span className="hidden @min-[640px]:inline">{s.name}</span>
              </span>
            ))}
          </div>
        </div>
      }
      style={style}
    />
  );
}

/** Rows of a result: a name, a bar sized against the largest value, and the value. */
function BarRows({
  rows,
}: {
  rows: { failed?: boolean; name: string; text: string; value: number }[];
}) {
  const max = Math.max(...rows.map((r) => r.value));
  return (
    <div className="flex flex-col justify-center gap-1.5 @min-[640px]:gap-2 size-full">
      {rows.map((r, i) => (
        <div
          className="ds-in grid grid-cols-[3.4em_minmax(0,1fr)] @min-[560px]:grid-cols-[6em_minmax(0,1fr)_4em] items-center gap-2"
          key={r.name}
          style={{ animationDelay: `${i * 90}ms` }}
        >
          <span
            className={r.failed ? '' : 'text-(--ds-ink)/70'}
            style={r.failed ? { color: RED } : undefined}
          >
            {r.name}
          </span>
          <span
            className="ds-grow h-2 @min-[640px]:h-2.5 rounded-[1px]"
            style={{
              background: r.failed
                ? RED
                : 'rgb(from var(--ds-accent) r g b / 0.6)',
              width: `${(r.value / max) * 100}%`,
            }}
          />
          <span className={`hidden @min-[560px]:inline text-right ${MUTED}`}>
            {r.text}
          </span>
        </div>
      ))}
    </div>
  );
}

// What the terminal runs, one command at a time. Each query is real SQL over the spans BAML records, cut off
// at the edge of the terminal when it's too long to fit.
const SESSION: { command: string; result: ReactNode }[] = [
  {
    command: `baml query --watch "SELECT date_trunc('minute', start_time) AS minute, count(*) FROM spans WHERE span_name = 'classify' GROUP BY minute"`,
    result: <MinuteLive />,
  },
  {
    command: `baml query "SELECT output_value AS label, count(*) FROM spans WHERE span_name = 'classify' GROUP BY label"`,
    result: (
      <BarRows
        rows={[
          { name: 'billing', text: '412', value: 412 },
          { name: 'bug', text: '318', value: 318 },
          { failed: true, name: 'other', text: '31', value: 31 },
        ]}
      />
    ),
  },
  {
    command: `baml query "SELECT version, quantile(duration_ms, 0.5) AS p50 FROM spans WHERE span_name = 'classify' GROUP BY version"`,
    result: (
      <BarRows
        rows={[
          { name: 'v41', text: '1.61s', value: 1.61 },
          { name: 'v42', text: '1.42s', value: 1.42 },
          { name: 'v43', text: '1.18s', value: 1.18 },
        ]}
      />
    ),
  },
  { command: 'baml alerts --list', result: <Alerts /> },
];

const TYPE_MS = 1300;
const RUN_MS = 350;
const SHOW_MS = 3400;
const STEP_MS = TYPE_MS + RUN_MS + SHOW_MS;

/** Milliseconds since mount, every frame. Under reduced motion it holds at `hold`. */
function useElapsed(hold: number) {
  const [t, setT] = useState(0);
  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setT(hold);
      return;
    }
    const start = performance.now();
    let frame = requestAnimationFrame(function tick(now) {
      // A frame's timestamp can be from just before the effect ran.
      setT(Math.max(0, now - start));
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  }, [hold]);
  return t;
}

/** A terminal that types each command in SESSION, runs it, shows what came back, and moves to the next. */
function Session() {
  const t = useElapsed(TYPE_MS + RUN_MS);
  const step = Math.floor(t / STEP_MS) % SESSION.length;
  const local = t % STEP_MS;
  const { command, result } = SESSION[step];
  // Every command takes the same time to type, however long it is.
  const typed = command.slice(
    0,
    Math.round(Math.min(1, local / TYPE_MS) * command.length),
  );
  const typing = local < TYPE_MS;
  const shown = local >= TYPE_MS + RUN_MS;

  return (
    <div className="flex flex-col size-full px-1 @min-[640px]:px-2 font-mono text-[10px] @min-[640px]:text-[12.5px]">
      <div className="flex items-center gap-2 pb-1.5 @min-[640px]:pb-2 mb-1.5 @min-[640px]:mb-2 border-b border-(--ds-line)">
        <span className={MUTED}>~/triage</span>
        <span className="ml-auto flex gap-1">
          {SESSION.map((s, i) => (
            <span
              className={`size-1 rounded-full transition-colors duration-300 ${i === step ? 'bg-(--ds-accent)' : 'bg-(--ds-ink)/20'}`}
              key={s.command}
            />
          ))}
        </span>
      </div>
      <p className="truncate text-(--ds-ink)/85">
        <span className="text-(--ds-accent-ink)">$</span> {typed}
        <span
          className={`${typing ? '' : 'ds-blink'} ml-px inline-block w-[0.55em] h-[1.1em] align-[-0.2em] bg-(--ds-ink)/60`}
        />
      </p>
      <div className="flex-1 min-h-0 mt-2 @min-[640px]:mt-3" key={step}>
        {shown && result}
      </div>
    </div>
  );
}

/** Production as a terminal running `baml query` and `baml alerts`, one command after another. */
export function HeroSplitQuery({ style }: { style?: string }) {
  return (
    <Split
      label={`${ROWS_LABEL}, where a terminal runs baml query in turn for calls to classify per minute, by label (billing 412, bug 318, other 31, which failed), and p50 latency by version (v41 1.61s, v42 1.42s, v43 1.18s), then baml alerts --list, with classify errors above 2% firing. A line runs from production back to build.`}
      panel={<Session />}
      style={style}
      tall
    />
  );
}

const LIVE_BARS = 40;
const BAR_MS = 900;
const CALL_MS = 260;

/** Noise in [0, 1) for bar i, so the chart can run forever and still draw the same bars every time. */
function noise(i: number) {
  const x = Math.sin(i * 12.9898 + 4.1) * 43758.5453;
  return x - Math.floor(x);
}
const liveValue = (i: number) =>
  0.45 + 0.22 * Math.sin(i / 3.3) + 0.3 * (noise(i) - 0.5);
const liveFailed = (i: number) => noise(i * 7 + 3) < 0.08;

/** Calls per minute that keep arriving: the newest bar fills on the right while the chart slides left. */
function LiveBars({ t }: { t: number }) {
  const first = Math.floor(t / BAR_MS);
  const progress = (t % BAR_MS) / BAR_MS;
  // One more bar than fits, slid left by however far the newest one has filled.
  const bars = Array.from({ length: LIVE_BARS + 1 }, (_, k) => first + k);
  return (
    <div className="relative size-full overflow-hidden">
      <div
        className="absolute inset-y-0 left-0 flex items-end gap-px"
        style={{
          transform: `translateX(${(-progress / (LIVE_BARS + 1)) * 100}%)`,
          width: `${((LIVE_BARS + 1) / LIVE_BARS) * 100}%`,
        }}
      >
        {bars.map((i, k) => {
          const last = k === LIVE_BARS;
          return (
            <span
              className="flex-1 min-w-0 rounded-t-[1px]"
              key={i}
              style={{
                background: liveFailed(i)
                  ? RED
                  : 'rgb(from var(--ds-accent) r g b / 0.55)',
                height: `${Math.max(4, liveValue(i) * 100 * (last ? progress : 1))}%`,
              }}
            />
          );
        })}
      </div>
    </div>
  );
}

/** Calls per minute in the terminal session, moving like the live chart while the watch runs. */
function MinuteLive() {
  const t = useElapsed(BAR_MS * 0.6);
  return (
    <div className="ds-in flex flex-col size-full">
      <div className="flex-1 min-h-0">
        <LiveBars t={t} />
      </div>
      <p className={`flex justify-between mt-1 ${MUTED}`}>
        <span>40 min ago</span>
        <span className="hidden @min-[560px]:inline">calls per minute</span>
        <span className="inline-flex items-center gap-1.5 text-(--ds-accent-ink)">
          <span className="ds-blink size-1.5 rounded-full bg-current" />
          now
        </span>
      </p>
    </div>
  );
}

/** Production as one live chart of calls per minute, with failed calls in red. */
export function HeroSplitLive({ style }: { style?: string }) {
  const t = useElapsed(BAR_MS * 0.6);
  const calls = 12408 + Math.floor(t / CALL_MS);
  return (
    <Split
      label={`${ROWS_LABEL}, where a live baml query chart shows calls per minute with a few failed calls in red. A line runs from production back to build.`}
      panel={
        <div className="flex flex-col size-full">
          <div className="flex items-center gap-2 min-w-0">
            <div className="min-w-0">
              <Prompt>
                {`baml query --watch "SELECT date_trunc('minute', start_time) AS minute, count(*) FROM spans WHERE span_name = 'classify' GROUP BY minute"`}
              </Prompt>
            </div>
            <span className="ml-auto shrink-0 inline-flex items-center gap-1.5 pr-1 @min-[640px]:pr-2 text-[10.5px] @min-[640px]:text-[12px] text-(--ds-accent-ink)">
              <span className="hidden @min-[560px]:inline font-mono tabular-nums text-(--ds-ink)/60">
                {calls.toLocaleString('en-US')} calls
              </span>
              <span className="ds-blink size-1.5 rounded-full bg-current" />
              <span className="hidden @min-[560px]:inline">live</span>
            </span>
          </div>
          <div className="flex-1 min-h-0 mt-2 px-1 @min-[640px]:px-2">
            <LiveBars t={t} />
          </div>
          <p
            className={`hidden @min-[560px]:flex items-center gap-1.5 mt-1.5 px-2 text-[11px] ${MUTED}`}
          >
            calls per minute
            <span
              className="ml-3 size-1.5 rounded-[1px]"
              style={{ background: RED }}
            />
            failed call
          </p>
        </div>
      }
      style={style}
    />
  );
}

const LOOP_MS = 15000;
const CHAT_TYPE_MS = 650;
const CHAT_RESULT_MS = 900;
const MAX_GROW = 52;

// An agent working in a terminal chat: what you asked, the commands it runs, the widgets they return, and
// what it says. Each turn starts at `at` milliseconds into the loop.
const CHAT: {
  at: number;
  kind: 'agent' | 'tool' | 'user';
  result?: ReactNode;
  text: string;
}[] = [
  {
    at: 0,
    kind: 'user',
    text: 'Tickets are landing in the wrong queue. Can you look?',
  },
  {
    at: 1000,
    kind: 'tool',
    result: (
      <div className="h-11 @min-[640px]:h-14">
        <BarRows
          rows={[
            { name: 'billing', text: '412', value: 412 },
            { name: 'bug', text: '318', value: 318 },
            { failed: true, name: 'other', text: '31', value: 31 },
          ]}
        />
      </div>
    ),
    text: `baml query "SELECT output_value['label'] AS label, count(*) FROM spans WHERE span_name = 'classify' GROUP BY label"`,
  },
  {
    at: 3400,
    kind: 'agent',
    text: '31 sarcastic tickets were labelled other since v42.',
  },
  {
    at: 4800,
    kind: 'tool',
    result: (
      <div className="h-12 @min-[640px]:h-16">
        <Alerts />
      </div>
    ),
    text: 'baml alerts --list',
  },
  {
    at: 7200,
    kind: 'tool',
    result: (
      <div className="h-8 @min-[640px]:h-11">
        <Bars failed={[5, 6, 19, 31]} values={series(40, 3)} />
      </div>
    ),
    text: `baml query "SELECT date_trunc('minute', start_time) AS minute, count(*) FROM spans WHERE span_name = 'classify' GROUP BY minute"`,
  },
  {
    at: 9600,
    kind: 'agent',
    text: 'Added a prompt rule for sarcasm and 31 tests from these calls.',
  },
  {
    at: 11000,
    kind: 'tool',
    result: (
      <p className="flex items-center gap-1.5">
        <Check className="size-3 @min-[640px]:size-4" />
        40 passed, 0 failed
      </p>
    ),
    text: 'baml test classify',
  },
];

/** One turn of the chat as of `local` milliseconds into it. */
function ChatTurn({
  local,
  turn,
}: {
  local: number;
  turn: (typeof CHAT)[number];
}) {
  if (turn.kind === 'user') {
    return (
      <p className="ds-in text-(--ds-ink)/90">
        <span className="text-(--ds-accent-ink)">&gt;</span> {turn.text}
      </p>
    );
  }
  if (turn.kind === 'agent') {
    return (
      <p className="ds-in flex gap-1.5 text-(--ds-ink)/75">
        <span className="mt-[0.45em] size-1.5 shrink-0 rounded-full bg-(--ds-accent)" />
        <span className="min-w-0">{turn.text}</span>
      </p>
    );
  }
  const typed = turn.text.slice(
    0,
    Math.round(Math.min(1, local / CHAT_TYPE_MS) * turn.text.length),
  );
  return (
    <div className="ds-in flex flex-col gap-1.5">
      <p className={`truncate ${MUTED}`}>
        <span className="text-(--ds-accent-ink)">$</span> {typed}
      </p>
      {local >= CHAT_RESULT_MS && (
        <div className="ds-in p-1.5 @min-[640px]:p-2 border border-(--ds-line) rounded-[max(0px,calc(var(--ds-card-radius)_-_4px))]">
          {turn.result}
        </div>
      )}
    </div>
  );
}

/** Production as a chat with an agent that runs baml query and baml alerts and gets widgets back. */
export function HeroSplitChat({ style }: { style?: string }) {
  const t = useElapsed(LOOP_MS - 1);
  const local = t % LOOP_MS;
  const shown = CHAT.filter((c) => local >= c.at);
  // The panel grows a little with each turn, up over the rows above it, then resets with the chat.
  const grow = Math.round((MAX_GROW * (shown.length - 1)) / (CHAT.length - 1));

  return (
    <Split
      grow={grow}
      label={`${ROWS_LABEL}, where an agent in a terminal chat is asked why tickets land in the wrong queue. It runs baml query and gets bars by label (billing 412, bug 318, other 31, which failed), says 31 sarcastic tickets were labelled other since v42, runs baml alerts --list, runs baml query for calls per minute, adds a prompt rule and 31 tests, and runs baml test classify: 40 passed. A line runs from production back to build.`}
      panel={
        <div className="flex flex-col size-full px-1 @min-[640px]:px-2 font-mono text-[10px] @min-[640px]:text-[12px] leading-snug">
          <div className="flex items-center gap-2 shrink-0 pb-1.5 @min-[640px]:pb-2 border-b border-(--ds-line)">
            <span className="text-(--ds-ink)/80">agent</span>
            <span className={`ml-auto ${MUTED}`}>~/triage</span>
          </div>
          {/* The newest turn sits at the bottom, and older ones scroll up out of view. */}
          <div className="flex-1 min-h-0 overflow-hidden flex flex-col justify-end gap-2 @min-[640px]:gap-2.5 pt-2">
            {shown.map((c) => (
              <ChatTurn key={c.at} local={local - c.at} turn={c} />
            ))}
          </div>
        </div>
      }
      style={style}
      tall
    />
  );
}
