'use client';

import { type ReactNode, useEffect, useState } from 'react';
import { track } from '../track';
import { Code, type LineMark } from './code';

// A real editor with a real function, and an agent terminal laid over it. Each step of the loop marks
// the lines it touches, and the fix changes the source. The source compiles with baml check; the terminal
// output is example data.
const BEFORE = '  prompt: `Classify this support ticket: ${text}`';
const AFTER =
  '  prompt: `Classify this support ticket: ${text}. Read sarcasm as what the customer means.`';

const source = (prompt: string) => [
  'type Label = "billing" | "bug" | "other";',
  '',
  'class Verdict {',
  '  label: Label,',
  '  confidence: float,',
  '}',
  '',
  '// An AI function',
  'function classify(text: string) -> Verdict {',
  '  client: "openai/gpt-6-luna"',
  prompt,
  '}',
  '',
  '// Ordinary code that calls it',
  'function triage(tickets: string[]) -> string[] {',
  '  let bugs: string[] = [];',
  '  for (let ticket in tickets) {',
  '    let verdict = classify(ticket);',
  '    if (verdict.label == "bug") {',
  '      bugs.push(ticket);',
  '    }',
  '  }',
  '  return bugs;',
  '}',
];

const range = (from: number, to: number, mark: LineMark) =>
  Object.fromEntries(
    Array.from({ length: to - from + 1 }, (_, i) => [from + i, mark]),
  ) as Record<number, LineMark>;

const Ok = ({ children }: { children: ReactNode }) => (
  <span className="text-clover">{children}</span>
);

const STEPS: {
  command: string;
  label: string;
  marks: Record<number, LineMark>;
  output: ReactNode;
  prompt: string;
}[] = [
  {
    command: 'baml check',
    label: 'Build',
    marks: range(8, 11, 'running'),
    output: (
      <p>
        <Ok>Finished</Ok> checked 1 file
      </p>
    ),
    prompt: BEFORE,
  },
  {
    command: 'baml test classify',
    label: 'Test',
    marks: range(14, 23, 'running'),
    output: (
      <p>
        3 tests, <Ok>3 passed</Ok>
      </p>
    ),
    prompt: BEFORE,
  },
  {
    command:
      'baml query "SELECT output_value[\'label\'] AS label, count(*) ..."',
    label: 'Observe',
    marks: range(3, 4, 'running'),
    output: (
      <table className="w-full max-w-64">
        <thead className="text-white/40">
          <tr>
            <th className="font-normal text-left" scope="col">
              label
            </th>
            <th className="font-normal text-right" scope="col">
              calls
            </th>
          </tr>
        </thead>
        <tbody>
          {[
            ['bug', '412'],
            ['billing', '260'],
            ['other', '97'],
          ].map(([label, calls]) => (
            <tr key={label}>
              <td>{label}</td>
              <td className="text-right">{calls}</td>
            </tr>
          ))}
        </tbody>
      </table>
    ),
    prompt: BEFORE,
  },
  {
    command: 'baml test classify',
    label: 'Fix',
    marks: { 10: 'added' },
    output: (
      <p>
        4 tests, <Ok>4 passed</Ok>
      </p>
    ),
    prompt: AFTER,
  },
];

const FILES = ['triage.baml', 'tests.baml', 'baml.toml'];
const CYCLE_MS = 4500;

export default function HeroCode() {
  const [active, setActive] = useState(0);
  const [held, setHeld] = useState(false);

  useEffect(() => {
    if (held) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
    const id = setInterval(
      () => setActive((a) => (a + 1) % STEPS.length),
      CYCLE_MS,
    );
    return () => clearInterval(id);
  }, [held]);

  const step = STEPS[active];

  return (
    <figure
      aria-label="A BAML source file with an AI function and ordinary code, and an agent terminal running baml check, baml test, and baml query against it."
      className="relative"
    >
      <div
        aria-hidden
        className="v3-glow left-[-10%] right-[-10%] -top-32 h-[760px]"
      />
      <div className="v3-fade relative h-[560px] md:h-[720px]">
        <div className="v3-window absolute inset-x-0 top-0 h-full overflow-hidden lg:right-[14%]">
          <div className="flex items-center h-11 border-b border-white/8 text-[13px]">
            {FILES.map((f, i) => (
              <span
                className={`flex items-center h-full px-4 border-r border-white/8 ${i === 0 ? 'text-white bg-white/4' : 'text-white/40'}`}
                key={f}
              >
                {f}
              </span>
            ))}
          </div>
          <div className="overflow-hidden py-4">
            <Code lines={source(step.prompt)} marks={step.marks} />
          </div>
        </div>

        {/* Docked to the window's bottom edge on a phone, and hanging off its right side on a wide screen. */}
        <div className="v3-card absolute inset-x-3 bottom-24 lg:inset-x-auto lg:right-0 lg:top-44 lg:bottom-auto lg:w-[460px] overflow-hidden">
          <div className="flex border-b border-white/10 text-[13px]">
            {STEPS.map((s, i) => (
              <button
                aria-pressed={i === active}
                className={`focus-ring relative flex-1 h-10 cursor-pointer transition-colors ${i === active ? 'text-white' : 'text-white/45 hover:text-white/80'}`}
                key={s.label}
                onClick={() => {
                  track('v3_code_step_selected', { step: s.label });
                  setActive(i);
                  setHeld(true);
                }}
                type="button"
              >
                {s.label}
                {i === active && (
                  <span
                    className={`absolute inset-x-0 bottom-0 h-px bg-clover origin-left ${held ? '' : 'v3-cycle'}`}
                    key={active}
                    style={{ '--ms': `${CYCLE_MS}ms` } as React.CSSProperties}
                  />
                )}
              </button>
            ))}
          </div>
          <div
            className="v3-enter min-h-40 p-4 font-mono text-[12.5px] md:text-[13px] leading-[21px] text-[#BDBDC4]"
            key={active}
          >
            <p className="text-white break-words">
              <span className="text-clover">$ </span>
              {step.command}
            </p>
            <div className="mt-2.5">{step.output}</div>
          </div>
        </div>
      </div>
    </figure>
  );
}
