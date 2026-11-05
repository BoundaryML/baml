'use client';

import lambWhite from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import Image from 'next/image';
import { useState } from 'react';
import { Code, type LineMark, proseLines } from './code';
import { JEV_MODE } from './jev-mode';
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

// More of the language, one tab each. Each tab highlights its `focus` lines, the ones the example is
// there to show. Every example builds on triage.baml's classify and Label, and
// each one passes baml check and is laid out as baml fmt writes it. S3Client in the Tests tab is a
// stand-in: the standard library has no S3 client, so it was checked against a stub.
const EXAMPLES = [
  {
    focus: ['    tools: [classify, lookup_customer, search_tickets]'],
    lines: AGENT,
    name: 'Agents',
  },
  {
    focus: ['        spawn { classify(t) }', '    await baml.future.all(jobs)'],
    lines: [
      '// Classify every ticket at once. Each spawn runs on a green thread.',
      'function triage_all(tickets: string[]) -> Label[] {',
      '    let jobs = tickets.map((t) -> {',
      '        spawn { classify(t) }',
      '    });',
      '    await baml.future.all(jobs)',
      '}',
    ],
    name: 'Green threads',
  },
  {
    focus: [
      '        Unroutable => "triage-queue",',
      '        ai.errors.Failure => "retry-queue",',
    ],
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
    focus: ['    match (reflect.Type.of<T>().as_class()) {'],
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
    focus: ['        test c.ticket {'],
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
  // Far longer than the rest, so it scrolls inside the panel instead of setting its height.
  {
    focus: ['    if (email.feels("urgent")) {'],
    lines: JEV_MODE,
    name: 'Jev-mode',
    scroll: true,
  },
];

// What BAML takes from each language, one line each in the page's ruled list. The language is named in
// its color, and the two traits are set a little heavier, over a faint underline in the brand's purple.
// Python's second trait is struck out in the same purple.
const PARTS: {
  color: string;
  name: string;
  traits: [string, string];
  struck?: boolean;
}[] = [
  { color: '#4b8fe0', name: 'TypeScript', traits: ['syntax', 'readability'] },
  { color: '#f0703a', name: 'Rust', traits: ['correctness', 'tooling'] },
  { color: '#00add8', name: 'Go', traits: ['compile times', 'concurrency'] },
  {
    color: '#e3b53c',
    name: 'Python',
    struck: true,
    traits: ['dynamism', 'nothing else'],
  },
];

/** A trait, set a little heavier over a faint purple underline, or struck out in the same purple. */
function Trait({ struck, text }: { struck?: boolean; text: string }) {
  return (
    <span
      className={`font-medium decoration-[1.5px] decoration-clover/50 ${struck ? 'line-through' : 'underline underline-offset-[5px]'}`}
    >
      {text}
    </span>
  );
}

/** The ruled list of what BAML takes from TypeScript, Rust, Go, and Python. */
function Parts() {
  return (
    <ul className="border-t border-rule">
      {PARTS.map((part) => (
        <li
          className="v4-body text-head py-3.5 border-b border-rule"
          key={part.name}
        >
          <Trait text={part.traits[0]} /> and{' '}
          <Trait struck={part.struck} text={part.traits[1]} /> of{' '}
          <strong className="font-semibold" style={{ color: part.color }}>
            {part.name}
          </strong>
        </li>
      ))}
    </ul>
  );
}

/** Section 4, Build: the code panel of language examples beside what BAML takes from other languages. */
export default function Language() {
  return (
    <Phase
      heading="Designed for agents."
      id="build"
      intro={
        <>
          BAML is a fully open-source, completely free-to-use language.
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
        <Parts />
      </div>
    </Phase>
  );
}

/** The code panel, with a tab per example. Every example is drawn in the same cell, and only the chosen
 *  one is visible, so the panel keeps the height of the longest and doesn't jump between tabs. */
/** The marks for an example's focus lines. */
function focusMarks(
  lines: string[],
  focus: string[],
): Record<number, LineMark> {
  return Object.fromEntries(
    focus.map((line) => [lines.indexOf(line), 'running' as const]),
  );
}

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
      <div className="relative grid grid-cols-[minmax(0,1fr)] py-4">
        {EXAMPLES.map((e, i) =>
          'scroll' in e ? (
            <section
              aria-hidden={i !== shown}
              aria-label={`${e.name} example`}
              className={`absolute inset-0 overflow-y-auto py-4 focus-ring ${i === shown ? '' : 'invisible'}`}
              key={e.name}
              // A scrolling region takes focus so the keyboard can scroll it.
              tabIndex={i === shown ? 0 : -1}
            >
              <Code
                lines={e.lines}
                marks={focusMarks(e.lines, e.focus)}
                prose={[...proseLines(e.lines)]}
                wrap
              />
            </section>
          ) : (
            <div
              aria-hidden={i !== shown}
              className={`col-start-1 row-start-1 ${i === shown ? '' : 'invisible'}`}
              key={e.name}
            >
              <Code
                lines={e.lines}
                marks={focusMarks(e.lines, e.focus)}
                prose={[...proseLines(e.lines)]}
                wrap
              />
            </div>
          ),
        )}
      </div>
    </figure>
  );
}
