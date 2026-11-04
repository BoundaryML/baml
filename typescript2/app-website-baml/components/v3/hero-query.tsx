'use client';

import { useEffect, useState } from 'react';
import { RED } from './story';

// The same question asked of production twice. With a separate tool for each kind of telemetry, the
// agent needs five query languages and five keys, and still has no answer. In BAML it's one baml query,
// in the syntax baml query documents. The tools are kinds of tools rather than products, and every
// result is example data.
const QUESTION = 'Why are refunds misrouted since yesterday?';

const TOOLS: {
  denied?: boolean;
  query: string;
  result: string;
  tool: string;
}[] = [
  {
    query: 'service:triage status:error "refund"',
    result: '412 lines. No model output.',
    tool: 'Log search',
  },
  {
    denied: true,
    query: '{ span.name = "llm.call" && duration > 1s }',
    result: '403: this key can’t read traces',
    tool: 'Tracing',
  },
  {
    query: 'rate(llm_errors_total{fn="classify"}[1d])',
    result: '0.024. No reason attached.',
    tool: 'Metrics',
  },
  {
    query: 'is:unresolved JSONDecodeError',
    result: '412 events. Stack traces only.',
    tool: 'Error tracking',
  },
  {
    denied: true,
    query: "SELECT properties.label FROM events WHERE event = 'triaged'",
    result: '401: missing API key',
    tool: 'Product analytics',
  },
];

const BAML_QUERY = [
  "SELECT input_args['text'] AS text,",
  "  output_value['label'] AS label",
  'FROM spans',
  "WHERE span_name = 'classify'",
  "  AND input_args['text'] ILIKE '%refund%'",
];
const ROWS = [
  ['Third time asking. Refund me.', 'bug'],
  ['Your app ate my money. Refund??', 'bug'],
  ['I want my money back. Now.', 'bug'],
];

const TICK_MS = 50;
const LAP_MS = 14000;
const START = 900;
const STEP = 1700;
const TYPE = 700;
const RESULT = 1000;
const BAML_TYPE = 1500;
const TABLE = START + BAML_TYPE + 300;
const ANSWER = TABLE + ROWS.length * 150 + 500;
const SUMMARY = START + TOOLS.length * STEP;

const clamp = (n: number) => Math.min(1, Math.max(0, n));

function typed(text: string, t: number, start: number, ms: number) {
  return text.slice(0, Math.round(text.length * clamp((t - start) / ms)));
}

function Window({
  children,
  lit,
  title,
}: {
  children: React.ReactNode;
  lit?: boolean;
  title: React.ReactNode;
}) {
  return (
    <div
      className={`v3-window relative flex flex-col min-w-0 overflow-hidden ${lit ? 'v3-card-on' : ''}`}
    >
      <div className="flex items-center h-11 px-4 border-b border-white/8 text-[13px] md:text-[14px] text-white/70">
        {title}
      </div>
      <div className="flex-1 p-4 md:p-5">{children}</div>
    </div>
  );
}

export default function HeroQuery() {
  const [t, setT] = useState(0);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setT(LAP_MS - 1);
      return;
    }
    const begin = performance.now();
    const id = setInterval(
      () => setT((performance.now() - begin) % LAP_MS),
      TICK_MS,
    );
    return () => clearInterval(id);
  }, []);

  const query = BAML_QUERY.join('\n');
  const answered = t >= ANSWER;

  return (
    <figure
      aria-label="An agent asks why refunds are misrouted. With five separate tools it needs five query languages and five keys, and two of them refuse it. In BAML it runs one baml query and finds that refund requests are being labelled bug."
      className="relative pb-16"
    >
      <div
        aria-hidden
        className="v3-glow left-[30%] right-[-10%] -top-10 h-[640px]"
      />

      <p className="relative flex justify-center mb-8 md:mb-10">
        <span className="max-w-full px-4 md:px-5 py-2.5 rounded-2xl rounded-br-md border border-white/12 bg-white/[0.06] text-[15px] md:text-lg text-white/90">
          {typed(QUESTION, t, 0, 700) || ' '}
        </span>
      </p>

      <div className="relative grid md:grid-cols-2 gap-4 md:gap-6">
        <Window
          title={
            <>
              Your agent, with five tools
              <span className="ml-auto font-mono text-white/35">
                5 keys, 5 query languages
              </span>
            </>
          }
        >
          <ol className="flex flex-col gap-3.5 md:gap-4">
            {TOOLS.map((tool, i) => {
              const start = START + i * STEP;
              const done = t >= start + RESULT;
              return (
                <li
                  className={`min-w-0 transition-opacity duration-300 ${t >= start ? '' : 'opacity-25'}`}
                  key={tool.tool}
                >
                  <p className="text-[12.5px] md:text-[13px] text-white/45">
                    {tool.tool}
                  </p>
                  <p className="mt-0.5 font-mono text-[12px] md:text-[13px] leading-5 text-white/85 truncate">
                    <span className="text-white/35">&gt; </span>
                    {typed(tool.query, t, start, TYPE)}
                  </p>
                  <p
                    className={`flex items-center gap-2 font-mono text-[12px] md:text-[13px] leading-5 transition-opacity duration-200 ${done ? '' : 'opacity-0'}`}
                    style={{ color: tool.denied ? '#FF9A9E' : '#F5C26B' }}
                  >
                    <span
                      className="size-1.5 shrink-0 rounded-full"
                      style={{ background: tool.denied ? RED : '#F5A524' }}
                    />
                    {tool.result}
                  </p>
                </li>
              );
            })}
          </ol>
          <p
            className={`mt-5 pt-4 border-t border-white/8 text-[14px] md:text-[15px] text-white/60 transition-opacity duration-300 ${t >= SUMMARY ? '' : 'opacity-0'}`}
          >
            Five queries in. Still no answer.
          </p>
        </Window>

        <Window
          lit={answered}
          title={
            <>
              Your agent, in BAML
              <span className="ml-auto font-mono text-white/35">1 query</span>
            </>
          }
        >
          <pre className="font-mono text-[12px] md:text-[13px] leading-5 text-white/85 whitespace-pre-wrap break-words min-h-[120px]">
            <span className="text-clover">$ </span>
            <span className="text-white">baml query</span>
            {'\n'}
            {typed(query, t, START, BAML_TYPE)}
          </pre>

          <table
            className={`w-full mt-4 table-fixed text-left font-mono text-[12px] md:text-[13px] leading-6 transition-opacity duration-300 ${t >= TABLE ? '' : 'opacity-0'}`}
          >
            <thead className="text-white/45">
              <tr>
                <th className="font-normal" scope="col">
                  text
                </th>
                <th className="w-14 font-normal" scope="col">
                  label
                </th>
              </tr>
            </thead>
            <tbody>
              {ROWS.map(([text, label], i) => (
                <tr
                  className={`border-t border-white/8 transition-opacity duration-200 ${t >= TABLE + (i + 1) * 150 ? '' : 'opacity-0'}`}
                  key={text}
                >
                  <td className="pr-3 truncate text-(--color-pasture-200)">
                    {text}
                  </td>
                  <td style={{ color: '#FF9A9E' }}>{label}</td>
                </tr>
              ))}
            </tbody>
          </table>

          <p
            className={`mt-5 pt-4 border-t border-white/8 text-[14px] md:text-[15px] text-clover transition-opacity duration-300 ${answered ? '' : 'opacity-0'}`}
          >
            Since v42, angry refund requests are labelled bug. 412 calls.
          </p>
        </Window>
      </div>
    </figure>
  );
}
