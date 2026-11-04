'use client';

import { type ReactNode, useEffect, useRef, useState } from 'react';
import { track } from '../track';

// One call behind every tool a team builds. Each use case shows the same API with a different query, and
// what that query turns into. The result pictures use example data.
type UseCase = {
  builds: string;
  key: string;
  label: string;
  query: string;
  summary: string;
  visual: ReactNode;
};

const CYCLE_MS = 5000;

const delay = (s: number) => ({ '--delay': `${s}s` }) as React.CSSProperties;

function CustomerBars() {
  const rows: [string, number, string][] = [
    ['acme-corp', 100, '1.2M tokens'],
    ['northwind', 70, '842K tokens'],
    ['halden', 26, '310K tokens'],
    ['sundial', 14, '171K tokens'],
  ];
  return (
    <div className="flex flex-col gap-4 w-full">
      {rows.map(([name, width, value], i) => (
        <div key={name}>
          <div className="flex justify-between mb-1.5 text-sm">
            <span>{name}</span>
            <span className="text-white/60">{value}</span>
          </div>
          <div className="h-2.5 rounded-full bg-white/8">
            <div
              className="v2-grow h-full rounded-full bg-pasture-300"
              style={{ ...delay(i * 0.08), width: `${width}%` }}
            />
          </div>
        </div>
      ))}
    </div>
  );
}

function FailureAlert() {
  return (
    <div className="w-full">
      <svg
        aria-hidden="true"
        className="w-full h-24"
        fill="none"
        viewBox="0 0 400 96"
      >
        <path
          className="v2-draw"
          d="M0 80 L60 76 L120 78 L180 74 L240 76 L290 70 L320 18 L350 30 L400 12"
          pathLength={1000}
          stroke="#E5484D"
          strokeWidth="2"
        />
      </svg>
      <div
        className="v2-pop mt-4 rounded-[10px] border border-white/12 p-4 text-sm"
        style={delay(0.7)}
      >
        <div className="text-white/60">#ai-alerts</div>
        <div className="mt-1.5">
          user.ExtractInvoice started failing. 14 errors in the last hour.
        </div>
      </div>
    </div>
  );
}

function EvalGrid() {
  const failed = new Set([5, 14, 22]);
  return (
    <div className="w-full">
      <div className="grid grid-cols-8 gap-1.5">
        {Array.from({ length: 32 }, (_, i) => (
          <div
            className={`v2-pop aspect-square rounded-[4px] ${failed.has(i) ? 'bg-[#E5484D]' : 'bg-pasture-300/30'}`}
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed grid of cells
            key={i}
            style={delay(i * 0.02)}
          />
        ))}
      </div>
      <div className="mt-4 text-sm text-white/70">
        3 failed calls, ready to run as baml test cases
      </div>
    </div>
  );
}

function MiniDashboard() {
  const bars = [38, 52, 44, 66, 58, 72, 64, 88, 76, 94, 82, 100];
  return (
    <div className="w-full">
      <div className="flex justify-between text-sm">
        <span>user.ExtractInvoice</span>
        <span className="text-white/60">401 ms average</span>
      </div>
      <div className="flex items-end gap-1.5 h-40 mt-4">
        {bars.map((h, i) => (
          <div
            className="v2-rise flex-1 rounded-t-[3px] bg-pasture-300"
            // biome-ignore lint/suspicious/noArrayIndexKey: a fixed series of bars
            key={i}
            style={{ ...delay(i * 0.04), height: `${h}%` }}
          />
        ))}
      </div>
    </div>
  );
}

function Funnel() {
  const steps: [string, number, string][] = [
    ['Signed up', 100, '12,480'],
    ['Ran first agent', 65, '8,112'],
    ['Accepted a result', 23, '2,921'],
  ];
  return (
    <div className="flex flex-col gap-3 w-full">
      {steps.map(([step, width, users], i) => (
        <div key={step}>
          <div className="flex justify-between mb-1.5 text-sm">
            <span>{step}</span>
            <span className="text-white/60">{users} users</span>
          </div>
          <div className="h-8 rounded-[4px] bg-white/6">
            <div
              className="v2-grow h-full rounded-[4px] bg-pasture-300"
              style={{ ...delay(i * 0.1), width: `${width}%` }}
            />
          </div>
        </div>
      ))}
    </div>
  );
}

function ModelCompare() {
  const rows: [string, string, string, boolean][] = [
    ['gpt-4o', '8,412', '401 ms', false],
    ['claude-sonnet-4-5', '8,412', '355 ms', true],
    ['gemini-2.5-flash', '8,412', '212 ms', false],
  ];
  return (
    <table className="w-full text-left text-sm">
      <thead className="text-white/50">
        <tr>
          <th className="pb-3 font-normal" scope="col">
            model
          </th>
          <th className="pb-3 font-normal text-right" scope="col">
            calls
          </th>
          <th className="pb-3 font-normal text-right" scope="col">
            average
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map(([model, calls, ms, best], i) => (
          <tr
            className={`v2-pop border-t border-white/12 ${best ? 'text-pasture-200' : ''}`}
            key={model}
            style={delay(i * 0.12)}
          >
            <td className="py-3 font-mono text-[13px]">{model}</td>
            <td className="py-3 text-right">{calls}</td>
            <td className="py-3 text-right">{ms}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const USE_CASES: UseCase[] = [
  {
    builds: 'Slack alert',
    key: 'alerts',
    label: 'Alerts',
    query: `SELECT span_name, count(*) AS errors
FROM spans WHERE status = 'user_error'
GROUP BY span_name`,
    summary: 'Post to Slack when a function starts failing.',
    visual: <FailureAlert />,
  },
  {
    builds: 'Custom dashboard',
    key: 'dashboards',
    label: 'Dashboards',
    query: `SELECT span_name, count(*) AS calls,
  avg(duration) / 1e6 AS avg_ms
FROM spans GROUP BY span_name`,
    summary: 'Track latency and volume for every function.',
    visual: <MiniDashboard />,
  },
  {
    builds: 'Test cases',
    key: 'evals',
    label: 'AI evals',
    query: `SELECT input_args, error_value FROM spans
WHERE span_name = 'user.ExtractInvoice'
  AND status = 'user_error'`,
    summary:
      'Turn failed production calls into test cases for your next change.',
    visual: <EvalGrid />,
  },
  {
    builds: 'User funnel',
    key: 'funnels',
    label: 'User funnels',
    query: `SELECT span_name,
  count(DISTINCT context_distinct_id) AS users
FROM spans WHERE span_name IN
  ('user.SignUp', 'user.RunAgent', 'user.AcceptResult')
GROUP BY span_name`,
    summary: 'See where new users drop off on the way to their first result.',
    visual: <Funnel />,
  },
  {
    builds: 'Usage report',
    key: 'user_tracking',
    label: 'Cost by customer',
    query: `SELECT context_metadata['customer'] AS customer,
  sum(total_usage[0]['input_tokens']) AS input_tokens,
  sum(total_usage[0]['output_tokens']) AS output_tokens
FROM spans WHERE parent_span_id IS NULL
GROUP BY customer`,
    summary:
      'See input and output tokens for each customer, from the metadata you attach to each call.',
    visual: <CustomerBars />,
  },
  {
    builds: 'Model comparison',
    key: 'model_releases',
    label: 'Model testing',
    query: `SELECT context_metadata['model'] AS model,
  count(*) AS calls,
  avg(duration) / 1e6 AS avg_ms
FROM spans WHERE span_name = 'user.ExtractInvoice'
GROUP BY model`,
    summary:
      'When a model ships, have an agent run your evals against it and compare the results.',
    visual: <ModelCompare />,
  },
];

// The same SQL, from each place it can run.
const CALLERS = [
  {
    after: '"',
    before: '$ baml query "',
    key: 'terminal',
    label: 'Terminal',
  },
  {
    after: '`);',
    before: 'const { rows } = await boundary.query(`',
    key: 'dashboard',
    label: 'Dashboard',
  },
];

const KEYWORDS =
  /(\b(?:SELECT|FROM|WHERE|GROUP BY|AND|AS|IS NULL|IN|DISTINCT)\b|'[^']*')/;

// Keywords and string literals take the brand's two colors.
function highlight(query: string): ReactNode[] {
  return query.split(KEYWORDS).map((part, i) => {
    const key = `${i}-${part}`;
    if (part.startsWith("'") && part.endsWith("'") && part.length > 1)
      return (
        <span className="text-pasture-200" key={key}>
          {part}
        </span>
      );
    return KEYWORDS.test(part) ? (
      <span className="text-clover" key={key}>
        {part}
      </span>
    ) : (
      part
    );
  });
}

export default function Workflows() {
  const [active, setActive] = useState(0);
  const [caller, setCaller] = useState(0);
  const [held, setHeld] = useState(false);
  const [hovering, setHovering] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const useCase = USE_CASES[active];
  const call = CALLERS[caller];

  // Moves through the use cases while on screen, until someone picks one. Hovering pauses it for reading.
  useEffect(() => {
    if (held || hovering) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const el = ref.current;
    if (!el) return;
    let id: ReturnType<typeof setInterval> | undefined;
    const io = new IntersectionObserver(
      ([e]) => {
        clearInterval(id);
        if (e.isIntersecting)
          id = setInterval(
            () => setActive((a) => (a + 1) % USE_CASES.length),
            CYCLE_MS,
          );
      },
      { threshold: 0.25 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      clearInterval(id);
    };
  }, [held, hovering]);

  return (
    <section
      className="max-w-[1440px] mx-auto px-5 md:px-12 pt-20 md:pt-36 pb-16 md:pb-28"
      id="product"
    >
      <h2 className="v2-h2">One API, for every use case.</h2>
      <p className="max-w-160 mt-6 text-xl leading-[1.5] text-white/70">
        Every tool starts with the same call: SQL over everything your program
        recorded. Run it in your terminal, from a dashboard, or have your coding
        agent write it.
      </p>

      <div
        className="grid lg:grid-cols-[250px_minmax(0,1fr)_minmax(0,1fr)] mt-10 md:mt-14 rounded-2xl border border-white/12 overflow-hidden"
        onBlur={() => setHovering(false)}
        onFocus={() => setHovering(true)}
        onMouseEnter={() => setHovering(true)}
        onMouseLeave={() => setHovering(false)}
        ref={ref}
      >
        <ul className="grid grid-cols-2 lg:grid-cols-1 lg:grid-rows-[repeat(6,auto)_1fr] gap-px bg-white/12 border-b lg:border-b-0 lg:border-r border-white/12">
          {USE_CASES.map((item, i) => (
            <li className="bg-black" key={item.key}>
              <button
                aria-pressed={i === active}
                className={`focus-ring relative w-full px-5 py-3.5 lg:py-4 text-left text-[15px] cursor-pointer transition-colors ${
                  i === active
                    ? 'bg-white/5 text-white'
                    : 'text-white/60 hover:text-white'
                }`}
                onClick={() => {
                  track('workflow_selected', { workflow: item.key });
                  setActive(i);
                  setHeld(true);
                }}
                type="button"
              >
                {item.label}
                {i === active && (
                  <span
                    className="absolute inset-x-0 bottom-0 h-0.5 bg-pasture-300 origin-left"
                    key={active}
                  >
                    <span
                      className={`block h-full bg-white ${held || hovering ? '' : 'v2-countdown'}`}
                      style={{ '--ms': `${CYCLE_MS}ms` } as React.CSSProperties}
                    />
                  </span>
                )}
              </button>
            </li>
          ))}
          <li className="col-span-2 lg:col-span-1 bg-black px-5 py-4 text-sm text-white/45">
            Or anything else your team needs.
          </li>
        </ul>

        <div className="min-w-0 flex flex-col border-b lg:border-b-0 lg:border-r border-white/12">
          <div className="flex items-center gap-1 px-3 h-12 border-b border-white/12">
            {CALLERS.map((c, i) => (
              <button
                aria-pressed={i === caller}
                className={`focus-ring px-3 py-1 rounded-md text-sm cursor-pointer transition-colors ${i === caller ? 'bg-white/10 text-white' : 'text-white/55 hover:text-white'}`}
                key={c.key}
                onClick={() => {
                  track('api_caller_selected', { caller: c.key });
                  setCaller(i);
                  setHeld(true);
                }}
                type="button"
              >
                {c.label}
              </button>
            ))}
          </div>
          <pre className="flex-1 px-5 md:px-7 py-6 overflow-x-auto font-mono text-[12.5px] md:text-[13.5px] leading-[1.75] text-white/90">
            <span className="text-white/50">{call.before}</span>
            {'\n'}
            {highlight(useCase.query)}
            {'\n'}
            <span className="text-white/50">{call.after}</span>
          </pre>
          <p className="px-5 md:px-7 py-5 border-t border-white/12 text-[15px] leading-6 text-white/70">
            {useCase.summary}
          </p>
        </div>

        {/* Keyed, so the entrance plays again for each use case. */}
        <div
          className="min-w-0 flex flex-col bg-white/[0.03]"
          key={useCase.key}
        >
          <div className="flex items-center h-12 px-5 md:px-7 border-b border-white/12 text-sm text-white/60">
            {useCase.builds}
          </div>
          <div className="flex flex-1 items-center min-h-[300px] lg:min-h-[400px] p-6 md:p-10">
            {useCase.visual}
          </div>
        </div>
      </div>
    </section>
  );
}
