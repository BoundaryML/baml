'use client';

import { useEffect, useRef, useState } from 'react';

// An agent working through a real bug in a chat, picking up one toolchain command at a time. Each command
// it uses joins the stack on the right with what it lets the agent do, and using them all closes the loop.
// The session is example data. `baml experiment start` isn't shipped, so it stays in its own locked slot.
type Tool = { command: string; power: string; detail: string };

const TOOLS: Tool[] = [
  {
    command: 'baml describe',
    detail: 'Finds functions, types, and tests by name.',
    power: 'Reads your code',
  },
  {
    command: 'baml query',
    detail: 'Runs SQL over every recorded call.',
    power: 'Sees production',
  },
  {
    command: 'baml check',
    detail: 'Finds type errors before anything runs.',
    power: 'Catches its own mistakes',
  },
  {
    command: 'baml test',
    detail: 'Runs the tests stored next to the function.',
    power: 'Proves the fix',
  },
  {
    command: 'baml run',
    detail: 'Runs any function from the terminal.',
    power: 'Tries the real thing',
  },
];

type Entry =
  | { kind: 'agent'; text: string }
  | { kind: 'user'; text: string }
  | { command: string; kind: 'tool'; result: string; tool: number };

const SESSION: Entry[] = [
  {
    kind: 'user',
    text: 'Sarcastic tickets are going to the wrong queue. Can you fix it?',
  },
  { kind: 'agent', text: "I'll find the classifier first." },
  {
    command: 'baml describe classify',
    kind: 'tool',
    result: 'classify.baml:8, 3 tests',
    tool: 0,
  },
  { kind: 'agent', text: "Next I'll look at what it did in production." },
  {
    command:
      'baml query "SELECT input_args, output_value FROM spans WHERE span_name = \'classify\'"',
    kind: 'tool',
    result: '14 sarcastic tickets labelled "other"',
    tool: 1,
  },
  {
    kind: 'agent',
    text: "The prompt doesn't mention sarcasm. I'll update it and add one of those tickets as a test.",
  },
  {
    command: 'baml check',
    kind: 'tool',
    result: 'Finished, no errors',
    tool: 2,
  },
  {
    command: 'baml test classify',
    kind: 'tool',
    result: '4 tests, 4 passed',
    tool: 3,
  },
  {
    command: 'baml run triage',
    kind: 'tool',
    result: 'The ticket from production now goes to the bug queue',
    tool: 4,
  },
  { kind: 'agent', text: 'Fixed. The new test covers the production case.' },
];

const STEP_MS = 1500;
const HOLD_MS = 6000;

export default function HeroAgent() {
  // Everything shows until the effect runs, and stays shown for anyone who prefers no motion.
  const [shown, setShown] = useState(SESSION.length);
  const log = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    setShown(1);
    let position = 1;
    let id: ReturnType<typeof setTimeout>;
    const next = () => {
      const done = position >= SESSION.length;
      id = setTimeout(
        () => {
          position = done ? 1 : position + 1;
          setShown(position);
          next();
        },
        done ? HOLD_MS : STEP_MS,
      );
    };
    next();
    return () => clearTimeout(id);
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: a new entry scrolls the log
  useEffect(() => {
    const el = log.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [shown]);

  const visible = SESSION.slice(0, shown);
  const unlocked = new Set(
    visible.flatMap((e) => (e.kind === 'tool' ? [e.tool] : [])),
  );
  const closed = unlocked.size === TOOLS.length;

  return (
    <figure
      aria-label="A chat with a coding agent that fixes a production bug using baml describe, baml query, baml check, baml test, and baml run. Each command adds to a stack of things the agent can do, and all of them together close the development loop."
      className="relative"
    >
      <div
        aria-hidden
        className={`v3-glow left-[-10%] right-[-10%] -top-32 h-[800px] transition-opacity duration-700 ${closed ? 'opacity-100' : 'opacity-45'}`}
      />
      <div className="relative pb-24">
        <div className="relative grid lg:grid-cols-[minmax(0,1fr)_420px] gap-4">
          <div className="v3-window flex flex-col h-[460px] md:h-[660px] overflow-hidden">
            <div className="flex items-center gap-2 h-11 shrink-0 px-4 border-b border-white/8 text-[13px] text-white/50">
              <span className="size-1.5 bg-clover" />
              Agent session
            </div>
            <div
              className="flex-1 min-h-0 overflow-hidden p-5 md:p-7"
              ref={log}
            >
              <div className="flex flex-col gap-4">
                {visible.map((entry, i) => {
                  if (entry.kind === 'user')
                    return (
                      <p
                        className="v3-enter self-end max-w-[85%] px-4 py-2.5 rounded-xl bg-white/8 text-[15px] md:text-base leading-6"
                        // biome-ignore lint/suspicious/noArrayIndexKey: the script only appends
                        key={i}
                      >
                        {entry.text}
                      </p>
                    );
                  if (entry.kind === 'agent')
                    return (
                      <p
                        className="v3-enter text-[15px] md:text-base leading-6 text-white/80"
                        // biome-ignore lint/suspicious/noArrayIndexKey: the script only appends
                        key={i}
                      >
                        {entry.text}
                      </p>
                    );
                  return (
                    <div
                      className="v3-enter rounded-lg bg-white/4 border border-white/8 px-3.5 py-2.5 font-mono text-[12.5px] md:text-[13px] leading-5"
                      // biome-ignore lint/suspicious/noArrayIndexKey: the script only appends
                      key={i}
                    >
                      <p className="truncate text-white">
                        <span className="text-clover">$ </span>
                        {entry.command}
                      </p>
                      <p className="mt-1 text-white/50">{entry.result}</p>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>

          <div className="flex flex-col gap-2">
            {TOOLS.map((tool, i) => {
              const on = unlocked.has(i);
              return (
                <div
                  className={`rounded-xl border px-4 py-3 transition-colors duration-300 ${on ? 'v3-land v3-tool-on' : 'border-white/8'}`}
                  key={`${tool.command}-${on}`}
                >
                  <div className="flex items-baseline justify-between gap-3">
                    <p
                      className={`text-[15px] md:text-base ${on ? 'text-white' : 'text-white/25'}`}
                    >
                      {on ? tool.power : 'Locked'}
                    </p>
                    <code
                      className={`font-mono text-[12.5px] ${on ? 'text-clover' : 'text-white/25'}`}
                    >
                      {tool.command}
                    </code>
                  </div>
                  <p
                    className={`mt-0.5 text-[13px] leading-5 ${on ? 'text-white/60' : 'invisible'}`}
                  >
                    {tool.detail}
                  </p>
                </div>
              );
            })}
            <div className="flex items-baseline justify-between gap-3 rounded-xl border border-dashed border-white/12 px-4 py-3">
              <p className="text-[15px] text-white/35">Coming soon</p>
              <code className="font-mono text-[12.5px] text-white/35">
                baml experiment start
              </code>
            </div>
            <div
              className={`flex-1 min-h-24 flex flex-col justify-center rounded-xl px-4 py-3 transition-opacity duration-500 ${closed ? 'opacity-100' : 'opacity-0'}`}
            >
              <p className="v2-baml-mark text-[26px] md:text-[30px] leading-tight tracking-[-0.02em] text-clover">
                Loop closed
              </p>
              <p className="mt-1 text-[15px] leading-[22px] text-white/70">
                Your agent found, fixed, and verified a production bug by
                itself.
              </p>
            </div>
          </div>
        </div>
      </div>
    </figure>
  );
}
