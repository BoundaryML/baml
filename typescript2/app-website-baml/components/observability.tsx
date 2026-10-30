import type { ReactNode } from 'react';
import { section } from './analytics';
import Reveal from './reveal';
import { Cmd, LANES, LEFT, PAD_Y, WRAP } from './ui';

/* Each generation of languages pulled more into the toolchain; BAML's addition is the record of what
   ran and SQL to read it. Go's pprof is in the table on purpose: production profiling isn't new, and
   the claim is about what a language keeps of each call. */
const LANGS = ['C (1972)', 'Go (2009)', 'Rust (2015)', 'BAML'];

const ROWS: { label: string; cells: [string, string, string, string] }[] = [
  { cells: ['✓', '✓', '✓', '✓, also at runtime'], label: 'Compiler and types' },
  { cells: ['', 'gofmt', 'rustfmt', 'baml fmt'], label: 'Formatter' },
  { cells: ['', 'go test', 'cargo test', 'baml test'], label: 'Test runner' },
  {
    cells: ['', 'go build, go mod', 'cargo', 'baml generate'],
    label: 'Build and dependencies',
  },
  {
    cells: ['', 'pprof, sampled', '', 'Exact counts per call path'],
    label: 'Production profiling',
  },
  {
    cells: ['', '', '', 'On by default'],
    label: 'A typed record of every call',
  },
  { cells: ['', '', '', 'baml query'], label: 'SQL over recorded calls' },
  {
    cells: ['', '', '', 'baml describe'],
    label: 'Code discovery for agents',
  },
];

const POINTS: { title: string; body: ReactNode }[] = [
  {
    body: 'BAML runs your functions, so it records every call with its typed arguments and result, the step that failed, the call path, the code version, and the user and metadata you attach. Recording costs about 24 ns a call for timing and profiles, and about 60 ns with captured values.',
    title: 'Recorded by the runtime.',
  },
  {
    body: (
      <>
        Each value is stored with the type that produced it, so production data
        uses the same schema as your code. A coding agent that has read your{' '}
        <Cmd>.baml</Cmd> files already knows the schema.
      </>
    ),
    title: 'Stored with your types.',
  },
  {
    body: (
      <>
        <Cmd>baml test</Cmd>, <Cmd>baml describe</Cmd>, and{' '}
        <Cmd>baml query</Cmd> are part of the same toolchain, so a coding agent
        can write a change, run it, and check the results from the terminal.
      </>
    ),
    title: 'One CLI for people and agents.',
  },
];

// A command reads as a command; prose cells like "On by default" stay in the body face.
const isCommand = (cell: string) =>
  /^(baml|go|cargo|gofmt|rustfmt|pprof)\b/.test(cell);

export default function Observability() {
  return (
    <section
      {...section('observability')}
      aria-labelledby="toolchain-title"
      className={`${WRAP} ${PAD_Y}`}
    >
      <div className={`${LANES} mb-10 md:mb-14 lg:mb-16`}>
        <div className={`${LEFT} flex flex-col gap-4`}>
          <h2 className="h2" id="toolchain-title">
            Designed for observability.
          </h2>
        </div>
        <p className="max-w-[520px] lg:pt-2.5 text-[17px] leading-[27px] text-body">
          Each generation of languages added more capabilities into their
          toolchains. A unified toolchain lets language designers build more
          efficient programs, and a better experience.
        </p>
      </div>

      {/* The table scrolls inside its own frame on a phone, so the page itself never scrolls sideways. */}
      <Reveal className="overflow-x-auto rounded-xl border border-rule">
        <table className="w-full min-w-[720px] text-left text-[14px] leading-5">
          <thead>
            <tr className="text-muted">
              <th className="p-4 font-normal" scope="col">
                Included with the language
              </th>
              {LANGS.map((l) => (
                <th
                  className={`p-4 font-normal ${l === 'BAML' ? 'bg-raised font-display font-medium text-accent' : ''}`}
                  key={l}
                  scope="col"
                >
                  {l}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {ROWS.map((r) => (
              <tr className="border-t border-rule" key={r.label}>
                <th className="p-4 font-normal text-ink" scope="row">
                  {r.label}
                </th>
                {r.cells.map((cell, i) => (
                  <td
                    className={`p-4 ${i === 3 ? 'bg-raised text-ink font-medium' : 'text-body'} ${isCommand(cell) ? 'font-mono text-[13px]' : ''}`}
                    key={LANGS[i]}
                  >
                    {cell || <span className="sr-only">Not included</span>}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </Reveal>

      <h3 className="mt-14 md:mt-20 mb-8 md:mb-10 font-display font-medium text-[clamp(26px,2.4vw,34px)] leading-[1.18] tracking-[-0.03em]">
        BAML builds the richest telemetry.
      </h3>
      <ul className="grid md:grid-cols-3 gap-x-10 gap-y-9">
        {POINTS.map((p, i) => (
          <li key={p.title}>
            <Reveal
              className={`h-full flex flex-col gap-2.5 pt-5 border-t ${i === 0 ? 'border-rule-accent' : 'border-rule'}`}
              delay={i * 60}
            >
              <h4 className="font-display font-medium text-[21px] leading-[27px] tracking-[-0.03em]">
                {p.title}
              </h4>
              <p className="text-[15px] leading-[23px] text-body">{p.body}</p>
            </Reveal>
          </li>
        ))}
      </ul>
    </section>
  );
}
