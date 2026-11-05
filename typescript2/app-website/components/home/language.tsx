'use client';

import lambWhite from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import Image from 'next/image';
import { useState } from 'react';
import { Code, proseLines } from './code';
import { Phase } from './phase';

// Section 4, Build: the language, with the sample the rest of the page uses, and the two commands that
// run before a deploy.

// An agent built on triage.baml's classify: an AI function with tools, which are ordinary functions.
// The model calls them until it can answer.
const AGENT = [
  '// Tools are ordinary functions.',
  "/// Look up a customer's plan and billing status.",
  'function lookup_customer(email: string) -> string {',
  // biome-ignore lint/suspicious/noTemplateCurlyInString: this is BAML source, whose own template string uses ${}.
  '    `${email}: pro plan, paid through March`',
  '}',
  '',
  '/// Search past tickets for ones like this one.',
  'function search_tickets(query: string) -> string[] {',
  '    ["Charged twice", "Wrong plan on invoice"]',
  '}',
  '',
  '// An agent is an AI function with tools.',
  'function support_agent(ticket: string) -> string {',
  '    client: "openai/gpt-6-luna"',
  '    tools: [classify, lookup_customer, search_tickets]',
  '    prompt: `',
  '        Answer this ticket. Look up the customer first.',
  // biome-ignore lint/suspicious/noTemplateCurlyInString: this is BAML source, shown as text.
  '        ${ticket}',
  '    `',
  '}',
];

// More of the language, one tab each. Every example builds on triage.baml's classify and Label, and
// each one passes baml check and is laid out as baml fmt writes it. S3Client in the Tests tab is a
// stand-in: the standard library has no S3 client, so it was checked against a stub.
const EXAMPLES = [
  { lines: AGENT, name: 'Agents' },
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

// What BAML takes from each language, one line each: the traits are underlined in the language's
// color, and the language is named in it. Python's line strikes out its second trait. The colors are
// each language's own, lightened where needed to read on the dark ground.
const PARTS: {
  color: string;
  logo: string;
  name: string;
  traits: [string, string];
  struck?: boolean;
}[] = [
  {
    color: '#4b8fe0',
    logo: '/logos/typescript.svg',
    name: 'TypeScript',
    traits: ['syntax', 'readability'],
  },
  {
    color: '#f0703a',
    logo: '/rust-crab.svg',
    name: 'Rust',
    traits: ['correctness', 'tooling'],
  },
  {
    color: '#00add8',
    logo: '/logos/go.svg',
    name: 'Go',
    traits: ['compile times', 'concurrency'],
  },
  {
    color: '#e3b53c',
    logo: '/logos/python.svg',
    name: 'Python',
    struck: true,
    traits: ['dynamism', 'nothing else'],
  },
];

/** The list of what BAML takes from TypeScript, Rust, Go, and Python, under "BAML has the:". It sits
 *  beside the code on wide screens and above it on narrow ones. */
function Parts() {
  return (
    <div className="order-first lg:order-none text-[21px] md:text-[25px] leading-[1.3] text-head">
      <p className="flex items-center gap-4">
        <span className="grid w-10 shrink-0 place-items-center">
          {/* The lamb at the brand's 32px minimum. */}
          <Image
            alt=""
            className="h-8 w-auto"
            height={32}
            src={lambWhite}
            width={27}
          />
        </span>
        <span>
          <strong className="font-semibold text-clover">BAML</strong> has the:
        </span>
      </p>
      <ul className="mt-7 md:mt-9 flex flex-col gap-6 md:gap-7">
        {PARTS.map((part) => {
          const [first, second] = part.traits;
          const line = { textDecorationColor: part.color };
          return (
            <li className="flex items-center gap-4" key={part.name}>
              <span className="grid w-10 shrink-0 place-items-center">
                {/* biome-ignore lint/performance/noImgElement: a small local SVG, which next/image would only pass through. */}
                <img
                  alt=""
                  className="max-h-9 w-auto max-w-10"
                  height={36}
                  src={part.logo}
                  width={40}
                />
              </span>
              <span>
                <span
                  className="underline decoration-2 underline-offset-[7px]"
                  style={line}
                >
                  {first}
                </span>{' '}
                and{' '}
                <span
                  className={
                    part.struck
                      ? 'line-through decoration-2'
                      : 'underline decoration-2 underline-offset-[7px]'
                  }
                  style={line}
                >
                  {second}
                </span>{' '}
                of{' '}
                <strong className="font-semibold" style={{ color: part.color }}>
                  {part.name}
                </strong>
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** Section 4, Build: the code panel of language examples beside what BAML takes from other languages. */
export default function Language() {
  return (
    <Phase
      heading="Designed for agents."
      id="build"
      intro="BAML is a fully open-source language. It takes the parts of each language that matter for agents, and leaves the rest."
      name="Build"
    >
      <div className="grid lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] gap-x-14 gap-y-12 items-center">
        <Examples />
        <Parts />
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
