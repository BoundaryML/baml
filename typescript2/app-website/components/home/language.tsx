'use client';

import lambWhite from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import Image from 'next/image';
import { useState } from 'react';
import { Code, proseLines } from './code';
import { FIRST, source } from './hero-video';
import { Phase } from './phase';

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
// each one passes baml check and is laid out as baml fmt writes it. S3Client in the Tests tab is a
// stand-in: the standard library has no S3 client, so it was checked against a stub.
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
      'class Order {',
      '    id: int,',
      '    total: float,',
      '}',
      '',
      "// Types are values at runtime. This lists a class's fields and their types.",
      'function fields_of<T>() -> string[] {',
      '    match (reflect.Type.of<T>().as_class()) {',
      '        let c: reflect.Class => {',
      '            c.fields().map((f) -> {',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: this is BAML source, whose own template string uses ${}.
      '                `${f.name}: ${f.type}`',
      '            })',
      '        },',
      '        null => [],',
      '    }',
      '}',
      '',
      'test "lists fields" {',
      '    assert.equal(fields_of<Order>(), ["id: int", "total: float"])',
      '}',
    ],
    name: 'Reflection',
  },
  {
    lines: [
      '// A test sits in the same file as the code it tests.',
      'test "sarcasm is billing" {',
      '    assert.equal(classify("Oh great, charged twice again. Love that."), "billing")',
      '}',
      '',
      'class Case {',
      '    ticket: string,',
      '    label: Label,',
      '}',
      '',
      '// Load real tickets from S3 and make a test for each one.',
      'testset "production tickets" {',
      '    let s3 = S3Client { region: "us-east-1" };',
      '    let cases = s3.get_json<Case[]>("triage-evals", "cases.json");',
      '    for (let c in cases) {',
      '        test c.ticket {',
      '            assert.equal(classify(c.ticket), c.label)',
      '        }',
      '    }',
      '}',
    ],
    name: 'Tests',
  },
];

// One line each, as boundaryml.com put them before the Build, Deploy, Observe, and Improve sections.
const CLAIMS = [
  'A sound type system, like Rust',
  'Fast compilation times, like Go',
  'Low-cost telemetry, built into the runtime',
  'Built-in AI capabilities',
  'Fully integrated with Boundary Cloud (or bring your own)',
  'Cross-compatible with Python, TypeScript, Rust, and more',
];

/** Section 4, Build: the code panel of language examples beside the points about the language. */
export default function Language() {
  return (
    <Phase
      heading="Build with the best parts of TypeScript, Rust, and Go."
      id="build"
      intro={
        <>
          BAML is a fully open-source language designed for agents.
          {/* The lamb at the brand's 32px minimum, pulled into the line so it doesn't grow it. */}
          <Image
            alt=""
            className="ml-3 inline-block h-8 w-auto -my-1 align-middle"
            height={32}
            src={lambWhite}
            width={27}
          />
        </>
      }
      name="Build"
    >
      <div className="grid lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] gap-x-12 gap-y-10 items-start">
        <Examples />
        <ul className="border-t border-rule">
          {CLAIMS.map((line) => (
            <li
              className="v4-body text-head py-3.5 border-b border-rule"
              key={line}
            >
              {line}
            </li>
          ))}
        </ul>
      </div>
    </Phase>
  );
}

/** The code panel, with a tab per example. Every example is drawn in the same cell, and only the chosen
 *  one is visible, so the panel keeps the height of the longest and doesn't jump between tabs. */
function Examples() {
  const [shown, setShown] = useState(0);
  return (
    <figure className="rounded-xl overflow-hidden border border-rule bg-raised">
      <figcaption className="flex items-center gap-1 overflow-x-auto px-2 py-1.5 border-b border-rule bg-raised">
        {EXAMPLES.map((e, i) => (
          <button
            aria-pressed={i === shown}
            className={`v4-caption focus-ring shrink-0 rounded-md px-2.5 py-1.5 transition-colors ${i === shown ? 'bg-rule text-head' : 'text-muted hover:text-ink-2'}`}
            key={e.name}
            onClick={() => setShown(i)}
            type="button"
          >
            {e.name}
          </button>
        ))}
      </figcaption>
      <div className="grid grid-cols-[minmax(0,1fr)] py-4">
        {EXAMPLES.map((e, i) => (
          <div
            aria-hidden={i !== shown}
            className={`col-start-1 row-start-1 ${i === shown ? '' : 'invisible'}`}
            key={e.name}
          >
            <Code lines={e.lines} prose={[...proseLines(e.lines)]} wrap />
          </div>
        ))}
      </div>
    </figure>
  );
}
