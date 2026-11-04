'use client';

import { type ReactNode, useEffect, useState } from 'react';
import { Code } from './code';
import { classifySource, FIXES, RED } from './story';

// One function as an object you can inspect, not only its source. Labels around it carry what the
// toolchain knows about it: its types, tests, traces, latency, errors, deployments, and experiments. Each
// is wired to the function by a hairline and brightens when its data changes. Pointing at a label, or
// focusing or tapping it, shows that data in the center panel. The source compiles with baml check; every
// number is example data.
type Key =
  | 'types'
  | 'tests'
  | 'traces'
  | 'latency'
  | 'errors'
  | 'deployments'
  | 'experiments';

const LEFT: Key[] = ['types', 'tests', 'traces'];
const RIGHT: Key[] = ['latency', 'errors', 'deployments', 'experiments'];
const NAMES: Record<Key, string> = {
  deployments: 'Deployments',
  errors: 'Errors',
  experiments: 'Experiments',
  latency: 'Latency',
  tests: 'Tests',
  traces: 'Traces',
  types: 'Types',
};

// Which labels have data that moves, and how often one of them updates.
const MOVING: Key[] = ['traces', 'latency', 'errors', 'experiments', 'traces'];
const TICK_MS = 1500;
const FRESH_MS = 1300;

type Live = {
  calls: number;
  errors: number;
  fresh: Partial<Record<Key, number>>;
  now: number;
  p50: number;
  trials: number;
};

const START: Live = {
  calls: 12408,
  errors: 0.8,
  fresh: {},
  now: 0,
  p50: 1.42,
  trials: 1840,
};

function update(s: Live, now: number): Live {
  const key = MOVING[Math.floor(Math.random() * MOVING.length)];
  return {
    calls:
      key === 'traces' ? s.calls + 2 + Math.floor(Math.random() * 6) : s.calls,
    errors:
      key === 'errors'
        ? Math.max(0.6, Math.min(1.1, s.errors + (Math.random() - 0.5) * 0.2))
        : s.errors,
    fresh: { ...s.fresh, [key]: now },
    now,
    p50: key === 'latency' ? 1.3 + Math.random() * 0.25 : s.p50,
    trials:
      key === 'experiments'
        ? s.trials + 3 + Math.floor(Math.random() * 5)
        : s.trials,
  };
}

function summary(key: Key, s: Live) {
  return {
    deployments: 'v42, 3h ago',
    errors: `${s.errors.toFixed(1)}% of calls`,
    experiments: `Prompt B, ${s.trials.toLocaleString('en-US')} trials`,
    latency: `p50 ${s.p50.toFixed(2)}s`,
    tests: '9 of 9 passing',
    traces: `${s.calls.toLocaleString('en-US')} calls`,
    types: '(string) -> Verdict',
  }[key];
}

const TYPES = [
  'type Label = "billing" | "bug" | "other";',
  '',
  'class Verdict {',
  '  label: Label,',
  '  confidence: float,',
  '}',
  '',
  'function classify(text: string) -> Verdict',
];

const TESTS: [string, string][] = [
  ['sarcastic_ticket', '1.2s'],
  ['spanish_ticket', '1.4s'],
  ['angry_refund', '1.1s'],
  ['plain_bug', '0.9s'],
  ['plain_billing', '1.0s'],
  ['empty_ticket', '0.7s'],
];

const TRACES: [string, string, string, boolean?][] = [
  ['12s', 'Third time asking. Refund me.', 'billing'],
  ['19s', 'App crashes when I upload a PDF', 'bug'],
  ['31s', 'Oh great, another outage.', 'bug'],
  ['44s', 'La factura está duplicada', 'billing'],
  ['58s', 'asdf', '', true],
];

const HISTOGRAM = [2, 5, 11, 19, 26, 24, 17, 12, 8, 6, 4, 3, 2, 2, 1, 1];

const ERRORS: [string, number][] = [
  ['Verdict.confidence missing from output', 61],
  ['Request timed out after 30s', 24],
  ['Rate limited, retried and passed', 15],
];

const DEPLOYS: [string, string, string][] = [
  ['v42', '3h ago', `Added "${FIXES[2].rule}"`],
  ['v41', '2d ago', `Added "${FIXES[1].rule}"`],
  ['v40', '5d ago', `Added "${FIXES[0].rule}"`],
];

const Row = ({ children }: { children: ReactNode }) => (
  <li className="flex items-baseline gap-4 py-2 border-t border-white/8 first:border-t-0">
    {children}
  </li>
);

function View({ s, view }: { s: Live; view: Key | null }) {
  const mono = 'font-mono text-[12px] md:text-[13px]';
  switch (view) {
    case 'types':
      return <Code lines={TYPES} />;
    case 'tests':
      return (
        <div className="px-4 md:px-5">
          <p className={`${mono} text-white/50 mb-2`}>$ baml test classify</p>
          <ul className={mono}>
            {TESTS.map(([name, ms]) => (
              <Row key={name}>
                <span className="text-clover">passed</span>
                <span className="flex-1 min-w-0 truncate text-white/85">
                  {name}
                </span>
                <span className="text-white/45">{ms}</span>
              </Row>
            ))}
          </ul>
          <p className={`${mono} mt-2 text-white/50`}>
            9 tests, 9 passed. 3 not shown.
          </p>
        </div>
      );
    case 'traces':
      return (
        <ul className={`px-4 md:px-5 ${mono}`}>
          {TRACES.map(([at, input, label, failed]) => (
            <Row key={at}>
              <span className="w-8 shrink-0 text-white/40">{at}</span>
              <span className="flex-1 min-w-0 truncate text-(--color-pasture-200)">
                {input}
              </span>
              <span
                className="shrink-0"
                style={{
                  color: failed ? '#FF9A9E' : 'rgb(255 255 255 / 0.85)',
                }}
              >
                {failed ? 'error' : label}
              </span>
            </Row>
          ))}
        </ul>
      );
    case 'latency': {
      const max = Math.max(...HISTOGRAM);
      return (
        <div className="px-4 md:px-5">
          <div className="flex items-end gap-[3px] h-36 md:h-44 border-b border-white/15">
            {HISTOGRAM.map((n, i) => (
              <span
                className={`flex-1 ${i === 5 ? 'bg-clover' : 'bg-white/25'}`}
                // biome-ignore lint/suspicious/noArrayIndexKey: one bar per latency bucket
                key={i}
                style={{ height: `${(n / max) * 100}%` }}
              />
            ))}
          </div>
          <div className={`flex justify-between mt-1.5 ${mono} text-white/40`}>
            <span>0s</span>
            <span>2s</span>
            <span>4s</span>
          </div>
          <dl className={`grid grid-cols-3 gap-4 mt-5 ${mono}`}>
            {[
              ['p50', `${s.p50.toFixed(2)}s`],
              ['p95', '2.81s'],
              ['p99', '3.94s'],
            ].map(([k, v]) => (
              <div key={k}>
                <dt className="text-white/45">{k}</dt>
                <dd className="text-white/90 text-[15px] md:text-[17px]">
                  {v}
                </dd>
              </div>
            ))}
          </dl>
        </div>
      );
    }
    case 'errors':
      return (
        <div className="px-4 md:px-5">
          <p className={`${mono} text-white/50 mb-2`}>
            {s.errors.toFixed(1)}% of calls in the last 24 hours, grouped by
            cause
          </p>
          <ul className={mono}>
            {ERRORS.map(([text, share]) => (
              <Row key={text}>
                <span className="flex-1 min-w-0 text-white/85">{text}</span>
                <span className="w-16 h-[3px] shrink-0 self-center bg-white/10">
                  <span
                    className="block h-full"
                    style={{ background: RED, width: `${share}%` }}
                  />
                </span>
                <span className="w-9 shrink-0 text-right text-white/45">
                  {share}%
                </span>
              </Row>
            ))}
          </ul>
        </div>
      );
    case 'deployments':
      return (
        <ul className={`px-4 md:px-5 ${mono}`}>
          {DEPLOYS.map(([version, at, change], i) => (
            <Row key={version}>
              <span
                className={`w-8 shrink-0 ${i ? 'text-white/60' : 'text-clover'}`}
              >
                {version}
              </span>
              <span className="w-14 shrink-0 text-white/40">{at}</span>
              <span className="flex-1 min-w-0 text-white/80">{change}</span>
            </Row>
          ))}
        </ul>
      );
    case 'experiments':
      return (
        <div className={`px-4 md:px-5 ${mono}`}>
          <p className="text-white/50 mb-4">
            Two prompts on 10% of traffic, scored against labels your team
            corrected
          </p>
          {[
            ['Prompt A, current', 91.2, false],
            ['Prompt B, adds an example per label', 94.1, true],
          ].map(([name, score, leads]) => (
            <div className="mb-4" key={String(name)}>
              <div className="flex justify-between gap-4 mb-1.5">
                <span className="text-white/80">{name}</span>
                <span className={leads ? 'text-clover' : 'text-white/60'}>
                  {Number(score).toFixed(1)}%
                </span>
              </div>
              <span className="block h-1.5 bg-white/10">
                <span
                  className={`block h-full ${leads ? 'bg-clover' : 'bg-white/40'}`}
                  style={{ width: `${score}%` }}
                />
              </span>
            </div>
          ))}
          <p className="text-white/45">
            {s.trials.toLocaleString('en-US')} trials so far
          </p>
        </div>
      );
    default: {
      const { lines, prose } = classifySource(FIXES.map((f) => f.rule));
      return <Code lines={lines} prose={prose} />;
    }
  }
}

function Label({
  active,
  fresh,
  k,
  onPick,
  s,
}: {
  active: boolean;
  fresh: boolean;
  k: Key;
  onPick: (k: Key | null) => void;
  s: Live;
}) {
  return (
    <button
      aria-pressed={active}
      className={`focus-ring shrink-0 min-w-0 px-3 py-2 rounded-md border text-left cursor-pointer transition-colors duration-500 ${active ? 'border-clover/70 bg-clover/10' : fresh ? 'border-white/40 bg-white/[0.05]' : 'border-white/12 bg-[#0b0b0d]'}`}
      onClick={() => onPick(active ? null : k)}
      onFocus={() => onPick(k)}
      onMouseEnter={() => onPick(k)}
      type="button"
    >
      <span
        className={`block text-[13px] md:text-[14px] transition-colors duration-500 ${active || fresh ? 'text-white' : 'text-white/70'}`}
      >
        {NAMES[k]}
      </span>
      <span
        className={`block font-mono text-[11.5px] md:text-[12px] whitespace-nowrap transition-colors duration-500 ${active ? 'text-clover' : fresh ? 'text-white/80' : 'text-white/40'}`}
      >
        {summary(k, s)}
      </span>
    </button>
  );
}

export default function HeroInspector() {
  const [live, setLive] = useState(START);
  const [view, setView] = useState<Key | null>(null);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const id = setInterval(
      () => setLive((s) => update(s, Date.now())),
      TICK_MS,
    );
    return () => clearInterval(id);
  }, []);

  const fresh = (k: Key) => live.now - (live.fresh[k] ?? 0) < FRESH_MS;

  // A label on either side, wired to the panel by a hairline that ends on its edge.
  const side = (keys: Key[], right?: boolean) => (
    <ul className="hidden md:flex flex-col justify-center gap-5">
      {keys.map((k) => {
        const on = view === k || fresh(k);
        return (
          <li
            className={`flex items-center ${right ? 'flex-row-reverse' : ''}`}
            key={k}
          >
            <Label
              active={view === k}
              fresh={fresh(k)}
              k={k}
              onPick={setView}
              s={live}
            />
            <span
              className={`flex-1 h-px transition-colors duration-500 ${view === k ? 'bg-clover' : on ? 'bg-white/45' : 'bg-white/15'}`}
            />
            <span
              className={`size-[5px] shrink-0 rounded-full transition-colors duration-500 ${view === k ? 'bg-clover' : on ? 'bg-white/70' : 'bg-white/30'}`}
            />
          </li>
        );
      })}
    </ul>
  );

  return (
    <figure
      aria-label="A BAML function as an object you can inspect. Labels around it show its types, tests, traces, latency, errors, deployments, and experiments, each wired to the function. Choosing a label shows that data in the center."
      className="relative pb-16"
      onMouseLeave={() => setView(null)}
    >
      {/* On a phone the labels sit above the panel, without connectors. */}
      <ul className="md:hidden flex flex-wrap gap-2 mb-4">
        {[...LEFT, ...RIGHT].map((k) => (
          <li key={k}>
            <Label
              active={view === k}
              fresh={fresh(k)}
              k={k}
              onPick={setView}
              s={live}
            />
          </li>
        ))}
      </ul>

      <div className="md:grid md:grid-cols-[210px_minmax(0,1fr)_230px] lg:grid-cols-[240px_minmax(0,1fr)_260px]">
        {side(LEFT)}
        <div className="relative z-10 min-w-0 overflow-hidden rounded-lg border border-white/15 bg-[#0b0b0d]">
          <div className="flex items-center justify-between gap-4 h-10 px-4 border-b border-white/10 text-[13px]">
            <span className="flex items-baseline gap-3 min-w-0">
              <span className="font-mono text-white">classify</span>
              <span className="hidden sm:inline text-white/40 truncate">
                triage.baml
              </span>
            </span>
            <span
              className={`font-mono text-[12px] ${view ? 'text-clover' : 'text-white/45'}`}
            >
              {view ? NAMES[view] : 'Source'}
            </span>
          </div>
          <div
            className="v3-enter min-h-[300px] md:min-h-[360px] py-4 overflow-x-auto"
            key={view ?? 'source'}
          >
            <View s={live} view={view} />
          </div>
        </div>
        {side(RIGHT, true)}
      </div>
    </figure>
  );
}
