'use client';

import { type ReactNode, useState } from 'react';
import { track } from '../track';

// The same model call as most LLM tracing records it and as BAML records it. OTEL attributes are typed,
// so the scalars stay scalars here; the point is the result, which reaches OTEL as a JSON string. The
// attribute names are OpenInference's. The BAML queries use the syntax baml query documents.
type Row = 'total' | 'paid' | 'vendor' | 'path';

const QUERIES: {
  baml: string;
  key: string;
  label: string;
  otel: string | null;
  reads: Row[];
}[] = [
  {
    baml: `SELECT span_id FROM spans
WHERE output_value['total'] > 1000`,
    key: 'filter_field',
    label: 'Invoices over 1,000',
    otel: `SELECT span_id FROM spans
WHERE CAST(json_extract(
    attributes['output.value'], '$.total'
  ) AS FLOAT) > 1000`,
    reads: ['total'],
  },
  {
    baml: `SELECT output_value['vendor'] AS vendor,
  count(*) AS calls
FROM spans GROUP BY vendor`,
    key: 'group_field',
    label: 'Calls by vendor',
    otel: `SELECT json_extract(
    attributes['output.value'], '$.vendor'
  ) AS vendor, count(*) AS calls
FROM spans GROUP BY vendor`,
    reads: ['vendor'],
  },
  {
    baml: `SELECT function_name, invocation_count
FROM profiler`,
    key: 'call_counts',
    label: 'Exact counts per call path',
    otel: null,
    reads: ['path'],
  },
];

function highlight(sql: string): ReactNode[] {
  return sql
    .split(/(\b(?:SELECT|FROM|WHERE|GROUP BY|AS|CAST|FLOAT)\b|'[^']*')/)
    .map((part, i) => {
      const key = `${i}-${part}`;
      if (part.startsWith("'"))
        return (
          <span className="text-pasture-200" key={key}>
            {part}
          </span>
        );
      return /^(SELECT|FROM|WHERE|GROUP BY|AS|CAST|FLOAT)$/.test(part) ? (
        <span className="text-clover" key={key}>
          {part}
        </span>
      ) : (
        part
      );
    });
}

function Field({
  hit,
  indent,
  name,
  type,
  value,
}: {
  hit: boolean;
  indent?: boolean;
  name: string;
  type?: string;
  value: string;
}) {
  return (
    <div
      className={`flex items-baseline gap-3 px-3 py-1.5 rounded-[3px] transition-colors duration-200 ${hit ? 'bg-clover/18' : ''} ${indent ? 'ml-4' : ''}`}
    >
      <span className="min-w-0 truncate text-white/60">{name}</span>
      {type && (
        <span className="px-1.5 rounded-[3px] border border-clover/40 text-[11px] leading-[18px] text-clover">
          {type}
        </span>
      )}
      <span className="ml-auto min-w-0 truncate text-white">{value}</span>
    </div>
  );
}

export default function Telemetry() {
  const [active, setActive] = useState(0);
  const query = QUERIES[active];
  const reads = (row: Row) => query.reads.includes(row);
  const blobHit = query.otel !== null;

  return (
    <div className="mt-8 md:mt-10 rounded-2xl border border-white/12 overflow-hidden font-mono text-[12px] md:text-[12.5px] leading-[20px]">
      <div className="grid lg:grid-cols-2">
        <div className="min-w-0 p-5 md:p-7 max-lg:border-b lg:border-r border-white/12">
          <div className="flex items-baseline justify-between gap-3 font-sans">
            <h4 className="text-[17px] text-white/70">OTEL span</h4>
            <span className="text-[13px] text-white/45">
              A typical LLM integration
            </span>
          </div>
          <pre className="mt-5 whitespace-pre-wrap break-words text-white/55">
            {`{
  "name": "ExtractInvoice",
  "attributes": {
    "llm.model_name": "gpt-4o",
    "llm.token_count.total": 412,
    "input.value": "{\\"file\\":\\"acme_0412.pdf\\"}",
`}
            <span
              className={`transition-colors duration-200 ${blobHit ? 'bg-[#E5484D]/25 text-white' : ''}`}
            >
              {`    "output.value": "{\\"total\\":1204.5,\\"paid\\":false,\\"vendor\\":\\"Acme\\"}"`}
            </span>
            {`
  }
}`}
          </pre>
          <p className="mt-4 font-sans text-[14px] leading-[21px] text-white/55">
            The result is one string.
          </p>
        </div>

        <div className="min-w-0 p-5 md:p-7">
          <div className="flex items-baseline justify-between gap-3 font-sans">
            <h4 className="v2-baml-mark text-[19px]">BAML record</h4>
            <span className="text-[13px] text-white/45">
              On every call, by the runtime
            </span>
          </div>
          <div className="mt-4">
            <Field hit={false} name="span_name" value="user.ExtractInvoice" />
            <Field
              hit={false}
              name="input_args['file']"
              type="pdf"
              value="acme_0412.pdf"
            />
            <Field hit={false} name="output_value" type="Invoice" value="" />
            <Field
              hit={reads('total')}
              indent
              name="total"
              type="float"
              value="1204.5"
            />
            <Field
              hit={reads('paid')}
              indent
              name="paid"
              type="bool"
              value="false"
            />
            <Field
              hit={reads('vendor')}
              indent
              name="vendor"
              type="string"
              value='"Acme"'
            />
            <Field
              hit={reads('path')}
              name="call path"
              value="Agent > ExtractInvoice"
            />
            <Field hit={false} name="code version" value="a41f9c2" />
          </div>
        </div>
      </div>

      <div className="border-t border-white/12">
        <div className="flex flex-wrap gap-2 p-4 md:px-7 font-sans border-b border-white/12">
          {QUERIES.map((q, i) => (
            <button
              aria-pressed={i === active}
              className={`focus-ring px-3.5 py-1.5 rounded-full border text-sm cursor-pointer transition-colors ${i === active ? 'bg-white text-black border-white' : 'border-white/20 text-white/70 hover:border-white/50'}`}
              key={q.key}
              onClick={() => {
                track('telemetry_query_selected', { query: q.key });
                setActive(i);
              }}
              type="button"
            >
              {q.label}
            </button>
          ))}
        </div>
        <div className="grid lg:grid-cols-2">
          <div className="min-w-0 p-5 md:px-7 max-lg:border-b lg:border-r border-white/12">
            {query.otel ? (
              <pre className="overflow-x-auto min-h-[80px] text-white/60">
                {highlight(query.otel)}
              </pre>
            ) : (
              <p className="min-h-[80px] font-sans text-[15px] leading-[23px] text-white/55">
                Not recorded by default.
              </p>
            )}
          </div>
          <div className="min-w-0 p-5 md:px-7">
            <pre className="overflow-x-auto min-h-[80px] text-white/90">
              {highlight(query.baml)}
            </pre>
          </div>
        </div>
      </div>
    </div>
  );
}
