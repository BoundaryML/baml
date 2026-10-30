'use client';

import Image from 'next/image';
import { type ReactNode, useState } from 'react';
import { track } from '../track';
import { Bar, MONO, Win } from './get-started';

// What teams build on their traces. Choosing one sets the end of the heading and swaps the window on
// the painting, so each tool shows the query behind it. The queries use the tables baml query reads;
// the result rows are example data. The last workflow is proactive, so it shows an agent prompt.
type Workflow = {
  body: string;
  key: string;
  prompt?: string;
  result?: { columns: string[]; rows: string[][] };
  sql?: string;
  title: string;
  word: string;
};

const WORKFLOWS: Workflow[] = [
  {
    body: 'See input and output tokens for each customer, from the metadata you attach to each call.',
    key: 'user_tracking',
    result: {
      columns: ['customer', 'input_tokens', 'output_tokens'],
      rows: [
        ['acme-corp', '1204331', '88210'],
        ['northwind', '842019', '61447'],
      ],
    },
    sql: `SELECT context_metadata['customer'] AS customer,
  sum(total_usage[0]['input_tokens']) AS input_tokens,
  sum(total_usage[0]['output_tokens']) AS output_tokens
FROM spans WHERE parent_span_id IS NULL
GROUP BY customer`,
    title: 'User tracking',
    word: 'user tracking pipeline',
  },
  {
    body: 'Post to Slack when a function starts failing.',
    key: 'alerts',
    result: {
      columns: ['span_name', 'errors'],
      rows: [
        ['user.ExtractInvoice', '14'],
        ['user.ClassifyTicket', '3'],
      ],
    },
    sql: `SELECT span_name, count(*) AS errors
FROM spans WHERE status = 'user_error'
GROUP BY span_name`,
    title: 'Alerts',
    word: 'alerts system',
  },
  {
    body: 'Turn failed production calls into test cases for your next change.',
    key: 'evals',
    result: {
      columns: ['input_args', 'error_value'],
      rows: [
        ['{"file":"acme_0412.pdf"}', 'parse error'],
        ['{"file":"halden_scan.pdf"}', 'parse error'],
      ],
    },
    sql: `SELECT input_args, error_value FROM spans
WHERE span_name = 'user.ExtractInvoice'
  AND status = 'user_error'`,
    title: 'AI evals',
    word: 'eval pipeline',
  },
  {
    body: 'Track latency and volume for every function.',
    key: 'dashboards',
    result: {
      columns: ['span_name', 'calls', 'avg_ms'],
      rows: [
        ['user.ExtractInvoice', '8412', '401'],
        ['user.ClassifyTicket', '2960', '188'],
      ],
    },
    sql: `SELECT span_name, count(*) AS calls,
  avg(duration) / 1e6 AS avg_ms
FROM spans GROUP BY span_name`,
    title: 'Dashboards',
    word: 'dashboards',
  },
  {
    body: 'Track new model releases and test them automatically. When a model ships, an agent runs your evals against it and reports what changed.',
    key: 'model_releases',
    prompt: `When a new model ships:
1. Switch ExtractInvoice to it on a new branch.
2. Run baml test against last week's failed calls.
3. Compare cost and latency with baml query.
4. Open a pull request with the results.`,
    title: 'Model release testing',
    word: 'model release tracker',
  },
];

// Keywords and string literals pick up the same colours as the other query windows.
function highlight(sql: string): ReactNode[] {
  return sql
    .split(/(\b(?:SELECT|FROM|WHERE|GROUP BY|AND|AS|IS NULL)\b|'[^']*')/)
    .map((part, i) => {
      const key = `${i}-${part}`;
      if (part.startsWith("'"))
        return (
          <span className="text-pasture-200" key={key}>
            {part}
          </span>
        );
      return /^(SELECT|FROM|WHERE|GROUP BY|AND|AS|IS NULL)$/.test(part) ? (
        <span className="text-clover" key={key}>
          {part}
        </span>
      ) : (
        part
      );
    });
}

function Window({ workflow }: { workflow: Workflow }) {
  const { prompt, result, sql } = workflow;
  if (prompt)
    return (
      <Win inline>
        <Bar sub="runs when a model ships" title="agent prompt" />
        <pre
          className={`px-5 py-5 text-[#D8DDD3] whitespace-pre-wrap ${MONO(true)} md:text-[13px]`}
        >
          {prompt}
        </pre>
      </Win>
    );
  return (
    <Win inline>
      <Bar sub="spans" title="baml query" />
      <pre
        className={`px-5 pt-5 pb-4 text-[#D8DDD3] overflow-x-auto ${MONO(true)} md:text-[13px]`}
      >
        {highlight(sql ?? '')}
      </pre>
      {result && (
        <div className="px-5 pb-5">
          <div
            className={`rounded-md border border-white/10 overflow-x-auto ${MONO(true)} md:text-[13px]`}
          >
            <table className="w-full text-left text-[#D8DDD3]">
              <thead className="text-[#6E756A]">
                <tr>
                  {result.columns.map((c) => (
                    <th className="px-3 py-2 font-normal" key={c} scope="col">
                      {c}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {result.rows.map((row) => (
                  <tr className="border-t border-white/10" key={row[0]}>
                    {row.map((cell, i) => (
                      <td
                        className="px-3 py-2 whitespace-nowrap"
                        key={result.columns[i]}
                      >
                        {cell}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className="mt-2 text-[11px] leading-4 text-[#6E756A]">
            Example data
          </p>
        </div>
      )}
    </Win>
  );
}

export default function Workflows() {
  const [active, setActive] = useState(0);
  const workflow = WORKFLOWS[active];

  return (
    <section
      className="w-full max-w-[1440px] mx-auto px-5 md:px-12 pt-16 md:pt-30 pb-20 md:pb-34 border-t border-hair"
      id="product"
    >
      <div className="mb-10 md:mb-14">
        <h2 className="max-w-215 h2-display">
          Build your own <span className="text-accent">{workflow.word}.</span>
        </h2>
      </div>

      <div className="flex flex-col lg:flex-row gap-10 lg:gap-16 items-stretch">
        <ul className="w-full lg:w-105 shrink-0 border-t border-ink">
          {WORKFLOWS.map((w, i) => {
            const on = i === active;
            return (
              <li className="border-b border-hair" key={w.key}>
                <button
                  aria-pressed={on}
                  className="w-full flex flex-col py-5 text-left cursor-pointer group"
                  onClick={() => {
                    track('workflow_selected', { workflow: w.key });
                    setActive(i);
                  }}
                  type="button"
                >
                  <span
                    className={`font-display font-medium text-[19px] leading-[22px] tracking-[-0.03em] transition-colors ${on ? 'text-ink' : 'text-ink-3 group-hover:text-ink-2'}`}
                  >
                    {w.title}
                  </span>
                  <span
                    className={`grid transition-[grid-template-rows,opacity] duration-300 ${on ? 'grid-rows-[1fr] opacity-100' : 'grid-rows-[0fr] opacity-0'}`}
                  >
                    <span className="overflow-hidden">
                      <span className="block pt-3 text-[15px] leading-6 text-ink-2">
                        {w.body}
                      </span>
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
          <li className="py-5 text-sm leading-[22px] text-ink-2">
            Or anything else your team needs.
          </li>
        </ul>

        <div
          className="relative flex-1 min-w-0 flex items-center rounded-[14px] overflow-hidden bg-pasture p-3 sm:p-8 lg:p-12 min-h-[360px] lg:min-h-[540px]"
          data-theme="light"
        >
          <Image
            alt=""
            className="object-cover object-[50%_65%]"
            fill
            quality={90}
            sizes="(min-width: 1024px) 860px, 100vw"
            src="/boundary/painting-hills.jpg"
          />
          <Window workflow={workflow} />
        </div>
      </div>
    </section>
  );
}
