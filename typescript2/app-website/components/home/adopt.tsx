'use client';

import { type ReactNode, useEffect, useState } from 'react';
import { section } from './analytics';

// Section 8: where to use BAML first. It can be adopted a piece at a time, so it starts with the code
// whose feedback loop matters most: AI code and user-facing features. One card shows files in a
// codebase turning into BAML, one shows Python calling a BAML function through the generated client,
// and a wide card under them shows the other way in: sending existing OpenTelemetry traces, whose
// spans move into BAML one after another. The codebase and the trace run on the same clock.

const LANGUAGES = 'Python, TypeScript, Go, Rust, Java, Swift, and C++';

const STEP_MS = 1400;
const HOLD_MS = 2800;
const STEPS = 3;

/** How many pieces have turned into BAML, counting up to STEPS and starting over after a pause. Under
 *  reduced motion it stays at STEPS, including when a visitor turns reduced motion on or off while the
 *  page is open. */
function useStep() {
  const [step, setStep] = useState(0);
  useEffect(() => {
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)');
    let timer: ReturnType<typeof setTimeout> | undefined;
    const next = (n: number) => {
      setStep(n);
      timer = setTimeout(
        () => next(n < STEPS ? n + 1 : 0),
        n < STEPS ? STEP_MS : HOLD_MS,
      );
    };
    const start = () => {
      clearTimeout(timer);
      if (reduce.matches) setStep(STEPS);
      else next(0);
    };
    start();
    reduce.addEventListener('change', start);
    return () => {
      clearTimeout(timer);
      reduce.removeEventListener('change', start);
    };
  }, []);
  return step;
}

const LIT =
  'border-clover/60 bg-[rgb(from_var(--color-clover)_r_g_b/0.22)] text-clover';
const DIM = 'border-rule bg-transparent text-muted';

// A codebase as a 6 by 4 treemap: [name, column, row, columns spanned, rows spanned, the step at which
// it becomes a BAML file]. A file that never turns has no step.
const FILES: [string, number, number, number, number, number?][] = [
  ['server.py', 1, 1, 3, 2],
  ['classify.py', 4, 1, 2, 1, 1],
  ['cfg', 6, 1, 1, 1],
  ['billing.ts', 4, 2, 1, 2],
  ['route.ts', 5, 2, 2, 1, 3],
  ['db.py', 1, 3, 1, 2],
  ['models.py', 2, 3, 2, 1],
  ['extract.py', 5, 3, 2, 2, 2],
  ['test', 2, 4, 1, 1],
  ['index.ts', 3, 4, 2, 1],
];

function Codebase({ step }: { step: number }) {
  return (
    <div
      aria-hidden
      className="grid grid-cols-6 grid-rows-4 gap-1.5 h-full font-mono text-[11px]"
    >
      {FILES.map(([name, col, row, w, h, at]) => {
        const lit = at !== undefined && step >= at;
        // On a narrow tile the name gives way before its extension, so a file turning into .baml
        // still shows on a phone.
        const shown = lit ? name.replace(/\.\w+$/, '.baml') : name;
        const dot = shown.lastIndexOf('.');
        return (
          <span
            className={`flex min-w-0 rounded-md border px-1.5 sm:px-2 py-1.5 overflow-hidden whitespace-nowrap transition-colors duration-500 motion-reduce:transition-none ${lit ? LIT : DIM}`}
            key={name}
            style={{
              gridColumn: `${col} / span ${w}`,
              gridRow: `${row} / span ${h}`,
            }}
          >
            {dot > 0 ? (
              <>
                <span className="truncate">{shown.slice(0, dot)}</span>
                <span className="shrink-0">{shown.slice(dot)}</span>
              </>
            ) : (
              shown
            )}
          </span>
        );
      })}
    </div>
  );
}

// One request's trace: [name, depth, start, width, the step at which the span moves into BAML], with
// start and width in shares of the request. The request itself stays where it is.
const SPANS: [string, number, number, number, number?][] = [
  ['POST /tickets', 0, 0, 1],
  ['load_ticket', 1, 0.02, 0.18],
  ['classify', 1, 0.22, 0.46, 1],
  ['extract', 2, 0.26, 0.3, 2],
  ['route', 1, 0.7, 0.12, 3],
  ['notify', 1, 0.84, 0.15],
];

function Trace({ step }: { step: number }) {
  return (
    <div aria-hidden className="flex flex-col h-full font-mono text-[11px]">
      <div className="flex-1 flex flex-col justify-between">
        {SPANS.map(([name, depth, start, width, at]) => {
          const lit = at !== undefined && step >= at;
          return (
            <div
              className="grid grid-cols-[7.5rem_minmax(0,1fr)] items-center gap-3"
              key={name}
            >
              <span
                className={`truncate transition-colors duration-500 motion-reduce:transition-none ${lit ? 'text-clover' : 'text-ink-2'}`}
                style={{ paddingLeft: `${depth * 0.75}rem` }}
              >
                {name}
              </span>
              <span className="relative h-3.5">
                <span
                  className={`absolute inset-y-0 rounded-[3px] border transition-colors duration-500 motion-reduce:transition-none ${lit ? LIT : 'border-rule bg-[rgb(from_var(--color-ink)_r_g_b/0.1)]'}`}
                  style={{
                    left: `${start * 100}%`,
                    width: `${width * 100}%`,
                  }}
                />
              </span>
            </div>
          );
        })}
      </div>
      <p className="mt-3 flex gap-4 text-muted">
        <span className="flex items-center gap-1.5">
          <span className="size-2 rounded-[2px] bg-[rgb(from_var(--color-ink)_r_g_b/0.25)]" />
          OpenTelemetry
        </span>
        <span className="flex items-center gap-1.5">
          <span className="size-2 rounded-[2px] bg-clover/70" />
          BAML, richer traces
        </span>
      </p>
    </div>
  );
}

// Python calling the classify function from the page's triage.baml, kept in baml_src/triage/, through
// the client `baml generate` writes. Each line is [text, tone] runs.
const PY: [string, string?][][] = [
  [['# app/tickets.py', 'c']],
  [['from', 'k'], [' baml_sdk '], ['import', 'k'], [' triage']],
  [],
  [
    ['def', 'k'],
    [' handle', 'f'],
    ['(ticket: Ticket) -> '],
    ['None', 'k'],
    [':'],
  ],
  [['    # "billing", "bug", or "other"', 'c']],
  [['    label = triage.'], ['classify', 'f'], ['(ticket.body)']],
  [['    if', 'k'], [' label == '], ['"bug"', 's'], [':']],
  [['        escalate(ticket)']],
];
// The import and the call: where the app meets BAML.
const FOCUS = new Set([1, 5]);
const TONE: Record<string, string> = {
  c: 'text-muted',
  f: 'text-clover',
  k: 'text-ink-2',
  s: 'text-head',
};

/** The Python sample: an app calling a BAML function like any other Python function. */
function Python() {
  return (
    <pre className="h-full -mx-2 overflow-x-auto overflow-y-hidden font-mono [font-variant-ligatures:none] text-[12px] md:text-[12.5px] leading-[1.65] text-body">
      {PY.map((runs, i) => (
        <span
          className={`block min-h-[1lh] px-2 rounded-[3px] whitespace-pre ${FOCUS.has(i) ? 'bg-clover/12' : ''}`}
          // biome-ignore lint/suspicious/noArrayIndexKey: the sample's lines never reorder.
          key={i}
        >
          {runs.map(([text, tone], j) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: a line's runs never reorder.
            <span className={tone ? TONE[tone] : undefined} key={j}>
              {text}
            </span>
          ))}
        </span>
      ))}
    </pre>
  );
}

export default function Adopt() {
  const step = useStep();
  return (
    <section
      {...section('adopt')}
      aria-labelledby="adopt"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 max-w-[22em]" id="adopt">
          Use BAML where the feedback loop matters most.
        </h2>
        <p className="mt-5 max-w-[40rem] v4-body text-ink-2">
          <strong className="font-semibold text-head">
            BAML can be adopted incrementally.
          </strong>{' '}
          You can literally call BAML functions from Python.
        </p>
        <div className="mt-10 md:mt-12 grid md:grid-cols-2 gap-3 md:gap-4">
          <Side
            figure={<Codebase step={step} />}
            title="Start migrating an AI agent or workflow."
          >
            Move one agent or workflow into BAML, and call it from the rest of
            your app.
          </Side>
          <Side figure={<Python />} title="Keep the rest of your stack.">
            BAML works alongside {LANGUAGES}.{' '}
            <code className="font-mono text-[0.88em] text-ink">
              baml generate
            </code>{' '}
            writes a typed client, so your app calls BAML like its own code.
          </Side>
        </div>
        <div className="mt-3 md:mt-4 grid md:grid-cols-2 rounded-2xl border border-rule bg-raised overflow-hidden">
          <div className="h-48 md:h-auto md:min-h-52 p-5 md:p-6 border-b md:border-b-0 md:border-r border-rule">
            <Trace step={step} />
          </div>
          <div className="p-6 md:p-8 self-center">
            <h3 className="v4-h3 text-head">
              Or send your existing OTEL traces.
            </h3>
            <p className="mt-4 v4-body text-ink-2">
              Send OpenTelemetry traces to Boundary to get started, and move
              code into BAML for richer traces and better DX.
            </p>
          </div>
        </div>
      </div>
    </section>
  );
}

function Side({
  children,
  figure,
  title,
}: {
  children: ReactNode;
  figure: ReactNode;
  title: string;
}) {
  return (
    <div className="flex flex-col rounded-2xl border border-rule bg-raised overflow-hidden">
      <div className="h-48 md:h-52 p-5 md:p-6 border-b border-rule">
        {figure}
      </div>
      <div className="p-6 md:p-8">
        <h3 className="v4-h3 text-head">{title}</h3>
        <p className="mt-4 v4-body text-ink-2">{children}</p>
      </div>
    </div>
  );
}
