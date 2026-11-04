'use client';

import logoWhite from '@boundaryml/brand/logo/svg/boundary-primary-white.svg';
import { type ReactNode, useEffect, useRef, useState } from 'react';
import { BOARD_DATA } from '../board/views';
import { track } from '../track';

// Studio's real frame around custom dashboards. A dashboard in Boundary is one HTML file hosted inside
// this frame, so each made-up company's can look nothing like the others. The sidebar labels and
// the bar come from Studio's Shell and Dashboard views, cut down to what the point needs. All the data
// is illustrative.
const CYCLE_MS = 5000;
const number = (n: number) => n.toLocaleString('en-US');

// Each company's own mark, at the top of its own dashboard. Alerts is Boundary's page, so it has none.
function InvoiceCoLogo() {
  return (
    <div className="flex items-center gap-2 mb-4 text-[15px] font-semibold tracking-[-0.02em] text-[#EDEDEF]">
      <svg aria-hidden="true" className="size-6" viewBox="0 0 24 24">
        <rect fill="#C8F03C" height={24} rx={6} width={24} />
        <path
          d="M6 7.500h12M6 12h8M6 16.500h4"
          fill="none"
          stroke="#0C0F02"
          strokeLinecap="round"
          strokeWidth={2.2}
        />
      </svg>
      InvoiceCo
    </div>
  );
}

function SupportCoLogo() {
  return (
    <div className="flex items-center gap-2 mb-4 text-[15px] font-medium tracking-[0.01em] text-[#E8EEF6]">
      <svg aria-hidden="true" className="size-6" viewBox="0 0 24 24">
        <path d="M12 1 19 8l-7 7-7-7z" fill="#3DD6B5" />
        <path d="M5 9.500l6.300 6.300L5 22.100z" fill="#6C8CFF" />
        <path d="M19 9.500l-6.300 6.300L19 22.100z" fill="#2A9D86" />
      </svg>
      SupportCo
    </div>
  );
}

function CodeAgentCoLogo() {
  return (
    <div className="flex items-center gap-2 mb-4 font-['Georgia','Times_New_Roman',serif] text-[18px] font-bold tracking-[-0.03em]">
      <svg aria-hidden="true" className="size-6" viewBox="0 0 24 24">
        <circle cx={12} cy={12} fill="#171715" r={12} />
        <path
          d="M8.500 5.500v13M8.500 13c0-3 7-1.500 7-5.500"
          fill="none"
          stroke="#FFD400"
          strokeWidth={2}
        />
        <circle cx={8.5} cy={18.5} fill="#FFD400" r={2} />
        <circle cx={15.5} cy={6.5} fill="#FFD400" r={2} />
      </svg>
      CodeAgentCo
    </div>
  );
}

// The query behind each panel, shown as you hover it. These are the same SQL a dashboard passes to
// boundary.query().
const SQL = {
  byCountry: `SELECT input_args['invoice']['country'] AS country,
  context_metadata['release'] AS release, status, count(*) AS calls
FROM spans WHERE span_name = 'user.ExtractInvoice'
GROUP BY country, release, status`,
  byMinute: `SELECT strftime('%H:%M', start_time) AS minute, status, count(*) AS calls
FROM spans WHERE span_name = 'user.ProcessInvoice'
GROUP BY minute, status`,
  cost: `SELECT context_metadata['ticket'] AS ticket,
  sum(total_usage[0]['input_tokens']) AS input_tokens
FROM spans GROUP BY ticket`,
  handoffs: `SELECT output_value['reason'] AS reason, count(*) AS handoffs
FROM spans WHERE span_name = 'user.HandOff'
GROUP BY reason`,
  outcomes: `SELECT output_value['outcome'] AS outcome, count(*) AS conversations
FROM spans WHERE span_name = 'user.HandleTicket'
GROUP BY outcome`,
  outcomesByDay: `SELECT strftime('%Y-%m-%d', start_time) AS day,
  output_value['outcome'] AS outcome, count(*) AS conversations
FROM spans WHERE span_name = 'user.HandleTicket'
GROUP BY day, outcome`,
  rating: `SELECT avg(output_value['rating']) AS rating
FROM spans WHERE span_name = 'user.RecordRating'`,
  response: `SELECT avg(duration) / 1e9 AS seconds
FROM spans WHERE span_name = 'user.FirstReply'`,
};

const funnelSql = (span: string) =>
  `SELECT context_metadata['plan'] AS plan,
  count(DISTINCT context_distinct_id) AS users
FROM spans WHERE span_name = 'user.${span}'
GROUP BY plan`;

/** A hover card over one bar of a chart, held inside the dashboard at the chart's edges. */
function Tip({
  at,
  children,
  index,
  total,
}: {
  at: number;
  children: ReactNode;
  index: number;
  total: number;
}) {
  const edge =
    index < total * 0.25
      ? 'left-0'
      : index > total * 0.75
        ? 'right-0'
        : 'left-1/2 -translate-x-1/2';
  return (
    <div
      className={`pointer-events-none absolute z-20 mb-1.5 hidden group-hover:block rounded-md border border-white/15 bg-[#16161A] px-2.5 py-1.5 text-left font-sans text-[12px] leading-[18px] normal-case tracking-normal whitespace-nowrap text-[#EDEDEF] shadow-[0_8px_24px_rgb(0_0_0/0.45)] ${edge}`}
      style={{ bottom: `${at}%` }}
    >
      {children}
    </div>
  );
}

function TipRow({
  color,
  label,
  value,
}: {
  color: string;
  label: string;
  value: string;
}) {
  return (
    <div className="flex items-center gap-3">
      <span className="flex items-center gap-1.5 text-white/70">
        <span className="size-2 rounded-[2px]" style={{ background: color }} />
        {label}
      </span>
      <span className="ml-auto">{value}</span>
    </div>
  );
}

/** A small trend line, sized by its box. */
function Spark({
  color,
  points,
  className = 'w-20 h-7',
}: {
  className?: string;
  color: string;
  points: number[];
}) {
  const max = Math.max(...points);
  const min = Math.min(...points);
  const line = points
    .map(
      (p, i) =>
        `${(i / (points.length - 1)) * 100},${28 - ((p - min) / (max - min || 1)) * 24}`,
    )
    .join(' ');
  return (
    <svg
      aria-hidden="true"
      className={className}
      preserveAspectRatio="none"
      viewBox="0 0 100 30"
    >
      <polyline
        fill="none"
        points={line}
        stroke={color}
        strokeWidth={1.5}
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

// SupportCo: how a support chatbot is doing, from the runs it recorded.
const OUTCOMES: [string, string][] = [
  ['Resolved by the agent', '#3DD6B5'],
  ['Handed to a person', '#6C8CFF'],
  ['Abandoned', '#47566B'],
];
const TICKET_DAYS: [number, number, number][] = [
  [62, 22, 9],
  [66, 21, 8],
  [64, 20, 10],
  [70, 19, 7],
  [73, 18, 8],
  [52, 14, 6],
  [48, 12, 5],
  [75, 18, 7],
  [78, 17, 6],
  [81, 16, 7],
  [79, 15, 6],
  [84, 14, 6],
  [60, 10, 4],
  [55, 9, 4],
];
const ESCALATIONS: [string, number][] = [
  ['Refund over the limit', 31],
  ['Account access', 24],
  ['Customer asked for a person', 18],
  ['No matching policy', 15],
  ['Tool call failed', 12],
];

function Support() {
  const peak = Math.max(...TICKET_DAYS.map((d) => d[0] + d[1] + d[2]));
  return (
    <div className="h-full p-5 md:p-8 bg-[#0B1524] text-[#E8EEF6] overflow-hidden">
      <SupportCoLogo />
      <div className="flex items-start justify-between gap-4">
        <div>
          <div className="text-[24px] md:text-[32px] font-semibold leading-[1.1] tracking-[-0.025em]">
            Support agent
          </div>
          <div className="mt-1 text-[14px] md:text-[16px] text-[#E8EEF6]/60">
            Every customer conversation the agent handled.
          </div>
        </div>
        <div className="flex shrink-0 gap-1.5 text-[13px] max-sm:hidden">
          <span className="rounded-md border border-white/15 px-2.5 py-1">
            All channels
          </span>
          <span className="rounded-md border border-white/15 px-2.5 py-1">
            Last 14 days
          </span>
        </div>
      </div>
      <div className="grid grid-cols-3 md:grid-cols-4 gap-2.5 mt-5">
        {(
          [
            [
              '68.4%',
              'Resolved by the agent',
              '+4.1 pts',
              [5, 5, 6, 6, 7, 8, 9],
            ],
            ['4.2 s', 'First response', '-0.6 s', [9, 8, 8, 6, 6, 5, 4]],
            ['4.6 / 5', 'Customer rating', '+0.2', [5, 6, 5, 7, 7, 8, 8]],
            ['$0.18', 'Model cost a ticket', '-9%', [9, 8, 7, 7, 6, 5, 5]],
          ] as [string, string, string, number[]][]
        ).map(([value, label, change, points], i) => (
          <div
            className={`rounded-lg bg-white/5 p-3 md:p-4 transition-colors hover:bg-white/10 ${i === 3 ? 'max-md:hidden' : ''}`}
            data-sql={[SQL.outcomes, SQL.response, SQL.rating, SQL.cost][i]}
            key={label}
          >
            <div className="flex items-end justify-between gap-2">
              <span className="text-[20px] md:text-[30px] font-semibold leading-none tracking-[-0.02em]">
                {value}
              </span>
              <Spark
                className="w-14 h-6 max-lg:hidden"
                color="#3DD6B5"
                points={points}
              />
            </div>
            <div className="mt-2 text-[12px] md:text-[13.5px] leading-tight text-[#E8EEF6]/60">
              {label}{' '}
              <span className="text-[#3DD6B5] max-md:hidden">{change}</span>
            </div>
          </div>
        ))}
      </div>
      <div className="grid md:grid-cols-[1.5fr_1fr] gap-2.5 mt-2.5">
        <div
          className="min-w-0 rounded-lg bg-white/5 p-4"
          data-sql={SQL.outcomesByDay}
        >
          <div className="text-[15px] font-medium leading-5">
            Conversations by outcome
          </div>
          <div className="flex flex-wrap gap-x-4 mt-2 text-[12.5px] text-[#E8EEF6]/60">
            {OUTCOMES.map(([outcome, color]) => (
              <span className="flex items-center gap-1.5" key={outcome}>
                <span
                  className="size-2.5 rounded-[2px]"
                  style={{ background: color }}
                />
                {outcome}
              </span>
            ))}
          </div>
          <div className="flex items-end gap-1.5 h-44 mt-3">
            {TICKET_DAYS.map((day, i) => {
              const height = ((day[0] + day[1] + day[2]) / peak) * 100;
              return (
                <div
                  className="group relative flex items-end flex-1 min-w-0 h-full"
                  // biome-ignore lint/suspicious/noArrayIndexKey: one bar per day
                  key={i}
                >
                  <div
                    className="v2-rise flex flex-col-reverse w-full gap-px overflow-hidden rounded-t-[3px] transition-[filter] group-hover:brightness-125"
                    style={
                      {
                        '--delay': `${i * 0.025}s`,
                        height: `${height}%`,
                      } as React.CSSProperties
                    }
                  >
                    {day.map((count, o) => (
                      <div
                        key={OUTCOMES[o][0]}
                        style={{ background: OUTCOMES[o][1], flexGrow: count }}
                      />
                    ))}
                  </div>
                  <Tip at={height} index={i} total={TICKET_DAYS.length}>
                    <div className="font-medium">Oct {17 + i}</div>
                    {day.map((count, o) => (
                      <TipRow
                        color={OUTCOMES[o][1]}
                        key={OUTCOMES[o][0]}
                        label={OUTCOMES[o][0]}
                        value={number(count * 10)}
                      />
                    ))}
                  </Tip>
                </div>
              );
            })}
          </div>
        </div>
        <div
          className="min-w-0 rounded-lg bg-white/5 p-4"
          data-sql={SQL.handoffs}
        >
          <div className="text-[15px] font-medium leading-5">
            Why the agent handed off
          </div>
          <ul className="flex flex-col gap-3 mt-3">
            {ESCALATIONS.map(([reason, share], i) => (
              <li
                className="group -mx-1.5 rounded px-1.5 py-1 transition-colors hover:bg-white/8"
                key={reason}
              >
                <div className="flex justify-between gap-2 text-[13px] leading-4">
                  <span className="truncate">{reason}</span>
                  <span className="shrink-0 text-[#E8EEF6]/60">{share}%</span>
                </div>
                <div className="h-1.5 mt-1.5 rounded-full bg-white/8">
                  <div
                    className="v2-grow h-full rounded-full bg-[#6C8CFF] transition-[filter] group-hover:brightness-125"
                    style={
                      {
                        '--delay': `${i * 0.06}s`,
                        width: `${(share / ESCALATIONS[0][1]) * 100}%`,
                      } as React.CSSProperties
                    }
                  />
                </div>
              </li>
            ))}
          </ul>
        </div>
      </div>
    </div>
  );
}

// The board Boundary's own dashboards start from: a chart of executions with the deploy marked, and what
// changed by segment. The numbers are the original product shot's.
function Board({
  chart,
  chartQuery,
  compare,
  compareQuery,
  meta,
  rows,
  subtitle,
  title,
}: {
  chart: string;
  chartQuery: string;
  compare: string;
  compareQuery: string;
  meta: string;
  rows: [string, number, number][];
  subtitle: string;
  title: string;
}) {
  const bars = BOARD_DATA.bars;
  const peak = Math.max(...bars.map((b) => b.ok + b.bad));
  const widest = Math.max(...rows.map((r) => r[2]));
  return (
    <div
      className={'relative h-full overflow-hidden text-[#EDEDEF]'}
      style={{ background: '#09090A' }}
    >
      <div className="p-5 md:p-8">
        <InvoiceCoLogo />
        <div className="flex items-start justify-between gap-4">
          <div className="min-w-0">
            <div
              className={
                'text-[24px] md:text-[34px] leading-[1.1] font-semibold tracking-[-0.02em]'
              }
            >
              {title}
            </div>
            <div className={'mt-1 text-[13.5px] md:text-[15px] text-[#72727B]'}>
              {subtitle}
            </div>
          </div>
          <span
            className={
              'shrink-0 border rounded-lg border-[#2D2D34] text-[#A4A4AD]  px-2.5 py-1 text-[12.5px] max-sm:hidden'
            }
          >
            Last 60 min
          </span>
        </div>
        <div className="grid md:grid-cols-[1.65fr_1fr] gap-3 mt-5">
          <div
            className="min-w-0 border border-[#212126] rounded-lg bg-[#101012] p-4"
            data-sql={SQL.byMinute}
          >
            <div className="text-[15px] font-medium leading-5">{chart}</div>
            <div
              className={'mt-0.5 truncate text-[12px] font-mono text-[#72727B]'}
            >
              {chartQuery}
            </div>
            <div
              className={
                'flex flex-wrap items-center gap-x-4 mt-3 text-[12.5px] text-[#72727B]'
              }
            >
              <span className="flex items-center gap-1.5">
                <span className="size-2.5" style={{ background: '#4D4D55' }} />
                succeeded
              </span>
              <span className="flex items-center gap-1.5">
                <span className="size-2.5" style={{ background: '#D03B3B' }} />
                failed
              </span>
              <span className="ml-auto max-sm:hidden">{meta}</span>
            </div>
            <div className="relative flex items-end gap-[2px] h-44 mt-7">
              {bars.map((b, i) => {
                const height = ((b.ok + b.bad) / peak) * 100;
                return (
                  <div
                    className="group relative flex items-end flex-1 min-w-0 h-full"
                    // biome-ignore lint/suspicious/noArrayIndexKey: one bar per minute
                    key={i}
                  >
                    <div
                      className="v2-rise flex flex-col justify-end w-full transition-[filter] group-hover:brightness-[1.6]"
                      style={
                        {
                          '--delay': `${i * 0.008}s`,
                          height: `${height}%`,
                        } as React.CSSProperties
                      }
                    >
                      <div
                        style={{
                          background: '#D03B3B',
                          height: `${(b.bad / (b.ok + b.bad)) * 100}%`,
                        }}
                      />
                      <div
                        className="flex-1"
                        style={{ background: '#4D4D55' }}
                      />
                    </div>
                    <Tip at={height} index={i} total={bars.length}>
                      <div className="font-medium">{b.t} UTC</div>
                      <TipRow
                        color="#8A8A94"
                        label="succeeded"
                        value={String(b.ok)}
                      />
                      <TipRow
                        color="#D03B3B"
                        label="failed"
                        value={String(b.bad)}
                      />
                    </Tip>
                  </div>
                );
              })}
              <div
                className={
                  'pointer-events-none absolute -top-1 -bottom-1 border-l border-dashed border-[#212126]'
                }
                style={{ left: `${(26 / bars.length) * 100}%` }}
              >
                <span
                  className={
                    'absolute -top-5 -translate-x-1/2 whitespace-nowrap text-[11.5px] font-mono text-[#72727B]'
                  }
                >
                  r42 deployed
                </span>
              </div>
            </div>
          </div>
          <div
            className="min-w-0 border border-[#212126] rounded-lg bg-[#101012] p-4"
            data-sql={SQL.byCountry}
          >
            <div className="text-[15px] font-medium leading-5">{compare}</div>
            <div
              className={'mt-0.5 truncate text-[12px] font-mono text-[#72727B]'}
            >
              {compareQuery}
            </div>
            <div
              className={
                'flex items-center gap-3 mt-3 text-[12.5px] font-mono text-[#72727B]'
              }
            >
              <span className="flex items-center gap-1.5">
                <span className="size-2.5" style={{ background: '#4D4D55' }} />
                r41
              </span>
              <span className="flex items-center gap-1.5">
                <span className="size-2.5" style={{ background: '#C8F03C' }} />
                r42
              </span>
            </div>
            <ul className="flex flex-col gap-2.5 mt-3">
              {rows.map(([name, before, after], i) => (
                <li
                  className="-mx-1.5 rounded px-1.5 py-1 transition-colors hover:bg-white/8"
                  key={name}
                >
                  <div
                    className={
                      'flex justify-between gap-2 text-[12.5px] leading-4 font-mono'
                    }
                  >
                    <span className="truncate">{name}</span>
                    <span className={'shrink-0 text-[#72727B]'}>
                      {before}% to {after}%
                    </span>
                  </div>
                  <div className="flex flex-col gap-px mt-1">
                    <div
                      className="h-1"
                      style={{
                        background: '#4D4D55',
                        width: `${Math.max((before / widest) * 100, 1)}%`,
                      }}
                    />
                    <div
                      className="v2-grow h-1.5"
                      style={
                        {
                          '--delay': `${i * 0.06}s`,
                          background: '#C8F03C',
                          width: `${(after / widest) * 100}%`,
                        } as React.CSSProperties
                      }
                    />
                  </div>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}

function BoundaryDefault() {
  return (
    <Board
      chart="ProcessInvoice executions per minute"
      chartQuery="spans, succeeded vs failed"
      compare="ExtractInvoice error rate by country"
      compareQuery="spans, r41 vs r42"
      meta="Last completed 26 s ago, refreshes every 60 s"
      rows={[
        ['DE', 0.4, 27],
        ['NL', 0.7, 10],
        ['FR', 1.5, 7.4],
        ['US', 1.3, 2.3],
        ['GB', 0.6, 2],
      ]}
      subtitle="Every function in production, against your last deploy."
      title="Invoice pipeline"
    />
  );
}

// CodeAgentCo's brand: paper, ink, a heavy serif, and bands of flat color with hatching.
const BANDS: [string, number][] = [
  ['#FFD400', 3],
  ['#FF0900', 1.2],
  ['#EF6B00', 2],
  ['#54B948', 1.5],
  ['#5BC3E4', 1.5],
  ['#EF81CA', 2],
  ['#5866CA', 2.5],
];

// A funnel in the style product analytics tools use, built from each user's agent runs. Each bar is split
// by plan, and the hatching above it is who dropped off at that step.
const FUNNEL: {
  count: number;
  median: string;
  plans: [number, number, number];
  span: string;
  step: string;
}[] = [
  {
    count: 12480,
    median: '',
    plans: [62, 28, 10],
    span: 'SignUp',
    step: 'Signed up',
  },
  {
    count: 8112,
    median: '6 min',
    plans: [56, 31, 13],
    span: 'ConnectRepo',
    step: 'Connected a repo',
  },
  {
    count: 6240,
    median: '4 min',
    plans: [52, 33, 15],
    span: 'OpenPullRequest',
    step: 'Agent opened a PR',
  },
  {
    count: 5116,
    median: '2 min',
    plans: [49, 35, 16],
    span: 'RunCi',
    step: 'PR passed CI',
  },
  {
    count: 2921,
    median: '1 h 4 min',
    plans: [41, 38, 21],
    span: 'MergePullRequest',
    step: 'PR merged',
  },
];
const PLANS: [string, string][] = [
  ['Free', '#FFD400'],
  ['Team', '#EF6B00'],
  ['Enterprise', '#5866CA'],
];

function CodeAgentCo() {
  const first = FUNNEL[0].count;
  return (
    <div className="h-full bg-[#E6E3D3] text-[#171715] overflow-hidden">
      <div className="flex h-5 border-b border-[#171715]">
        {BANDS.map(([color, grow]) => (
          <span
            key={color}
            style={{
              background: `repeating-linear-gradient(135deg, ${color} 0 7px, rgb(23 23 21 / 0.35) 7px 9px, ${color} 9px 14px)`,
              flexGrow: grow,
            }}
          />
        ))}
      </div>
      <div className="p-5 md:p-8">
        <CodeAgentCoLogo />
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="font-['Georgia','Times_New_Roman',serif] text-[24px] md:text-[34px] font-bold leading-[1.1] tracking-[-0.04em]">
              Activation funnel
            </div>
            <div className="mt-1 text-[13.5px] md:text-[15px] text-[#171715]/65">
              <span className="font-medium text-[#171715]">23.4%</span> of new
              users merge their first agent PR. Median time 1 h 12 min.
            </div>
          </div>
          <span className="shrink-0 bg-[#FFD400] px-3 py-1.5 font-mono text-[12px] uppercase tracking-[0.06em] max-sm:hidden">
            Add drop-offs to evals
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-1.5 mt-4 font-mono text-[11.5px] uppercase tracking-[0.04em]">
          {[
            'Last 30 days',
            'Within 14 days',
            'Breakdown: plan',
            'Unique users',
          ].map((filter) => (
            <span
              className="border border-[#171715] px-2 py-1 transition-colors hover:bg-[#171715] hover:text-[#E6E3D3]"
              key={filter}
            >
              {filter}
            </span>
          ))}
          <span className="flex items-center gap-3 ml-auto max-md:hidden">
            {PLANS.map(([plan, color]) => (
              <span className="flex items-center gap-1.5" key={plan}>
                <span className="size-2.5" style={{ background: color }} />
                {plan}
              </span>
            ))}
          </span>
        </div>
        <div className="grid grid-cols-3 md:grid-cols-5 gap-px mt-4 border border-[#171715] bg-[#171715]">
          {FUNNEL.map((s, i) => {
            const previous = i > 0 ? FUNNEL[i - 1].count : s.count;
            const height = (s.count / first) * 100;
            const lost = (previous / first) * 100 - height;
            return (
              <div
                className={`group min-w-0 bg-[#E6E3D3] p-3 transition-colors hover:bg-[#F1EEDF] ${i > 2 ? 'max-md:hidden' : ''}`}
                data-sql={funnelSql(s.span)}
                key={s.step}
              >
                <div className="flex items-center gap-1.5 text-[12.5px] font-medium leading-4">
                  <span className="grid place-items-center size-4 shrink-0 bg-[#171715] font-mono text-[10.5px] text-[#E6E3D3]">
                    {i + 1}
                  </span>
                  <span className="truncate">{s.step}</span>
                </div>
                <div className="flex flex-wrap items-baseline gap-x-1.5 mt-2">
                  <span className="font-['Georgia','Times_New_Roman',serif] text-[20px] md:text-[26px] font-bold leading-none tracking-[-0.03em]">
                    {((s.count / previous) * 100).toFixed(i ? 1 : 0)}%
                  </span>
                  <span className="font-mono text-[11.5px] text-[#171715]/60">
                    {number(s.count)}
                  </span>
                </div>
                <div className="relative flex flex-col justify-end h-32 mt-3 bg-[#171715]/5">
                  <div
                    style={{
                      background:
                        'repeating-linear-gradient(135deg, rgb(23 23 21 / 0.3) 0 2px, transparent 2px 6px)',
                      height: `${lost}%`,
                    }}
                  />
                  <div
                    className="v2-rise flex flex-col-reverse overflow-hidden"
                    style={
                      {
                        '--delay': `${i * 0.06}s`,
                        height: `${height}%`,
                      } as React.CSSProperties
                    }
                  >
                    {s.plans.map((share, p) => (
                      <div
                        key={PLANS[p][0]}
                        style={{
                          background: PLANS[p][1],
                          height: `${share}%`,
                        }}
                      />
                    ))}
                  </div>
                  <Tip at={height} index={i} total={FUNNEL.length}>
                    <div className="font-medium">{s.step}</div>
                    {s.plans.map((share, p) => (
                      <TipRow
                        color={PLANS[p][1]}
                        key={PLANS[p][0]}
                        label={PLANS[p][0]}
                        value={number(Math.round((s.count * share) / 100))}
                      />
                    ))}
                  </Tip>
                </div>
                <div className="mt-2.5 text-[12px] leading-[18px] text-[#171715]/65">
                  <div>
                    {i
                      ? `${number(previous - s.count)} dropped`
                      : 'Start of funnel'}
                  </div>
                  <div>{s.median ? `Median ${s.median}` : 'All new users'}</div>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

// Alerts are part of Boundary, not a custom dashboard, so every company gets the same page.
type Alert = {
  channel: string;
  condition: string;
  firing?: boolean;
  last: string;
  name: string;
  query: string;
};

// Query is part of Boundary too, so every company gets the same page. Each query uses the tables baml
// query reads; the result rows are example data.
type SavedQuery = {
  columns: string[];
  ms: number;
  rows: string[][];
  sql: string;
};

function highlightSql(sql: string): ReactNode[] {
  return sql
    .split(/(\b(?:SELECT|FROM|WHERE|GROUP BY|AND|AS|IS NULL)\b|'[^']*')/)
    .map((part, i) => {
      const key = `${i}-${part}`;
      if (part.startsWith("'"))
        return (
          <span className="text-[#E2B77C]" key={key}>
            {part}
          </span>
        );
      return /^(SELECT|FROM|WHERE|GROUP BY|AND|AS|IS NULL)$/.test(part) ? (
        <span className="text-[#B8A8FF]" key={key}>
          {part}
        </span>
      ) : (
        part
      );
    });
}

function QueryPage({ query }: { query: SavedQuery }) {
  const [running, setRunning] = useState(false);
  useEffect(() => {
    if (!running) return;
    const id = setTimeout(() => setRunning(false), 550);
    return () => clearTimeout(id);
  }, [running]);
  return (
    <div className="flex flex-col h-full overflow-hidden bg-[#09090A] text-[#EDEDEF]">
      <pre className="min-h-0 px-4 py-3 overflow-x-auto font-mono text-[13px] md:text-[14px] leading-[23px] text-[#EDEDEF]">
        {highlightSql(query.sql)}
      </pre>
      <div className="flex shrink-0 items-center gap-2 h-10 px-3 border-y border-[#212126] text-[13px]">
        <span className="text-[#72727B] max-sm:hidden">
          spans, profiler, processes
        </span>
        <button
          className="focus-ring ml-auto rounded-md bg-[#C8F03C] px-3 py-1 font-medium text-[#0C0F02] cursor-pointer transition hover:bg-[#D4F55E] active:translate-y-px"
          onClick={() => {
            track('dashboard_demo_query_run');
            setRunning(true);
          }}
          type="button"
        >
          {running ? 'Running...' : 'Run'}
        </button>
      </div>
      <div className="flex shrink-0 items-center gap-3 h-9 px-4 border-b border-[#212126] font-mono text-[12px] text-[#A4A4AD]">
        {running ? (
          'Running'
        ) : (
          <>
            <span>Complete</span>
            <span>{query.rows.length} rows</span>
            <span>{query.ms} ms</span>
          </>
        )}
      </div>
      <div
        className={`min-h-0 overflow-hidden px-4 transition-opacity duration-150 ${running ? 'opacity-30' : ''}`}
      >
        <table className="w-full text-left font-mono text-[12px] md:text-[13px]">
          <thead className="text-[#72727B]">
            <tr>
              {query.columns.map((c) => (
                <th className="py-2 pr-4 font-normal" key={c} scope="col">
                  {c}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {query.rows.map((row) => (
              <tr
                className="border-t border-[#212126] transition-colors hover:bg-white/5"
                key={row[0]}
              >
                {row.map((cell, i) => (
                  <td
                    className="py-2 pr-4 whitespace-nowrap"
                    key={query.columns[i]}
                  >
                    {cell}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

const COMPANIES: {
  about: string;
  alerts: Alert[];
  board: string;
  initials: string;
  key: string;
  name: string;
  project: string;
  query: SavedQuery;
  render: () => ReactNode;
}[] = [
  {
    about: 'Invoice extraction',
    alerts: [
      {
        channel: '#invoices-oncall',
        condition: 'errors > 10 in 5 min',
        firing: true,
        last: '2 min ago',
        name: 'ExtractInvoice is failing',
        query: "status = 'user_error'",
      },
      {
        channel: '#invoices-oncall',
        condition: 'avg_ms > 800',
        last: '3 days ago',
        name: 'Extraction is slow',
        query: 'avg(duration) / 1e6',
      },
      {
        channel: '#finance',
        condition: 'tokens > 2M a day',
        last: '2 weeks ago',
        name: 'Daily model spend',
        query: "sum(total_usage[0]['input_tokens'])",
      },
    ],
    board: 'Invoice pipeline',
    initials: 'IC',
    key: 'invoiceco',
    name: 'InvoiceCo',
    project: 'invoices',
    query: {
      columns: ['span_name', 'calls', 'avg_ms'],
      ms: 318,
      rows: [
        ['user.ProcessInvoice', '8412', '612'],
        ['user.ExtractInvoice', '8412', '401'],
        ['user.ValidateTotals', '8390', '42'],
        ['user.NotifyApprover', '1264', '88'],
      ],
      sql: `SELECT span_name, count(*) AS calls,
  avg(duration) / 1e6 AS avg_ms
FROM spans
GROUP BY span_name`,
    },
    render: () => <BoundaryDefault />,
  },
  {
    about: 'Support chatbot analytics',
    alerts: [
      {
        channel: '#support-ai',
        condition: 'share > 25% in 1 h',
        firing: true,
        last: '14 min ago',
        name: 'Handoffs are climbing',
        query: "span_name = 'user.HandOff'",
      },
      {
        channel: '#support-ai',
        condition: 'errors > 5 in 5 min',
        last: 'Yesterday',
        name: 'A tool call is failing',
        query: "status = 'user_error'",
      },
      {
        channel: '#customer-success',
        condition: 'tokens > 500K a day',
        last: '5 days ago',
        name: 'One customer is spending fast',
        query: "context_metadata['customer']",
      },
    ],
    board: 'Support agent',
    initials: 'SC',
    key: 'supportco',
    name: 'SupportCo',
    project: 'support-agent',
    query: {
      columns: ['reason', 'handoffs'],
      ms: 204,
      rows: [
        ['refund_over_limit', '412'],
        ['account_access', '319'],
        ['asked_for_person', '240'],
        ['no_matching_policy', '199'],
      ],
      sql: `SELECT output_value['reason'] AS reason,
  count(*) AS handoffs
FROM spans
WHERE span_name = 'user.HandOff'
GROUP BY reason`,
    },
    render: () => <Support />,
  },
  {
    about: 'Coding agent activation',
    alerts: [
      {
        channel: '#agent-runs',
        condition: 'errors > 20 in 10 min',
        firing: true,
        last: '6 min ago',
        name: 'OpenPullRequest is failing',
        query: "status = 'user_error'",
      },
      {
        channel: '#agent-runs',
        condition: 'avg_ms > 90000',
        last: '2 days ago',
        name: 'Agent runs are slow',
        query: 'avg(duration) / 1e6',
      },
      {
        channel: '#growth',
        condition: 'tokens > 5M a day',
        last: 'Last week',
        name: 'Free plan model spend',
        query: "context_metadata['plan']",
      },
    ],
    board: 'Activation funnel',
    initials: 'CA',
    key: 'codeagentco',
    name: 'CodeAgentCo',
    project: 'coding-agent',
    query: {
      columns: ['plan', 'runs', 'input_tokens'],
      ms: 1410,
      rows: [
        ['free', '18204', '41200318'],
        ['team', '9630', '38455102'],
        ['enterprise', '3117', '17903644'],
      ],
      sql: `SELECT context_metadata['plan'] AS plan,
  count(*) AS runs,
  sum(total_usage[0]['input_tokens']) AS input_tokens
FROM spans
WHERE parent_span_id IS NULL
GROUP BY plan`,
    },
    render: () => <CodeAgentCo />,
  },
];

function Alerts({ alerts }: { alerts: Alert[] }) {
  return (
    <div className="h-full overflow-hidden bg-[#09090A] p-5 md:p-8 text-[#EDEDEF]">
      <div className="flex items-center justify-between gap-4">
        <div className="text-[22px] md:text-[26px] font-semibold tracking-[-0.02em]">
          Alerts
        </div>
        <span className="rounded-md bg-[#C8F03C] px-3 py-1.5 text-[13px] font-medium text-[#0C0F02]">
          New alert
        </span>
      </div>
      <div className="mt-1 text-[14px] text-[#A4A4AD]">
        Run a query on a schedule and post to Slack when it crosses a line.
      </div>
      <ul className="mt-5 flex flex-col rounded-lg border border-[#212126] bg-[#101012]">
        {alerts.map((a) => (
          <li
            className="grid md:grid-cols-[1.3fr_1.3fr_auto] items-center gap-x-5 gap-y-1 p-4 border-b border-[#212126] last:border-b-0"
            key={a.name}
          >
            <div className="min-w-0">
              <div className="flex items-center gap-2 text-[15px] font-medium">
                <span
                  className="size-2 shrink-0 rounded-full"
                  style={{ background: a.firing ? '#D03B3B' : '#0CA30C' }}
                />
                <span className="truncate">{a.name}</span>
              </div>
              <div className="mt-1 ml-4 text-[13px] text-[#72727B]">
                {a.firing ? 'Firing' : 'OK'}, last fired {a.last}
              </div>
            </div>
            <div className="min-w-0 ml-4 md:ml-0">
              <div className="truncate font-mono text-[12.5px] text-[#A4A4AD]">
                {a.query}
              </div>
              <div className="mt-1 truncate font-mono text-[12.5px] text-[#72727B]">
                {a.condition}
              </div>
            </div>
            <span className="ml-4 md:ml-0 text-[13px] text-[#A4A4AD]">
              {a.channel}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Icon({ d }: { d: string }) {
  return (
    <svg
      aria-hidden="true"
      className="size-3.5 shrink-0"
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth={1.4}
      viewBox="0 0 16 16"
    >
      <path d={d} />
    </svg>
  );
}

const ICONS = {
  alerts:
    'M8 2.500a3.800 3.800 0 0 0-3.800 3.800c0 3-1.200 4.200-1.200 4.200h10s-1.200-1.200-1.200-4.200A3.800 3.800 0 0 0 8 2.500zM6.800 13a1.300 1.300 0 0 0 2.400 0',
  dashboards:
    'M2.5 2.5h4.5v4.5h-4.5zM9 2.5h4.5v4.5H9zM2.5 9h4.5v4.5h-4.5zM9 9h4.5v4.5H9z',
  fullscreen: 'M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10',
  key: 'M10 2.5a3.5 3.5 0 1 1-2.9 5.4L2.5 12.5v1h2v-1.500h1.500v-1.500h1.500',
  link: 'M6.5 9.500l3-3M7 4.500l1-1a2.800 2.800 0 0 1 4 4l-1 1M9 11.500l-1 1a2.800 2.800 0 0 1-4-4l1-1',
  query:
    'M2.500 4c0-1 2.400-1.800 5.500-1.800S13.500 3 13.500 4v8c0 1-2.400 1.800-5.500 1.800S2.500 13 2.500 12zM2.500 8c0 1 2.400 1.800 5.500 1.800S13.500 9 13.500 8',
  sidebar: 'M2.500 3h11v10h-11zM6.500 3v10',
  upload: 'M8 10.500V3M5 6l3-3 3 3M3 13h10',
};

function NavItem({
  active,
  icon,
  label,
  onClick,
}: {
  active?: boolean;
  icon: string;
  label: string;
  onClick?: () => void;
}) {
  const style = `flex items-center gap-2.5 w-full h-8 px-2.5 rounded-md text-left text-[13.5px] ${active ? 'bg-[#1C1C21] text-[#EDEDEF]' : 'text-[#A4A4AD]'}`;
  return onClick ? (
    <button
      aria-pressed={active}
      className={`focus-ring cursor-pointer transition-colors hover:text-[#EDEDEF] ${style}`}
      onClick={onClick}
      type="button"
    >
      <Icon d={icon} />
      {label}
    </button>
  ) : (
    <div className={style}>
      <Icon d={icon} />
      {label}
    </div>
  );
}

/** Types out the query behind whatever is hovered, so the dashboard reads as code over a data API. */
function QueryStrip({ sql }: { sql: string | null }) {
  const [chars, setChars] = useState(0);
  useEffect(() => {
    if (!sql) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setChars(sql.length);
      return;
    }
    setChars(0);
    const id = setInterval(
      () =>
        setChars((c) => {
          if (c >= sql.length) clearInterval(id);
          return Math.min(c + 3, sql.length);
        }),
      12,
    );
    return () => clearInterval(id);
  }, [sql]);
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-0 z-30 h-[104px] border-t border-[#2D2D34] bg-[#09090A]/96 px-4 py-2.5 font-mono text-[11.5px] md:text-[12.5px] leading-[19px] text-[#EDEDEF] overflow-hidden">
      {sql ? (
        <pre className="whitespace-pre-wrap">
          <span className="text-[#C8F03C]">boundary.query</span>
          <span className="text-[#72727B]">(`</span>
          {highlightSql(sql.slice(0, chars))}
          <span className="text-[#72727B]">`)</span>
        </pre>
      ) : (
        <p className="text-[#A4A4AD]">
          <span className="text-[#C8F03C]">boundary.query</span>(sql)
          <span className="block mt-1 font-sans text-[13px] text-[#72727B]">
            Every panel is a SQL query. Hover one to see it.
          </span>
        </p>
      )}
    </div>
  );
}

type View = 'dashboards' | 'alerts' | 'query';
const VIEW_LABELS: Record<View, string> = {
  alerts: 'Alerts',
  dashboards: 'Dashboards',
  query: 'Query',
};

export default function Dashboards() {
  const [active, setActive] = useState(0);
  const [held, setHeld] = useState(false);
  const [view, setView] = useState<View>('dashboards');
  const [hovering, setHovering] = useState(false);
  const [sql, setSql] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);

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
            () => setActive((a) => (a + 1) % COMPANIES.length),
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

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new dashboard starts with no query shown
  useEffect(() => setSql(null), [active, view]);

  const company = COMPANIES[active];
  const reveal = (e: { target: EventTarget }) =>
    setSql(
      (e.target as Element).closest('[data-sql]')?.getAttribute('data-sql') ??
        null,
    );
  const open = (next: View) => {
    track('dashboard_demo_view', { company: company.key, view: next });
    setView(next);
    setHeld(true);
  };

  return (
    <div className="relative max-w-[1240px] mx-auto" ref={ref}>
      <h3 className="max-w-[28em] mx-auto text-center font-display font-medium text-[22px] md:text-[32px] leading-[1.2] tracking-[-0.03em] text-balance text-white">
        Build any dashboard, from support chatbot analytics to invoice
        extraction evals.
      </h3>
      <div className="grid grid-cols-3 gap-3 md:gap-5 mt-6 md:mt-8 mb-4">
        {COMPANIES.map((c, i) => (
          <button
            aria-pressed={i === active}
            className={`focus-ring relative pt-3 text-left cursor-pointer transition-colors ${i === active ? 'text-white' : 'text-white/70 hover:text-white'}`}
            key={c.key}
            onClick={() => {
              track('dashboard_demo_selected', { company: c.key });
              setActive(i);
              setHeld(true);
            }}
            type="button"
          >
            <span className="absolute inset-x-0 top-0 h-0.5 bg-white/30">
              {i === active && (
                <span
                  className={`block h-full bg-white ${held || hovering ? '' : 'v2-countdown'}`}
                  key={active}
                  style={{ '--ms': `${CYCLE_MS}ms` } as React.CSSProperties}
                />
              )}
            </span>
            <span className="block font-display font-medium text-[14px] md:text-[17px] leading-5 tracking-[-0.02em]">
              {c.name}
            </span>
            <span className="block mt-0.5 text-[12px] md:text-[13.5px] leading-[18px] max-sm:hidden">
              {c.about}
            </span>
          </button>
        ))}
      </div>

      {/* Runs off the bottom of the painting, so the top of the app is all that shows. */}
      <div
        className="h-[500px] md:h-[580px] flex rounded-t-[12px] overflow-hidden border border-b-0 border-[#2D2D34] bg-[#09090A] shadow-[0_40px_120px_-20px_rgb(0_0_0/0.8)]"
        onMouseEnter={() => setHovering(true)}
        onMouseLeave={() => setHovering(false)}
      >
        <div className="hidden lg:flex flex-col gap-1 w-[216px] shrink-0 p-3 border-r border-[#212126] bg-[#101012]">
          <div className="flex items-center justify-between h-10 pl-1.5 pr-1 mb-3 text-[#72727B]">
            {/* biome-ignore lint/performance/noImgElement: the brand wordmark is an SVG */}
            <img alt="Boundary" className="h-4.5" src={logoWhite.src} />
            <Icon d={ICONS.sidebar} />
          </div>
          <div className="px-2.5 py-1.5 text-[12px] text-[#72727B]">
            Project
          </div>
          <NavItem
            active={view === 'dashboards'}
            icon={ICONS.dashboards}
            label="Dashboards"
            onClick={() => open('dashboards')}
          />
          <NavItem
            active={view === 'alerts'}
            icon={ICONS.alerts}
            label="Alerts"
            onClick={() => open('alerts')}
          />
          <NavItem icon={ICONS.key} label="API keys" />
          <div className="px-2.5 pt-4 pb-1.5 text-[12px] text-[#72727B]">
            Environment
          </div>
          <NavItem
            active={view === 'query'}
            icon={ICONS.query}
            label="Query"
            onClick={() => open('query')}
          />
        </div>
        <div className="relative flex flex-col flex-1 min-w-0">
          <div className="flex items-center gap-2 h-11 shrink-0 px-4 border-b border-[#212126] bg-[#09090A] text-[13.5px] text-[#A4A4AD]">
            {/* biome-ignore lint/performance/noImgElement: the brand wordmark is an SVG */}
            <img
              alt="Boundary"
              className="h-4 mr-2 lg:hidden max-sm:hidden"
              src={logoWhite.src}
            />
            <span className="grid place-items-center size-5 shrink-0 rounded-[5px] bg-[#25252B] text-[9.5px] text-[#EDEDEF]">
              {company.initials}
            </span>
            <span className="text-[#EDEDEF]">{company.name}</span>
            <span className="text-[#4D4D55]">/</span>
            <span className="truncate">{company.project}</span>
            <span className="text-[#4D4D55] max-sm:hidden">/</span>
            <span className="max-sm:hidden">production</span>
            <span className="ml-auto shrink-0 rounded-[5px] border border-[#2D2D34] px-1.5 py-px text-[12px] text-[#A4A4AD]">
              Example data
            </span>
          </div>
          <div className="flex items-center gap-2 h-10 shrink-0 px-4 border-b border-[#212126] bg-[#101012] text-[13.5px]">
            <span className="truncate text-[#EDEDEF]">
              {view === 'dashboards' ? company.board : VIEW_LABELS[view]}
            </span>
            {view === 'dashboards' && (
              <span className="text-[#72727B]">v3</span>
            )}
            <span className="flex items-center gap-3 ml-auto text-[#72727B]">
              {/* The sidebar is hidden on small screens, so its switch moves here. */}
              {(Object.keys(VIEW_LABELS) as View[])
                .filter((v) => v !== view)
                .map((v) => (
                  <button
                    className="focus-ring lg:hidden rounded-md border border-[#2D2D34] px-2 py-0.5 text-[12.5px] text-[#A4A4AD] cursor-pointer"
                    key={v}
                    onClick={() => open(v)}
                    type="button"
                  >
                    {VIEW_LABELS[v]}
                  </button>
                ))}
              {view === 'dashboards' && (
                <span className="flex items-center gap-3 max-sm:hidden">
                  <Icon d={ICONS.link} />
                  <Icon d={ICONS.fullscreen} />
                  <Icon d={ICONS.upload} />
                </span>
              )}
            </span>
          </div>
          {/* Remounting replays each dashboard's entrance. */}
          <div
            className="v2-fade flex-1 min-h-0"
            key={`${company.key}-${view}`}
            onFocus={reveal}
            onMouseOver={reveal}
          >
            {view === 'alerts' && <Alerts alerts={company.alerts} />}
            {view === 'query' && <QueryPage query={company.query} />}
            {view === 'dashboards' && company.render()}
          </div>
          {view === 'dashboards' && <QueryStrip sql={sql} />}
        </div>
      </div>
    </div>
  );
}
