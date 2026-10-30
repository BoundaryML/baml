'use client';

import { type ReactNode, useState } from 'react';
import { track } from '../track';

// What teams build on their traces. Choosing one sets the end of the heading, swaps the query, and
// draws what that query turns into. The result visuals use example data.
type Workflow = {
  key: string;
  label: string;
  query: string;
  queryTitle: string;
  summary: string;
  visual: ReactNode;
  word: string;
};

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

function ReleaseSteps() {
  const steps = [
    'New model detected',
    'Tests run on last week’s failed calls',
    'Cost and latency compared',
    'Pull request opened with the results',
  ];
  return (
    <ol className="flex flex-col gap-5 w-full text-[15px]">
      {steps.map((s, i) => (
        <li
          className="v2-pop flex gap-3 items-center"
          key={s}
          style={delay(i * 0.25)}
        >
          <span className="grid place-items-center shrink-0 w-6 h-6 rounded-full bg-pasture-300 text-black text-xs font-medium">
            ✓
          </span>
          {s}
        </li>
      ))}
    </ol>
  );
}

const WORKFLOWS: Workflow[] = [
  {
    key: 'user_tracking',
    label: 'User tracking',
    query: `SELECT context_metadata['customer'] AS customer,
  sum(total_usage[0]['input_tokens']) AS input_tokens,
  sum(total_usage[0]['output_tokens']) AS output_tokens
FROM spans WHERE parent_span_id IS NULL
GROUP BY customer`,
    queryTitle: 'baml query',
    summary:
      'See input and output tokens for each customer, from the metadata you attach to each call.',
    visual: <CustomerBars />,
    word: 'user tracking pipeline',
  },
  {
    key: 'alerts',
    label: 'Alerts',
    query: `SELECT span_name, count(*) AS errors
FROM spans WHERE status = 'user_error'
GROUP BY span_name`,
    queryTitle: 'baml query',
    summary: 'Post to Slack when a function starts failing.',
    visual: <FailureAlert />,
    word: 'alerts system',
  },
  {
    key: 'evals',
    label: 'AI evals',
    query: `SELECT input_args, error_value FROM spans
WHERE span_name = 'user.ExtractInvoice'
  AND status = 'user_error'`,
    queryTitle: 'baml query',
    summary:
      'Turn failed production calls into test cases for your next change.',
    visual: <EvalGrid />,
    word: 'eval pipeline',
  },
  {
    key: 'dashboards',
    label: 'Dashboards',
    query: `SELECT span_name, count(*) AS calls,
  avg(duration) / 1e6 AS avg_ms
FROM spans GROUP BY span_name`,
    queryTitle: 'baml query',
    summary: 'Track latency and volume for every function.',
    visual: <MiniDashboard />,
    word: 'dashboards',
  },
  {
    key: 'model_releases',
    label: 'Model release testing',
    query: `When a new model ships:
1. Switch ExtractInvoice to it on a new branch.
2. Run baml test against last week's failed calls.
3. Compare cost and latency with baml query.
4. Open a pull request with the results.`,
    queryTitle: 'agent prompt',
    summary:
      'Track new model releases and test them automatically. When a model ships, an agent runs your evals against it and reports what changed.',
    visual: <ReleaseSteps />,
    word: 'model release tracker',
  },
];

const KEYWORDS = /(\b(?:SELECT|FROM|WHERE|GROUP BY|AND|AS|IS NULL)\b|'[^']*')/;

// Keywords and string literals take the brand's two colors. A prompt is plain text.
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
  const w = WORKFLOWS[active];

  return (
    <section
      className="max-w-[1440px] mx-auto px-5 md:px-12 pt-20 md:pt-36 pb-16 md:pb-28"
      id="product"
    >
      <h2 className="v2-h2 max-w-300 min-h-[2.04em]">
        Build your own {w.word}.
      </h2>

      <div className="flex flex-wrap gap-2 mt-8 md:mt-12">
        {WORKFLOWS.map((item, i) => (
          <button
            aria-pressed={i === active}
            className={`py-2.5 px-4 rounded-full border text-sm cursor-pointer transition-colors ${
              i === active
                ? 'bg-white text-black border-white'
                : 'border-white/15 text-white/70 hover:text-white hover:border-white/40'
            }`}
            key={item.key}
            onClick={() => {
              track('workflow_selected', { workflow: item.key });
              setActive(i);
            }}
            type="button"
          >
            {item.label}
          </button>
        ))}
        <span className="py-2.5 px-4 text-sm text-white/50">
          Or anything else your team needs.
        </span>
      </div>

      <div className="grid lg:grid-cols-2 mt-6 rounded-2xl border border-white/12 overflow-hidden">
        <div className="min-w-0 flex flex-col border-b lg:border-b-0 lg:border-r border-white/12">
          <div className="px-5 md:px-7 py-3.5 border-b border-white/12 font-mono text-xs text-white/60">
            {w.queryTitle}
          </div>
          <pre className="flex-1 px-5 md:px-7 py-6 overflow-x-auto font-mono text-[12.5px] md:text-[14px] leading-[1.7] text-white/85">
            {highlight(w.query)}
          </pre>
          <p className="px-5 md:px-7 py-5 border-t border-white/12 text-[15px] leading-6 text-white/70">
            {w.summary}
          </p>
        </div>
        {/* Keyed, so the entrance plays again for each workflow. */}
        <div
          className="flex items-center min-h-[300px] lg:min-h-[420px] p-6 md:p-12 bg-white/[0.03]"
          key={w.key}
        >
          {w.visual}
        </div>
      </div>
    </section>
  );
}
