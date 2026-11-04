'use client';

import { Card, Check, Chip, Frame, Pending, Spinner, useCycle } from './kit';

// A change going through the loop, drawn like a deployment page: four steps with their commands and
// times, and the log of whichever step is running. The run is example data.
const STEPS = [
  {
    command: 'baml check',
    log: ['Checked 4 files', 'No errors'],
    name: 'Build',
    time: '0.8s',
  },
  {
    command: 'baml test classify',
    log: ['9 tests', '9 passed, 0 failed'],
    name: 'Test',
    time: '6.2s',
  },
  {
    command: 'baml pack triage',
    log: ['Packed triage v43', 'Released to production'],
    name: 'Deploy',
    time: '3.1s',
  },
  {
    command: 'baml query',
    log: ['1,204 calls since v43', '0 sarcastic tickets labelled other'],
    name: 'Monitor',
    time: 'Live',
  },
];
const PER = 3;
const TICKS = STEPS.length * PER + 4;

export default function Pipeline() {
  const tick = useCycle(TICKS, 650);
  const current = Math.min(Math.floor(tick / PER), STEPS.length);
  const done = current >= STEPS.length;
  const shown = STEPS[Math.min(current, STEPS.length - 1)];

  return (
    <Frame label="A change to triage going through Build with baml check, Test with baml test, Deploy with baml pack, and Monitor with baml query. Each step shows its time and a short log.">
      <div className="p-4 @min-[640px]:p-8 @min-[768px]:p-12">
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2 mb-6">
          <span className="font-display font-medium text-[17px] @min-[768px]:text-[20px] tracking-[-0.02em]">
            triage
          </span>
          <Chip>
            <span className="font-mono">main</span>
          </Chip>
          <span className="text-[13px] text-(--ds-ink)/45 truncate">
            <span className="font-mono text-(--ds-ink)/60">a41f9c2</span> Read
            sarcasm as what the customer means
          </span>
          <span
            className={`ml-auto inline-flex items-center gap-2 h-7 px-3 rounded-full border text-[12.5px] transition-colors duration-300 ${done ? 'border-(--ds-ok)/30 text-(--ds-ok)' : 'border-(--ds-ink)/10 text-(--ds-ink)/60'}`}
          >
            <span
              className={`size-1.5 rounded-full ${done ? 'bg-(--ds-ok)' : 'bg-(--ds-accent) ds-blink'}`}
            />
            {done ? 'Live' : 'Running'}
          </span>
        </div>

        <ol className="grid grid-cols-2 @min-[768px]:grid-cols-4 gap-2 @min-[768px]:gap-0">
          {STEPS.map((s, i) => {
            const state =
              i < current ? 'done' : i === current ? 'running' : 'pending';
            return (
              <li className="relative flex items-center" key={s.name}>
                <Card
                  className={`flex-1 min-w-0 p-4 ${state === 'pending' ? 'opacity-50' : ''}`}
                  lit={state === 'running'}
                >
                  <p className="flex items-center gap-2 text-[14px] text-(--ds-ink)/90">
                    {state === 'done' ? (
                      <Check />
                    ) : state === 'running' ? (
                      <Spinner />
                    ) : (
                      <Pending />
                    )}
                    {s.name}
                    <span className="ml-auto font-mono text-[12px] text-(--ds-ink)/40">
                      {state === 'done' ? s.time : ''}
                    </span>
                  </p>
                  <p className="mt-2 font-mono text-[12px] text-(--ds-ink)/45 truncate">
                    {s.command}
                  </p>
                </Card>
                {i < STEPS.length - 1 && (
                  <span className="hidden @min-[768px]:block w-6 h-px shrink-0 bg-(--ds-ink)/10 overflow-hidden">
                    <span
                      className={`block h-full bg-(--ds-accent) transition-[width] duration-500 ${i < current ? 'w-full' : 'w-0'}`}
                    />
                  </span>
                )}
              </li>
            );
          })}
        </ol>

        <Card className="mt-4 p-4 font-mono text-[12px] leading-6">
          <p className="text-(--ds-ink)/35">
            <span className="text-(--ds-accent-ink)">$</span> {shown.command}
          </p>
          <div className="min-h-12" key={current}>
            {shown.log.map((l, i) => (
              <p
                className="ds-in text-(--ds-ink)/70"
                key={l}
                style={{ animationDelay: `${i * 180}ms` }}
              >
                {l}
              </p>
            ))}
          </div>
        </Card>
      </div>
    </Frame>
  );
}
