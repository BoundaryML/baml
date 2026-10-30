import type { CSSProperties, ReactNode } from 'react';
import { section } from '../analytics';
import { Cmd } from '../ui';
import Seen from './seen';

// Each language's toolchain as a stack of blocks. Rows run bottom to top, so the filled blocks climb
// like a staircase and BAML's keeps going past the others. Go's pprof is here on purpose: production
// profiling isn't new, and the claim is about what a language keeps of each call.
const LANGS = ['C (1972)', 'Go (2009)', 'Rust (2015)', 'BAML'];
const ROWS: { label: string; cells: [string, string, string, string] }[] = [
  { cells: ['', '', '', 'baml describe'], label: 'Code discovery for agents' },
  { cells: ['', '', '', 'baml query'], label: 'SQL over recorded calls' },
  {
    cells: ['', '', '', 'On by default'],
    label: 'A typed record of every call',
  },
  {
    cells: ['', 'pprof, sampled', '', 'Exact counts per call path'],
    label: 'Production profiling',
  },
  {
    cells: ['', 'go build, go mod', 'cargo', 'baml generate'],
    label: 'Build and dependencies',
  },
  { cells: ['', 'go test', 'cargo test', 'baml test'], label: 'Test runner' },
  { cells: ['', 'gofmt', 'rustfmt', 'baml fmt'], label: 'Formatter' },
  { cells: ['✓', '✓', '✓', '✓, also at runtime'], label: 'Compiler and types' },
];

const isCommand = (cell: string) =>
  /^(baml|go|cargo|gofmt|rustfmt|pprof)\b/.test(cell);

const TRACE = [
  { ms: '412ms', name: 'main', start: 0, width: 100 },
  { ms: '401ms', name: 'ExtractInvoice', start: 2, width: 95 },
  { ms: '388ms', name: 'LLM call', start: 4, width: 88 },
  { ms: '4ms', name: 'parse → Invoice', start: 93, width: 3 },
];

function TracePicture() {
  return (
    <div className="flex flex-col gap-2.5 font-mono text-[12px]">
      {TRACE.map((t, i) => (
        <div key={t.name}>
          <div className="flex justify-between mb-1 text-white/70">
            <span>{t.name}</span>
            <span className="text-white/45">{t.ms}</span>
          </div>
          <div className="relative h-1.5 rounded-full bg-white/8">
            <div
              className="v2-bar absolute inset-y-0 rounded-full bg-clover"
              style={
                {
                  '--delay': `${i * 120}ms`,
                  left: `${t.start}%`,
                  width: `${t.width}%`,
                } as CSSProperties
              }
            />
          </div>
        </div>
      ))}
    </div>
  );
}

function TypesPicture() {
  return (
    <div className="font-mono text-[12px] leading-[1.7]">
      <div className="rounded-lg border border-white/12 px-3 py-2 text-white/80">
        <span className="text-clover">class</span> Verdict {'{'} label: Label{' '}
        {'}'}
      </div>
      <svg
        aria-hidden="true"
        className="block ml-6 w-3 h-7"
        viewBox="0 0 12 28"
      >
        <path className="v2-wire" d="M6 0V28" pathLength={1000} />
        <path className="v2-pulse" d="M6 0V28" pathLength={1000} />
      </svg>
      <div className="rounded-lg border border-clover/40 px-3 py-2 text-white/80">
        output_value{' '}
        <span className="text-periwinkle">{'{"label":"negative"}'}</span>
      </div>
    </div>
  );
}

function CliPicture() {
  return (
    <div className="font-mono text-[12px] leading-[2] text-white/80">
      {['baml test', 'baml describe', 'baml query'].map((c) => (
        <div key={c}>
          <span className="text-clover">$</span> {c}
        </div>
      ))}
    </div>
  );
}

const POINTS: { title: string; body: ReactNode; picture: ReactNode }[] = [
  {
    body: 'BAML runs your functions, so it records every call with its typed arguments and result, the step that failed, the call path, the code version, and the user and metadata you attach. Recording costs about 24 ns a call for timing and profiles, and about 60 ns with captured values.',
    picture: <TracePicture />,
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
    picture: <TypesPicture />,
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
    picture: <CliPicture />,
    title: 'One CLI for people and agents.',
  },
];

export default function Observability() {
  return (
    <section
      {...section('observability')}
      aria-labelledby="toolchain-title"
      className="max-w-[1440px] mx-auto px-5 md:px-12 pt-20 md:pt-36 scroll-mt-16"
    >
      <h2 className="v2-h2" id="toolchain-title">
        Designed for observability.
      </h2>
      <p className="max-w-160 mt-6 text-xl leading-[1.5] text-white/70">
        Each generation of languages added more capabilities into their
        toolchains. A unified toolchain lets language designers build more
        efficient programs, and a better experience.
      </p>

      <Seen className="relative mt-12 md:mt-16 overflow-x-auto">
        <table className="w-full min-w-[760px] table-fixed border-separate border-spacing-1 text-left text-sm">
          <thead>
            <tr>
              <th className="w-[24%] font-normal" scope="col">
                <span className="sr-only">Included with the language</span>
              </th>
              {LANGS.map((l) => (
                <th
                  className={`px-3 pb-3 font-display font-medium text-lg ${l === 'BAML' ? 'text-clover' : 'text-white/60'}`}
                  key={l}
                  scope="col"
                >
                  {l}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {ROWS.map((row, r) => (
              <tr key={row.label}>
                <th
                  className="pr-3 font-normal text-[13px] md:text-sm text-white/60"
                  scope="row"
                >
                  {row.label}
                </th>
                {row.cells.map((cell, c) => (
                  <td
                    className={
                      cell
                        ? `v2-block h-12 px-3 rounded-[8px] ${LANGS[c] === 'BAML' ? 'bg-clover-900 border border-clover/50 font-medium' : 'bg-white/8 text-white/75'} ${isCommand(cell) ? 'font-mono text-[12.5px]' : ''}`
                        : ''
                    }
                    key={LANGS[c]}
                    style={
                      cell
                        ? ({
                            '--col': c,
                            '--row': ROWS.length - 1 - r,
                          } as CSSProperties)
                        : undefined
                    }
                  >
                    {cell || <span className="sr-only">Not included</span>}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </Seen>

      <h3 className="mt-16 md:mt-24 font-display font-medium text-[clamp(24px,2.6vw,38px)] leading-[1.15] tracking-[-0.03em]">
        BAML builds the richest telemetry.
      </h3>
      <Seen className="grid md:grid-cols-3 gap-px mt-8 rounded-2xl overflow-hidden bg-white/12 border border-white/12">
        {POINTS.map((p) => (
          <div className="flex flex-col bg-black" key={p.title}>
            <div className="flex flex-col justify-center min-h-48 p-6 md:p-7 border-b border-white/12">
              {p.picture}
            </div>
            <div className="p-6 md:p-7">
              <h4 className="font-display font-medium text-[21px] leading-[27px] tracking-[-0.03em]">
                {p.title}
              </h4>
              <p className="mt-2.5 text-[15px] leading-[23px] text-white/65">
                {p.body}
              </p>
            </div>
          </div>
        ))}
      </Seen>
    </section>
  );
}
