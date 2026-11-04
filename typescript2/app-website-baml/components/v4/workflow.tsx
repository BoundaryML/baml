'use client';

import { Fragment } from 'react';
import { section } from '../analytics';
import {
  AlertRow,
  Bars,
  type Deco,
  FIRST,
  Player,
  RULE,
  type Script,
  source,
  typed,
  Watch,
  watchedCalls,
  within,
} from '../v3/hero-video';
import { MarkTile } from './mark';
import { PRODUCTS } from './tracker';

// Section 3: an agent works through the loop in one terminal beside triage.baml. Each chapter matches
// a column of the hero, and when its query answers, the products it replaces are crossed out in the
// grid under the video. The player, editor, and terminal are hero version 7's from /v3.

const TRACE_AT = 600;
const TRACE_SHOWN = 1500;
const WATCH_AT = 6500;
const WATCH_SHOWN = 7400;
const ALERT_AT = 12500;
const ALERT_SHOWN = 13200;
const FLAG_AT = 17500;
const RULE_AT = 18600;
const COMPARED = 22200;
const FIX_AT = 25500;
const RESOLVED = 30400;
const TOTAL = 35000;

// When each column of products is crossed out: the moment its query answers.
const CROSSED_AT = [TRACE_SHOWN, WATCH_SHOWN, ALERT_SHOWN, COMPARED];
const REPLACED_BY = [
  'baml query',
  'baml query --watch',
  'baml alerts',
  'baml flags',
];
const COLUMNS = ['Trace', 'Dashboards', 'Alerts', 'Feature flags'];

const TRACE = [
  { depth: 0, ms: 1342, name: 'route' },
  { depth: 1, ms: 1318, name: 'classify' },
  { depth: 2, ms: 1296, name: 'openai/gpt-6-luna' },
];

/** One request's calls as a waterfall, a row at a time. */
function Waterfall({ since }: { since: number }) {
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.3fr)_auto] gap-x-4 items-center">
      {TRACE.slice(0, Math.floor(since / 220) + 1).map((r) => (
        <Fragment key={r.name}>
          <span
            className="truncate text-white/80"
            style={{ paddingLeft: r.depth * 14 }}
          >
            {r.name}
          </span>
          <span className="h-2 relative">
            <span
              className="v3-enter absolute inset-y-0 rounded-[2px] bg-clover/60"
              style={{
                left: `${(1342 - r.ms) / 2}%`,
                width: `${(r.ms / 1342) * 100 - (1342 - r.ms) / 2}%`,
              }}
            />
          </span>
          <span className="text-right tabular-nums text-white/60">
            {r.ms.toLocaleString('en-US')}ms
          </span>
        </Fragment>
      ))}
    </div>
  );
}

function deco(t: number): Deco {
  const ends: NonNullable<Deco['ends']> = [];
  const marks: NonNullable<Deco['marks']> = [];
  const result: Deco = { ends, lines: {}, marks };
  // Lines after the prompt move down one once the new line is in.
  const after = t >= RULE_AT ? 1 : 0;

  // The trace lights the call path: route, the call to classify, and the model it used.
  if (within(t, TRACE_SHOWN, WATCH_AT)) {
    marks.push({ line: 10, text: 'route' });
    ends.push({ hint: '1,342ms', line: 10 });
    if (t >= TRACE_SHOWN + 220) {
      marks.push({ line: 11, text: 'classify(ticket)' });
      ends.push({ hint: '1,318ms', line: 2 });
    }
    if (t >= TRACE_SHOWN + 440) {
      marks.push({ line: 3, text: '"openai/gpt-6-luna"' });
      ends.push({ hint: '1,296ms', line: 3 });
    }
  }
  // The watch puts each function's calls at the end of its first line, counting up.
  if (within(t, WATCH_SHOWN, ALERT_AT)) {
    const calls = watchedCalls(t - WATCH_SHOWN).toLocaleString('en-US');
    marks.push({ line: 2, text: 'classify' }, { line: 10, text: 'route' });
    ends.push(
      { hint: `${calls} calls`, line: 2 },
      { hint: `${calls} calls`, line: 10 },
    );
  }
  // The alert, from when it fires until it resolves.
  if (within(t, ALERT_SHOWN, RESOLVED)) {
    result.alarm = { firing: true, line: 2 };
    ends.push({ hint: 'other 3.1%, above 2%', line: 2, tone: 'bad' });
    if (t < FLAG_AT) marks.push({ line: 0, text: '"other"', tone: 'bad' });
  }
  if (t >= RESOLVED) {
    result.alarm = { firing: false, line: 2 };
    ends.push({ hint: 'other 0.3%, resolved', line: 2, tone: 'ok' });
  }
  // The new prompt line stays marked as a diff from when it's written.
  if (t >= RULE_AT) result.added = [6];
  if (within(t, RULE_AT, RULE_AT + 1000)) result.caret = 6;
  // The flag splits traffic between the two versions of classify.
  if (within(t, FLAG_AT + 2900, FIX_AT)) {
    marks.push({ line: 2, text: 'classify' });
    ends.push({ hint: 'v43 90%  v44 10%', line: 10 + after });
  }
  return result;
}

const FLOW: Script = {
  chapters: [
    { at: 0, name: 'Trace' },
    { at: WATCH_AT, name: 'Dashboards' },
    { at: ALERT_AT, name: 'Alerts' },
    { at: FLAG_AT, name: 'Feature flags' },
    { at: FIX_AT, name: 'Fix' },
  ],
  code: (t) =>
    t >= RULE_AT ? source(FIRST, typed(RULE, t, RULE_AT, 900)) : source(FIRST),
  deco,
  height: 480,
  hold: 31500,
  label:
    'A video of an agent working on triage.baml in a terminal. It reads the trace of a slow request with baml query, and the call path lights up in the code. It watches calls per minute with baml query --watch. An alert fires on classify because 3.1% of tickets are labelled other, above 2%. The agent adds a line to the prompt, ships the new version to 10% of traffic with a feature flag, and compares the two: other falls to 0.3%. It turns the bad calls into tests, ships the new version to everyone, and the alert resolves. Under the video, the products each step replaces are crossed out.',
  terminal: [
    { at: 0, kind: 'user', text: 'Check on triage in production.' },
    {
      at: TRACE_AT,
      kind: 'tool',
      text: `baml query "SELECT span_name, duration_ms FROM spans WHERE trace_id = 'tr_8f2c' ORDER BY start_time"`,
    },
    {
      at: TRACE_SHOWN,
      kind: 'node',
      render: (since) => <Waterfall since={since} />,
    },
    {
      at: WATCH_AT,
      kind: 'tool',
      text: `baml query --watch "SELECT span_name, date_trunc('minute', start_time) AS minute, count(*) AS calls FROM spans GROUP BY span_name, minute"`,
    },
    {
      at: WATCH_SHOWN,
      kind: 'node',
      render: (since) => <Watch since={since} />,
    },
    { at: ALERT_AT, kind: 'tool', text: 'baml alerts' },
    { at: ALERT_SHOWN, kind: 'node', render: () => <AlertRow /> },
    {
      at: FLAG_AT,
      kind: 'agent',
      text: 'The calls labelled other are sarcastic tickets. I will add a line to the prompt and ship it to 10% of traffic.',
    },
    { at: RULE_AT - 300, kind: 'tool', text: 'edit triage.baml' },
    { at: RULE_AT, kind: 'out', text: `+ ${RULE}`, tone: 'ok' },
    {
      at: FLAG_AT + 2400,
      kind: 'tool',
      text: 'baml flags set classify --version v44 --traffic 10%',
    },
    {
      at: FLAG_AT + 2900,
      kind: 'out',
      text: 'classify: v43 90%, v44 10%',
      tone: 'ok',
    },
    {
      at: COMPARED - 700,
      kind: 'tool',
      text: `baml query "SELECT version, avg(output_value = 'other') AS other FROM spans WHERE span_name = 'classify' GROUP BY version"`,
    },
    {
      at: COMPARED,
      kind: 'node',
      render: (since) => (
        <Bars
          format={(v) => `${v}%`}
          rows={[
            { bad: true, label: 'v43', value: 3.1 },
            { good: true, label: 'v44', value: 0.3 },
          ]}
          since={since}
        />
      ),
    },
    {
      at: FIX_AT,
      kind: 'tool',
      text: 'baml test add classify --from-query "output_value = \'other\'"',
    },
    {
      at: FIX_AT + 600,
      kind: 'out',
      text: '31 tests added from production calls',
      tone: 'ok',
    },
    { at: FIX_AT + 1300, kind: 'tool', text: 'baml test classify' },
    {
      at: FIX_AT + 1900,
      kind: 'out',
      text: '40 passed, 0 failed',
      tone: 'ok',
    },
    {
      at: FIX_AT + 2700,
      kind: 'tool',
      text: 'baml flags set classify --version v44 --traffic 100%',
    },
    {
      at: FIX_AT + 3200,
      kind: 'out',
      text: 'classify: v44 100%',
      tone: 'ok',
    },
    { at: RESOLVED - 900, kind: 'sep', text: 'a day later' },
    { at: RESOLVED - 400, kind: 'tool', text: 'baml alerts' },
    { at: RESOLVED, kind: 'node', render: () => <AlertRow resolved /> },
    {
      at: RESOLVED + 1000,
      kind: 'agent',
      text: 'The alert on classify resolved. Calls labelled other fell from 3.1% to 0.3%.',
    },
  ],
  total: TOTAL,
};

/** The hero's products past the deploy line, crossed out as the agent's queries replace them. */
function Replaced({ t }: { t: number }) {
  return (
    <div className="grid grid-cols-2 md:grid-cols-4 gap-2 md:gap-3 mt-4 md:mt-5">
      {PRODUCTS.map((marks, i) => {
        const crossed = t >= CROSSED_AT[i];
        return (
          <div
            className={`rounded-xl border px-4 py-3.5 transition-colors duration-500 ${crossed ? 'border-clover/40 bg-clover/[0.06]' : 'border-white/10 bg-white/[0.02]'}`}
            key={COLUMNS[i]}
          >
            <p className="text-[13px] text-white/50">{COLUMNS[i]}</p>
            <div className="mt-2.5 flex flex-wrap items-center gap-3 text-white/85">
              {marks.map((m) => (
                <MarkTile crossed={crossed} key={m.name} mark={m} size={20} />
              ))}
            </div>
            <p
              className={`mt-3 font-mono text-[12.5px] transition-colors duration-500 ${crossed ? 'text-clover' : 'text-white/25'}`}
            >
              {REPLACED_BY[i]}
            </p>
          </div>
        );
      })}
    </div>
  );
}

export default function Workflow() {
  return (
    <section
      {...section('how')}
      aria-labelledby="how"
      className="border-t border-white/[0.08]"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) pt-20 md:pt-28 pb-4">
        <h2 className="v4-h2 max-w-[20em] text-balance" id="how">
          Write it in BAML, deploy it, query your data, and build anything.
        </h2>
        <p className="mt-6 max-w-[38rem] text-lg leading-[1.55] text-white/70">
          You write your code in BAML and deploy it. There's no tracing SDK to
          add and nothing to configure. Your agent uses the same CLI you do to
          see what happened and fix it.
        </p>
        <div className="mt-10 md:mt-14">
          <Player below={(t) => <Replaced t={t} />} script={FLOW} />
        </div>
      </div>
    </section>
  );
}
