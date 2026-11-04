'use client';

import { useState } from 'react';
import { Code, proseLines } from '../v3/code';
import { FIRST, source } from '../v3/hero-video';
import { Cmd, Phase, Points } from './phase';

// Section 4, Build: the language, with the sample the rest of the page uses, and the two commands that
// run before a deploy.

// The page's sample, with a comment over classify. Only this section adds it: the video's highlights
// are tied to the sample's line numbers.
const SAMPLE = source(FIRST);
const CLASSIFY = SAMPLE.findIndex((l) => l.startsWith('function classify'));
const LINES = [
  ...SAMPLE.slice(0, CLASSIFY),
  "// 'AI function' syntax for easy extraction",
  ...SAMPLE.slice(CLASSIFY),
];

// More of the language, one tab each. Every example builds on triage.baml's classify and Label, and
// each one passes baml check and is laid out as baml fmt writes it.
const EXAMPLES = [
  { lines: LINES, name: 'AI functions' },
  {
    lines: [
      '// Classify every ticket at once. Each spawn runs on a green thread.',
      'function triage_all(tickets: string[]) -> Label[] {',
      '    let jobs = tickets.map((t) -> {',
      '        spawn { classify(t) }',
      '    });',
      '    jobs.map((job) -> {',
      '        await job',
      '    })',
      '}',
    ],
    name: 'Green threads',
  },
  {
    lines: [
      'class Unroutable {',
      '    label: Label,',
      '}',
      '',
      '// The match has to cover every Label, or baml check fails.',
      'function route(ticket: string) -> string {',
      '    match (classify(ticket)) {',
      '        "billing" => "support",',
      '        "bug" => "engineering",',
      '        "other" => throw Unroutable { label: "other" },',
      '    }',
      '}',
      '',
      '// The errors route can throw are part of its type.',
      'function handle(ticket: string) -> string {',
      '    route(ticket) catch (e) {',
      '        Unroutable => "triage-queue",',
      '        ai.errors.Failure => "retry-queue",',
      '    }',
      '}',
    ],
    name: 'Typed errors',
  },
  {
    lines: [
      'class Ticket {',
      '    id: int,',
      '    body: string,',
      '}',
      '',
      '// decode checks the JSON against Ticket at runtime.',
      '// There is no any to cast to.',
      'function parse(raw: string) -> Ticket? {',
      '    baml.json.decode<Ticket>(raw) catch (_) {',
      '        let _ => null',
      '    }',
      '}',
    ],
    name: 'Runtime types',
  },
  {
    lines: [
      '// A test sits in the same file as the code it tests.',
      'test "sarcasm is billing" {',
      '    assert.equal(classify("Oh great, charged twice again. Love that."), "billing")',
      '}',
    ],
    name: 'Tests',
  },
];

const POINTS = [
  {
    body: 'Unions, generics, and interfaces, in a syntax TypeScript developers already read.',
    title: 'Syntax like TypeScript.',
  },
  {
    body: (
      <>
        Types hold at runtime, with no <Cmd>any</Cmd> and no unchecked casts.
        Errors are part of a function's type, and pattern matching is
        exhaustive.
      </>
    ),
    title: 'A type system like Rust.',
  },
  {
    body: (
      <>
        <Cmd>baml check</Cmd> type-checks 60,000 lines in about 1.4 seconds, and
        green threads run work concurrently with <Cmd>spawn</Cmd> and{' '}
        <Cmd>await</Cmd>.
      </>
    ),
    title: 'Compiles as fast as Go.',
  },
  {
    body: 'Declare a model call like any other function, with typed arguments and a typed result. Declare a test anywhere, like in Rust.',
    title: 'AI functions built in.',
  },
  {
    body: (
      <>
        <Cmd>baml describe</Cmd> finds code by its structure,{' '}
        <Cmd>baml run</Cmd> runs any function from the command line, and{' '}
        <Cmd>baml test</Cmd> runs the tests.
      </>
    ),
    title: 'Tools for agents.',
  },
  {
    body: 'Generate an SDK and move one function at a time.',
    title: 'Use it from Python or TypeScript.',
  },
];

export default function Language() {
  return (
    <Phase
      heading="The best parts of TypeScript, Rust, Go, and Python."
      id="build"
      intro="BAML is a fully open-source language designed for agents."
      name="Build"
    >
      <div className="grid lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] gap-x-12 gap-y-10 items-start">
        <Examples />
        <Points points={POINTS} />
      </div>
    </Phase>
  );
}

/** The code panel, with a tab per example. Every example is drawn in the same cell, and only the chosen
 *  one is visible, so the panel keeps the height of the longest and doesn't jump between tabs. */
function Examples() {
  const [shown, setShown] = useState(0);
  return (
    <figure className="rounded-xl overflow-hidden border border-white/12 bg-[#0b0b0c]">
      <figcaption className="flex items-center gap-1 overflow-x-auto px-2 py-1.5 border-b border-white/[0.08] bg-[#111113]">
        {EXAMPLES.map((e, i) => (
          <button
            aria-pressed={i === shown}
            className={`focus-ring shrink-0 rounded-md px-2.5 py-1 font-mono text-[12px] transition-colors ${i === shown ? 'bg-white/[0.08] text-white' : 'text-white/50 hover:text-white/80'}`}
            key={e.name}
            onClick={() => setShown(i)}
            type="button"
          >
            {e.name}
          </button>
        ))}
      </figcaption>
      <div className="grid py-4 overflow-x-auto">
        {EXAMPLES.map((e, i) => (
          <div
            aria-hidden={i !== shown}
            className={`col-start-1 row-start-1 ${i === shown ? '' : 'invisible'}`}
            key={e.name}
          >
            <Code lines={e.lines} prose={[...proseLines(e.lines)]} />
          </div>
        ))}
      </div>
    </figure>
  );
}
