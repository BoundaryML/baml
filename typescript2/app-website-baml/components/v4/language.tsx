'use client';

import { Code, proseLines } from '../v3/code';
import { FIRST, source } from '../v3/hero-video';
import { Cmd, Panel, Phase, Points } from './phase';

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
    body: 'Declare a model call like any other function, with typed arguments and a typed result. Tests, testsets, and evaluations sit next to the code.',
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
      intro="BAML is a language designed for agents."
      name="Build"
    >
      <div className="grid lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] gap-x-12 gap-y-10 items-start">
        <Panel title="triage.baml">
          <div className="py-4 overflow-x-auto">
            <Code lines={LINES} prose={[...proseLines(LINES)]} />
          </div>
        </Panel>
        <Points points={POINTS} />
      </div>
    </Phase>
  );
}
