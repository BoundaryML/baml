'use client';

import { Card, Check, Frame, Spinner, useCycle } from './kit';

// A coding agent fixing a production bug through BAML's commands, drawn like an agent product's chat.
// Each tool call appears, runs, and settles, then the agent reports back. The session is example data.
const CALLS = [
  {
    detail: "SELECT input FROM spans WHERE output_value['label'] = 'other'",
    result: '14 rows',
    title: 'Ran baml query',
  },
  {
    detail: '+ Read sarcasm as what the customer means.',
    result: '+1 line',
    title: 'Edited triage.baml',
  },
  {
    detail: 'sarcastic_ticket, and 8 more',
    result: '9 passed',
    title: 'Ran baml test classify',
  },
];
const PER = 3;
const TICKS = 2 + CALLS.length * PER + 6;

export default function Agent() {
  const tick = useCycle(TICKS, 600);
  const t = tick - 2;
  const finished = t >= CALLS.length * PER;

  return (
    <Frame label="A chat with a coding agent. Asked why sarcastic tickets go to the wrong queue, it runs baml query and finds 14 of them, adds one line to the prompt in triage.baml, runs baml test classify with 9 passing, and reports the fix.">
      <div className="flex justify-center p-3 @min-[640px]:p-8 @min-[768px]:p-12">
        <Card className="w-full max-w-[640px] p-4 @min-[640px]:p-6">
          <div className="flex justify-end">
            <p className="max-w-[85%] px-4 py-2.5 rounded-2xl rounded-br-md bg-(--ds-ink)/[0.07] text-[14px] text-(--ds-ink)/90">
              Sarcastic tickets are going to the wrong queue. Can you fix it?
            </p>
          </div>

          <div className="mt-6 flex flex-col gap-2 min-h-[228px]">
            {CALLS.map((c, i) => {
              const start = i * PER;
              if (t < start) return null;
              const running = t < start + PER - 1;
              return (
                <div
                  className="ds-in rounded-lg border border-(--ds-ink)/[0.08] bg-(--ds-ink)/[0.02]"
                  key={c.title}
                >
                  <p className="flex items-center gap-2.5 h-10 px-3 text-[13px]">
                    {running ? <Spinner /> : <Check />}
                    <span className="text-(--ds-ink)/85">{c.title}</span>
                    <span
                      className={`ml-auto font-mono text-[12px] transition-opacity ${running ? 'opacity-0' : 'text-(--ds-ink)/45'}`}
                    >
                      {c.result}
                    </span>
                  </p>
                  <p className="px-3 pb-2.5 -mt-1 pl-9 font-mono text-[11.5px] text-(--ds-ink)/40 truncate">
                    {c.detail}
                  </p>
                </div>
              );
            })}
            {finished && (
              <p className="ds-in mt-3 text-[14px] leading-[1.55] text-(--ds-ink)/80">
                Fixed. 14 sarcastic tickets were labelled other in production. I
                added one line to the prompt and a test for them, and all 9
                tests pass.
              </p>
            )}
          </div>

          <div className="flex items-center gap-3 mt-5 h-11 px-4 rounded-xl border border-(--ds-line) text-[13.5px] text-(--ds-ink)/30">
            Ask your agent
            <span className="ml-auto grid place-items-center size-7 rounded-lg bg-(--ds-ink) text-(--ds-ground)">
              <svg
                aria-hidden="true"
                className="size-3.5"
                fill="none"
                viewBox="0 0 16 16"
              >
                <path
                  d="M8 13V3m0 0L3.5 7.5M8 3l4.5 4.5"
                  stroke="currentColor"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth="1.8"
                />
              </svg>
            </span>
          </div>
        </Card>
      </div>
    </Frame>
  );
}
