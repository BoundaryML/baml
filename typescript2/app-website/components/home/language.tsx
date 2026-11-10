'use client';

import lambWhite from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import Image from 'next/image';
import { useState } from 'react';
import { SiGithub } from 'react-icons/si';
import { section } from './analytics';
import { Code, type LineMark, proseLines } from './code';
import { JEV_MODE } from './jev-mode';
import { STARS } from './nav';
import { LINKS } from './ui';

// Section 4, Build: what BAML takes from the languages it's designed like, and the language itself in
// a tab per idea, each example checked with baml check.

// The language, one tab per idea. Each example compiles on its own (baml check, in an empty project)
// and is laid out as baml fmt writes it. Each tab highlights its `focus` lines, the ones it is there
// to show, and each of those carries a note.
export const EXAMPLES = [
  {
    focus: [
      'function support_agent(ticket: string) -> Answer | FollowUp {',
      '    tools: [lookup_customer, search_tickets]',
    ],
    lines: [
      '// An agent is an AI function with tools.',
      'function support_agent(ticket: string) -> Answer | FollowUp {',
      '    client: anthropic.Client.new(model = "claude-sonnet-5-5")',
      '    tools: [lookup_customer, search_tickets]',
      '    prompt: `',
      '        Answer this ticket, or ask one question.',
      '        Look up the customer first.',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, whose template strings use ${}.
      '        ${ticket}',
      '    `',
      '}',
      '',
      'class Answer {',
      '    reply: string,',
      '}',
      '',
      'class FollowUp {',
      '    question: string,',
      '}',
      '',
      '// Tools are plain functions. Their docs tell the model what they do.',
      "/// Look up a customer's plan and billing status.",
      'function lookup_customer(email: string) -> string {',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, whose template strings use ${}.
      '    `${email}: pro plan, paid through March`',
      '}',
      '',
      '/// Search past tickets for ones like this one.',
      'function search_tickets(query: string) -> string[] {',
      '    ["Charged twice", "Wrong plan on invoice"]',
      '}',
    ],
    name: 'Agents',
  },
  {
    focus: ['        spawn { classify(t) }', '    await baml.future.all(jobs)'],
    lines: [
      '// Classify every ticket at once. Each spawn is a green thread.',
      'function triage_all(tickets: string[]) -> Label[] {',
      '    let jobs = tickets.map((t) -> {',
      '        spawn { classify(t) }',
      '    });',
      '    await baml.future.all(jobs)',
      '}',
      '',
      'type Label = "billing" | "bug" | "other";',
      '',
      'function classify(ticket: string) -> Label {',
      '    client: openai.OllamaClient.new(model = "llama3.3")',
      '    prompt: `',
      '        Classify this support ticket.',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, whose template strings use ${}.
      '        ${ticket}',
      '    `',
      '}',
    ],
    name: 'Green threads',
  },
  {
    focus: [
      'function handle(ticket: string) -> string throws never {',
      '        Unroutable => "triage-queue",',
      '        ai.errors.Failure | baml.errors.Timeout => "retry-queue",',
    ],
    lines: [
      "// handle can't throw. baml check proves every error is handled.",
      'function handle(ticket: string) -> string throws never {',
      '    route(ticket) catch (e) {',
      '        Unroutable => "triage-queue",',
      '        ai.errors.Failure | baml.errors.Timeout => "retry-queue",',
      '        _ => "on-call",',
      '    }',
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
      'class Unroutable {',
      '    label: Label,',
      '}',
      '',
      'type Label = "billing" | "bug" | "other";',
      '',
      'function classify(ticket: string) -> Label {',
      '    client: google.GeminiClient.new(model = "gemini-3-pro")',
      '    prompt: `',
      '        Classify this support ticket.',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, whose template strings use ${}.
      '        ${ticket}',
      '    `',
      '}',
    ],
    name: 'Typed errors',
  },
  {
    focus: ['    let type Row = row.build();'],
    lines: [
      "// Make a type at runtime from a spreadsheet's columns.",
      '// The model fills it in, and its reply is checked against it.',
      'function extract_row(doc: string, columns: string[]) -> unknown {',
      '    let row = reflect.Class.builder("Row");',
      '    for (let c in columns) {',
      '        row.field(c, reflect.Type.of<string>());',
      '    }',
      '    let type Row = row.build();',
      '    extract<Row>(doc)',
      '}',
      '',
      'function extract<T>(doc: string) -> T {',
      '    client: openai.ResponsesClient.new(model = "gpt-6-luna")',
      '    prompt: `',
      '        Fill this in from the document.',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, whose template strings use ${}.
      '        ${doc}',
      '    `',
      '}',
    ],
    name: 'Reflection',
  },
  {
    focus: [
      '    let n = reflect.Session.new().eval<int>(code) catch (e) {',
      '        reflect.errors.CompilationError => {',
      '    assert.equal(session.eval<int>("total + 2"), 42)',
    ],
    lines: [
      "// Run an agent's code. eval<int> type-checks it before it runs.",
      'function run(code: string) -> string {',
      '    let n = reflect.Session.new().eval<int>(code) catch (e) {',
      '        reflect.errors.CompilationError => {',
      '            return "rejected";',
      '        }',
      '        reflect.errors.EvaluationError => {',
      '            return "threw";',
      '        }',
      '    };',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, whose template strings use ${}.
      '    `got ${n}`',
      '}',
      '',
      'test "an int runs" {',
      '    assert.equal(run("6 * 7"), "got 42")',
      '}',
      '',
      'test "a string never runs" {',
      '    assert.equal(run(`"forty-two"`), "rejected")',
      '}',
      '',
      'test "a runtime error is caught" {',
      '    assert.equal(run(`baml.json.decode<int>("nope")`), "threw")',
      '}',
      '',
      'test "a session remembers" {',
      '    let session = reflect.Session.new();',
      '    session.eval<void>("let total = 40;");',
      '    assert.equal(session.eval<int>("total + 2"), 42)',
      '}',
    ],
    name: 'Type-safe eval',
  },
  {
    focus: ['        test c.ticket {'],
    lines: [
      '// A test sits in the same file as the code it tests.',
      'test "sarcasm is billing" {',
      '    assert.equal(classify("Charged twice again. Love that."), "billing")',
      '}',
      '',
      '// One test per ticket in the file, named for the ticket:',
      '//   production tickets::Charged twice for one order',
      '//   production tickets::App crashes on login',
      'testset "production tickets" {',
      '    let cases = baml.json.decode<Case[]>(baml.fs.read("evals/cases.json"));',
      '    for (let c in cases) {',
      '        test c.ticket {',
      '            assert.equal(classify(c.ticket), c.label)',
      '        }',
      '    }',
      '}',
      '',
      'class Case {',
      '    ticket: string,',
      '    label: Label,',
      '}',
      '',
      'type Label = "billing" | "bug" | "other";',
      '',
      'function classify(ticket: string) -> Label {',
      '    client: anthropic.Client.new(model = "claude-haiku-5-5")',
      '    prompt: `',
      '        Classify this support ticket.',
      // biome-ignore lint/suspicious/noTemplateCurlyInString: BAML source, whose template strings use ${}.
      '        ${ticket}',
      '    `',
      '}',
    ],
    name: 'Unit tests',
  },
  {
    focus: ['    if (email.feels("urgent")) {', 'implements<S> Vibes for S {'],
    lines: JEV_MODE,
    name: 'Jev-mode',
  },
];

// What BAML takes from each language, as "<Language>-like <trait>": the line people share, so it
// is the section's headline, set big. Python's line is the joke.
const LINEAGE = [
  { color: '#4b8fe0', name: 'TypeScript', trait: 'syntax' },
  { color: '#f0703a', name: 'Rust', trait: 'correctness' },
  { color: '#00add8', name: 'Go', trait: 'concurrency' },
  { color: '#a179dc', name: 'C#', trait: 'reflection' },
];
const PYTHON = '#e3b53c';

/** The lineage list, one language per line. */
function Lineage() {
  const line =
    'py-2 whitespace-nowrap font-display text-[clamp(22px,2.15vw,32px)] font-medium leading-tight tracking-[-0.02em] text-head';
  return (
    <ul>
      {LINEAGE.map((part) => (
        <li className={line} key={part.name}>
          <span style={{ color: part.color }}>{part.name}</span>
          <span className="text-white/60">-like</span>{' '}
          <span className="underline decoration-clover/50 decoration-2 underline-offset-[6px]">
            {part.trait}
          </span>
        </li>
      ))}
      <li className={line}>
        <span style={{ color: PYTHON }}>Python</span>
        <span className="text-white/60">, well, you know, Python.</span>
      </li>
    </ul>
  );
}

/** Section 4, Build: the heading and the facts, then the lineage list beside the code, which is
 *  there for whoever wants to read it: quiet, and cut short until asked for. */
export default function Language() {
  return (
    <section
      {...section('build')}
      aria-labelledby="build"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        <h2 className="v4-h2 text-head max-w-[22em]" id="build">
          <span className="text-clover">BAML</span> is designed like the
          languages you already trust.
          {/* The lamb leaping off the sentence, at the heading's height. */}
          <Image
            alt=""
            className="ml-3 inline-block h-[1.1em] w-auto -mt-[0.2em] align-middle"
            height={32}
            src={lambWhite}
            width={27}
          />
        </h2>
        {/* Counts from GitHub, rounded; update them by hand as they grow. */}
        <ul className="mt-5 flex flex-wrap items-center gap-x-6 gap-y-2 font-mono text-[14px] text-white/70">
          <li>
            <a
              className="flex items-center gap-2 hover:text-white"
              href={LINKS.github}
              rel="noopener noreferrer"
              target="_blank"
            >
              <SiGithub aria-hidden className="text-[16px]" />
              <span className="text-white">{STARS}</span> stars
            </a>
          </li>
          <li>
            Open source, <span className="text-white">Apache 2.0</span>
          </li>
          <li>Free to use</li>
          <li>No internet required</li>
        </ul>
        <div className="mt-10 md:mt-14 grid lg:grid-cols-[auto_minmax(0,1fr)] gap-x-14 gap-y-10 items-start">
          <Lineage />
          <Examples quiet />
        </div>
      </div>
    </section>
  );
}

// What each highlighted line does, shown when it's hovered or focused. Keyed by the line itself, so
// a note can't drift onto the wrong line when an example changes.
const NOTES: Record<string, string> = {
  '        ai.errors.Failure | baml.errors.Timeout => "retry-queue",':
    'A failed or timed-out model call is a typed error too, so the worth-retrying ones get their own route.',
  '        reflect.errors.CompilationError => {':
    "Code that isn't an int lands here, before a single line of it runs.",
  '        spawn { classify(t) }':
    'spawn runs classify on a green thread and hands back a future right away, so every ticket is in flight at once.',
  '        test c.ticket {':
    'One test per case, made at runtime and named for its ticket, so baml test lists production tickets::Charged twice for one order.',
  '        Unroutable => "triage-queue",':
    "route's type says it can throw Unroutable, so handle catches it by type, not by message.",
  '    assert.equal(session.eval<int>("total + 2"), 42)':
    'A session keeps what earlier evals defined, so an agent can build up state step by step.',
  '    await baml.future.all(jobs)':
    'Waits for every future. The result is a Label[] in the same order as the tickets.',
  '    if (email.feels("urgent")) {':
    'Asks Jev whether the email feels urgent, and gets back a bool.',
  '    let n = reflect.Session.new().eval<int>(code) catch (e) {':
    "Compiles the agent's code and type-checks it as an int before running it. Nothing else gets to run.",
  '    let type Row = row.build();':
    'Turns the class built at runtime into a real type, so extract<Row> can use it like any other.',
  '    tools: [lookup_customer, search_tickets]':
    'Tools are plain BAML functions. The model calls them until it can answer, and their /// docs tell it what each one does.',
  'function handle(ticket: string) -> string throws never {':
    "throws never is checked: baml check fails if any error route can throw isn't handled below.",
  'function support_agent(ticket: string) -> Answer | FollowUp {':
    'The return type is a union. The model either answers or asks a question, and you get back whichever it picked, typed.',
  'implements<S> Vibes for S {':
    'A blanket implementation: every type S gets Vibes, so .feels() works on strings, numbers, classes, anything.',
};

/** The notes for an example's highlighted lines, by line number. */
function focusNotes(lines: string[], focus: string[]): Record<number, string> {
  return Object.fromEntries(
    focus
      .filter((line) => NOTES[line])
      .map((line) => [lines.indexOf(line), NOTES[line]]),
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

export function Examples({
  index,
  onIndex,
  quiet,
}: {
  /** The tab shown, when the caller controls it. */
  index?: number;
  onIndex?: (i: number) => void;
  /** A quieter panel for when the code is there for the curious, not the headline: smaller tabs,
   *  dimmer code, and only the first lines until the reader asks for the rest. */
  quiet?: boolean;
} = {}) {
  const [own, setOwn] = useState(0);
  const shown = index ?? own;
  const setShown = (i: number) => {
    setOwn(i);
    onIndex?.(i);
  };
  const [open, setOpen] = useState(false);
  const capped = quiet && !open;
  return (
    <figure
      className={`rounded-xl overflow-hidden border bg-raised ${quiet ? 'border-rule/70' : 'border-rule'}`}
    >
      {/* On a phone the tabs wrap onto a second row, so every example shows instead of hiding off
          the edge of a scrolling strip. */}
      <figcaption
        className={`flex items-center gap-1 px-2 py-1.5 border-b border-rule bg-raised ${quiet ? 'flex-nowrap overflow-x-auto [scrollbar-width:none]' : 'flex-wrap md:flex-nowrap md:overflow-x-auto'}`}
      >
        {EXAMPLES.map((e, i) => (
          <button
            aria-pressed={i === shown}
            className={`focus-ring shrink-0 rounded-md transition-colors ${quiet ? 'px-2 py-1 text-[13px]' : 'v4-caption px-2.5 py-1.5'} ${i === shown ? 'bg-rule text-head' : 'text-muted hover:text-ink-2'}`}
            key={e.name}
            onClick={() => setShown(i)}
            type="button"
          >
            {e.name}
          </button>
        ))}
      </figcaption>
      <div
        className={`relative grid grid-cols-[minmax(0,1fr)] py-4 ${quiet ? 'opacity-80 overflow-x-auto md:overflow-x-visible' : ''} ${capped ? 'max-h-[380px] overflow-y-hidden md:overflow-hidden' : ''}`}
      >
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
                focusable={!capped}
                lines={e.lines}
                marks={focusMarks(e.lines, e.focus)}
                notes={focusNotes(e.lines, e.focus)}
                prose={[...proseLines(e.lines)]}
                wrap={quiet ? 'md' : true}
              />
            </section>
          ) : (
            <div
              aria-hidden={i !== shown}
              className={`col-start-1 row-start-1 ${i === shown ? '' : 'invisible'}`}
              key={e.name}
            >
              <Code
                focusable={!capped}
                lines={e.lines}
                marks={focusMarks(e.lines, e.focus)}
                notes={focusNotes(e.lines, e.focus)}
                prose={[...proseLines(e.lines)]}
                wrap={quiet ? 'md' : true}
              />
            </div>
          ),
        )}
      </div>
      {quiet && (
        <div className="relative border-t border-rule/70">
          {capped && (
            <span
              aria-hidden
              className="pointer-events-none absolute inset-x-0 bottom-full h-20 bg-gradient-to-t from-raised to-transparent"
            />
          )}
          <button
            aria-expanded={open}
            className="focus-ring w-full cursor-pointer py-2 text-[13px] text-muted hover:text-ink-2"
            onClick={() => setOpen((o) => !o)}
            type="button"
          >
            {open ? 'Show less' : 'Read the code'}
          </button>
        </div>
      )}
    </figure>
  );
}
