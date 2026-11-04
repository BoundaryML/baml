'use client';

import { Fragment } from 'react';
import { section } from './analytics';
import {
  AlertRow,
  Bars,
  clamp,
  type Deco,
  FIRST,
  Inputs,
  OTHER_QUERY,
  Player,
  RULE,
  type Script,
  source,
  typed,
  Watch,
  watchedCalls,
  within,
} from './hero-video';

// Section 3: an agent works through the loop in one terminal beside triage.baml, saying what it will do
// before each command. It checks, tests, and deploys the code, then watches it in production and fixes what it finds. A rail of steps on the
// left follows the video and seeks it. The four sections after this one take the rail's phases in
// turn. The player, editor, and terminal are hero version 7's.

// The agent says what it's about to do before each command, so the session reads as one task.
const CHECK_AT = 1400;
const CHECK_SHOWN = 1900;
const TEST_AT = 2400;
const TEST_SHOWN = 2900;
const DEPLOY_AT = 3900;
const DEPLOY_SHOWN = 4400;
// Everything after the deploy happens in production. The user's next message comes just before.
const PROD = 5600;
const TRACE_AT = PROD + 500;
const TRACE_SHOWN = PROD + 1300;
const WATCH_AT = PROD + 4600;
const WATCH_SHOWN = PROD + 5300;
const ALERT_AT = PROD + 9000;
const ALERT_SHOWN = PROD + 9600;
const FLAG_AT = PROD + 12400;
const RULE_AT = PROD + 13300;
const SPLIT_AT = PROD + 14300;
const SPLIT_SHOWN = PROD + 14700;
const COMPARED = PROD + 16400;
const FIX_AT = PROD + 18800;
const RESOLVED = PROD + 23400;
const TOTAL = PROD + 26500;

/** The video's steps, grouped by the rail's phases. Each phase seeks the video to its first step. */
const PHASES = [
  {
    name: 'Build',
    steps: [
      { at: 0, name: 'Check' },
      { at: TEST_AT, name: 'Test' },
    ],
  },
  {
    name: 'Deploy',
    steps: [{ at: DEPLOY_AT, name: 'Deploy' }],
  },
  {
    name: 'Observe',
    steps: [
      { at: PROD, name: 'Trace' },
      { at: WATCH_AT, name: 'Dashboards' },
      { at: ALERT_AT, name: 'Alerts' },
    ],
  },
  {
    name: 'Improve',
    steps: [
      { at: FLAG_AT, name: 'Feature flags' },
      { at: FIX_AT, name: 'Fix' },
    ],
  },
];
const STEPS = PHASES.flatMap((p) => p.steps);

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
            className="truncate text-ink-2"
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
          <span className="text-right tabular-nums text-body">
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

  // Before the deploy: the tests pass on classify, and then v43 goes live.
  if (within(t, TEST_SHOWN, DEPLOY_SHOWN)) {
    marks.push({ line: 2, text: 'classify' });
    ends.push({ hint: '9 tests passed', line: 2, tone: 'ok' });
  }
  if (within(t, DEPLOY_SHOWN, TRACE_SHOWN)) {
    ends.push({ hint: 'v43 deployed', line: 2, tone: 'ok' });
  }
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
  if (within(t, SPLIT_SHOWN, FIX_AT)) {
    marks.push({ line: 2, text: 'classify' });
    ends.push({ hint: 'v43 90%  v44 10%', line: 10 + after });
  }
  return result;
}

const FLOW: Script = {
  chapters: STEPS.map((step) => ({ at: step.at, name: step.name })),
  code: (t) =>
    t >= RULE_AT ? source(FIRST, typed(RULE, t, RULE_AT, 900)) : source(FIRST),
  deco,
  height: 480,
  hold: RESOLVED + 1100,
  label:
    'A video of an agent working on triage.baml in a terminal, saying what it will do before each step. Asked to ship triage.baml, it checks the code with baml check and runs its 9 tests with baml test. After the app is deployed, the user asks it to check on production. It reads the trace of a slow request with baml query, and the call path lights up in the code. It watches calls per minute with baml query --watch. An alert fires on classify because 3.1% of tickets are labelled other, above 2%. The agent pulls a few of those tickets, sees they are sarcastic, adds a line to the prompt, ships the new version to 10% of traffic with a feature flag, and compares the two: other falls to 0.3%. It turns the bad calls into tests, ships the new version to everyone, and a day later tells the user the alert resolved.',
  terminal: [
    { at: 0, kind: 'user', text: 'Ship triage.baml.' },
    {
      at: 900,
      kind: 'agent',
      text: "I'll type-check it and run its tests before it goes out.",
    },
    { at: CHECK_AT, kind: 'tool', text: 'baml check' },
    {
      at: CHECK_SHOWN,
      kind: 'out',
      text: '0 errors in triage.baml',
      tone: 'ok',
    },
    { at: TEST_AT, kind: 'tool', text: 'baml test classify' },
    { at: TEST_SHOWN, kind: 'out', text: '9 passed, 0 failed', tone: 'ok' },
    {
      at: DEPLOY_AT - 500,
      kind: 'agent',
      text: 'Everything passes. Deploying it as v43.',
    },
    { at: DEPLOY_AT, kind: 'sep', text: 'after app deployment' },
    { at: PROD - 1100, kind: 'user', text: 'Check on triage in production.' },
    {
      at: PROD - 200,
      kind: 'agent',
      text: "I'll start with one slow request and see where its time goes.",
    },
    {
      at: TRACE_AT,
      kind: 'tool',
      text: `baml query "SELECT span_name, duration_ms FROM spans WHERE trace_id = 'tr_8f2c'"`,
    },
    {
      at: TRACE_SHOWN,
      kind: 'node',
      render: (since) => <Waterfall since={since} />,
    },
    {
      at: WATCH_AT - 700,
      kind: 'agent',
      text: 'Almost all of it is the model call. Now watching traffic.',
    },
    {
      at: WATCH_AT,
      kind: 'tool',
      text: `baml query --watch "SELECT count(*) FROM spans GROUP BY minute"`,
    },
    {
      at: WATCH_SHOWN,
      kind: 'node',
      render: (since) => <Watch since={since} />,
    },
    {
      at: ALERT_AT - 500,
      kind: 'agent',
      text: 'Traffic is steady. Checking alerts.',
    },
    { at: ALERT_AT, kind: 'tool', text: 'baml alerts' },
    { at: ALERT_SHOWN, kind: 'node', render: () => <AlertRow /> },
    {
      at: ALERT_SHOWN + 500,
      kind: 'agent',
      text: 'classify labels 3.1% of tickets other. Pulling a few of them.',
    },
    { at: ALERT_SHOWN + 900, kind: 'tool', text: OTHER_QUERY },
    {
      at: ALERT_SHOWN + 1500,
      kind: 'node',
      render: (since) => <Inputs since={since} />,
    },
    {
      at: FLAG_AT,
      kind: 'agent',
      text: "They're sarcastic. I'll add a line to the prompt and send it to 10% of traffic.",
    },
    { at: RULE_AT, kind: 'out', text: `+ ${RULE}`, tone: 'ok' },
    {
      at: SPLIT_AT,
      kind: 'tool',
      text: 'baml flags set classify --version v44 --traffic 10%',
    },
    {
      at: SPLIT_SHOWN,
      kind: 'out',
      text: 'classify: v43 90%, v44 10%',
      tone: 'ok',
    },
    {
      at: COMPARED - 1000,
      kind: 'agent',
      text: 'Comparing the two versions.',
    },
    {
      at: COMPARED - 600,
      kind: 'tool',
      text: `baml query "SELECT version, avg(output_value = 'other') FROM spans GROUP BY version"`,
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
      at: FIX_AT - 600,
      kind: 'agent',
      text: "v44 fixes it. I'll keep the bad calls as tests and ship v44 to everyone.",
    },
    {
      at: FIX_AT,
      kind: 'tool',
      text: 'baml test add classify --from-query "output_value = \'other\'"',
    },
    {
      at: FIX_AT + 500,
      kind: 'out',
      text: '31 tests added',
      tone: 'ok',
    },
    {
      at: FIX_AT + 1300,
      kind: 'tool',
      text: 'baml flags set classify --version v44 --traffic 100%',
    },
    {
      at: FIX_AT + 1700,
      kind: 'out',
      text: 'classify: v44 100%',
      tone: 'ok',
    },
    { at: RESOLVED - 1700, kind: 'sep', text: 'a day later' },
    {
      at: RESOLVED - 1400,
      kind: 'user',
      text: 'Is classify still alerting?',
    },
    { at: RESOLVED - 400, kind: 'tool', text: 'baml alerts' },
    { at: RESOLVED, kind: 'node', render: () => <AlertRow resolved /> },
    {
      at: RESOLVED + 800,
      kind: 'agent',
      text: 'No, it resolved. Other is down to 0.3%.',
    },
  ],
  total: TOTAL,
};

/** The four phases beside the video: down a line on wide screens, a row on narrow ones. The current
 *  phase is lit, and each one seeks the video to where it starts. */
function Rail({ seek, t }: { seek: (t: number) => void; t: number }) {
  const current = STEPS.reduce((last, step) => (t >= step.at ? step : last));
  return (
    <>
      <ol className="hidden lg:flex flex-col w-[110px] shrink-0 pt-2">
        {PHASES.map((phase, i) => {
          const live = phase.steps.includes(current);
          const reached = t >= phase.steps[0].at;
          const next = PHASES[i + 1];
          return (
            <li className="relative pb-8 last:pb-0" key={phase.name}>
              {next && (
                <span
                  aria-hidden
                  className={`absolute left-[5px] top-4 bottom-0 w-px transition-colors duration-500 ${t >= next.steps[0].at ? 'bg-clover/70' : 'bg-rule'}`}
                />
              )}
              <button
                className={`v4-caption focus-ring relative flex items-center gap-3 transition-colors ${live ? 'text-head' : 'text-muted hover:text-ink-2'}`}
                onClick={() => seek(phase.steps[0].at)}
                type="button"
              >
                <span
                  aria-hidden
                  className={`size-[11px] shrink-0 rounded-full border transition-colors duration-500 ${live ? 'border-clover bg-clover' : reached ? 'border-clover/70 bg-ground' : 'border-rule bg-ground'}`}
                />
                {phase.name}
              </button>
            </li>
          );
        })}
      </ol>

      <div className="grid grid-cols-4 gap-2 lg:hidden">
        {PHASES.map((phase, i) => {
          const start = phase.steps[0].at;
          const end = PHASES[i + 1]?.steps[0].at ?? TOTAL;
          const live = phase.steps.includes(current);
          return (
            <button
              className="focus-ring flex flex-col gap-2 pt-1 text-left"
              key={phase.name}
              onClick={() => seek(start)}
              type="button"
            >
              <span className="h-[3px] rounded-full bg-rule overflow-hidden">
                <span
                  className="block h-full bg-clover"
                  style={{
                    width: `${clamp((t - start) / (end - start)) * 100}%`,
                  }}
                />
              </span>
              <span
                className={`text-[13px] ${live ? 'text-head' : 'text-muted'}`}
              >
                {phase.name}
              </span>
            </button>
          );
        })}
      </div>
    </>
  );
}

export default function Workflow() {
  return (
    <section
      {...section('how')}
      aria-labelledby="how"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) pt-20 md:pt-28 pb-4">
        <h2 className="v4-h2 max-w-[20em] text-balance" id="how">
          Build, deploy, observe, and improve with one CLI.
        </h2>
      </div>
      {/* The video runs wider than the rest of the page so the editor and terminal are readable. */}
      <div className="mx-auto max-w-[1560px] px-(--frame-pad) mt-10 md:mt-14">
        <Player rail={(t, seek) => <Rail seek={seek} t={t} />} script={FLOW} />
      </div>
    </section>
  );
}
