/** biome-ignore-all lint/performance/noImgElement: small static local marks, sized in css; next/image buys nothing */
import { Fragment } from 'react';
import { createMetadata } from '@/app/_lib/metadata';
import { TryBaml } from '@/app/baml-intro/_components/try-baml';
import { DiscordCta } from '@/components/discord-cta';
import { EapCta } from '@/components/eap-cta';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { Acquirer } from './_components/acquirer';
import { CodeExplorer } from './_components/code-explorer';
import { DroppedFootnote } from './_components/dropped-footnote';
import { LostEvents } from './_components/lost-events';
import { RoomRebus } from './_components/room-rebus';
import { TryBamlFab } from './_components/try-baml-fab';

// Staging page for the /explore redesign. Copy is the single source of truth in
// EXPLORE_COPY_DRAFT.md. Each section's interactive "explorer" artifact is a
// placeholder for now (the shared tabbed-explorer shell comes next).

export const metadata = createMetadata({
  description:
    'Why agents need a new language, and the parts of BAML that are different: AI functions, observability, agent-first tooling, workflows, evals, and an anti-slop base language.',
  ogTitle: 'What is BAML',
  path: '/what-is-baml',
  title: 'What is BAML',
});

const INK = '#1A1612';
const MUTED = '#5C5852';

// {lamb} stands in for a letter mid-word (Sh{lamb}t). A bare "BAML" only gets
// the mark when it opens a sentence: marking every mention put too much purple
// on the page. Lowercase `baml` is the CLI, never the wordmark.
function withLamb(text: string, prefix: number) {
  return text.split(/(\{lamb\}|(?<=^|[.!?]\s)BAML)/).map((chunk, i) => {
    const key = `${prefix}-${i}`;
    if (chunk === '{lamb}') {
      return (
        <img
          alt=""
          className="wib-inline-lamb"
          key={key}
          src="/bamllogopurple.svg"
        />
      );
    }
    if (chunk === 'BAML') {
      return (
        <span className="baml-mark" key={key}>
          <img alt="" src="/bamllogopurple.svg" />
          BAML
        </span>
      );
    }
    return <span key={key}>{chunk}</span>;
  });
}

// Body copy supports `code` spans, [label](href) links, and {lamb}.
function inline(text: string) {
  const parts = text.split(/(`[^`]+`|\[[^\]]+\]\([^)]+\))/);
  return parts.map((part, i) => {
    if (part.startsWith('`') && part.endsWith('`')) {
      return (
        // biome-ignore lint/suspicious/noArrayIndexKey: positional split output
        <code className="wib-code" key={i}>
          {part.slice(1, -1)}
        </code>
      );
    }
    const link = part.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
    if (link) {
      return (
        // biome-ignore lint/suspicious/noArrayIndexKey: positional split output
        <a href={link[2]} key={i}>
          {link[1]}
        </a>
      );
    }
    // biome-ignore lint/suspicious/noArrayIndexKey: positional split output
    return <span key={i}>{withLamb(part, i)}</span>;
  });
}

type Card = { id: string; title: string; line: string };

const CARDS: Card[] = [
  {
    id: 'observability',
    line: 'Every function traced and profiled, locally. Agents can read the traces.',
    title: 'Observability and profiling',
  },
  {
    id: 'ai-functions',
    line: 'Streaming, batching, websockets, voice, any provider.',
    title: 'AI functions',
  },
  {
    id: 'language',
    line: 'No `any`, no unchecked casts, no imports. But we did add pattern matching.',
    title: 'The anti-slop type system',
  },
  {
    id: 'tooling',
    line: 'Compiles faster than Go, searches better than ripgrep, packs smaller binaries than Bun.',
    title: 'Agent-first toolchain',
  },
  {
    id: 'workflows',
    line: 'Concurrency, retries, cancellation, codemode, sandbox. It’s a lot, just read the code.',
    title: 'Workflow primitives',
  },
  {
    id: 'evals',
    line: 'Generate tests from data or prod traces. Grade flaky (AI) functions over many runs.',
    title: 'Better testing',
  },
];

const BORROWS = [
  {
    cls: 'ts',
    icon: '/logos/typescript.svg',
    lang: 'TypeScript',
    traits: ['syntax', 'readability'],
  },
  {
    cls: 'rs',
    icon: '/rust-crab.svg',
    lang: 'Rust',
    traits: ['correctness', 'tooling'],
  },
  {
    cls: 'go',
    icon: '/logos/go.svg',
    lang: 'Go',
    traits: ['compile times', 'concurrency'],
  },
  {
    cls: 'py',
    icon: '/logos/python.svg',
    lang: 'Python',
    plain: true,
    traits: ['dynamism', 'nothing else'],
  },
];

const PLATFORMS = [
  { icon: '/logos/apple.svg', name: 'macOS' },
  { icon: '/logos/linux.svg', name: 'Linux' },
  { icon: '/logos/windows.svg', name: 'Windows' },
];

const HOSTS = [
  { icon: '/logos/python.svg', name: 'Python' },
  { icon: '/logos/nodejs.svg', name: 'TypeScript', tag: 'Node' },
  { icon: '/logos/wasm.svg', name: 'TypeScript', tag: 'Web' },
  { icon: '/logos/java.svg', name: 'Java' },
  { icon: '/logos/csharp.svg', name: 'C#' },
  { icon: '/logos/dotnet.svg', name: '.NET' },
  { icon: '/logos/cplusplus.svg', name: 'C++' },
  { icon: '/logos/go.svg', name: 'Go' },
  { icon: '/logos/rust.svg', name: 'Rust' },
  { icon: '/logos/kotlin.svg', name: 'Kotlin', tag: 'Android' },
  { icon: '/logos/swift.svg', name: 'Swift', tag: 'iOS' },
  { icon: '/logos/php.svg', name: 'PHP', soon: true, tag: 'soon' },
  { icon: '/logos/ruby.svg', name: 'Ruby', soon: true, tag: 'soon' },
];

// A tab is either a plain name or a category with its own sub-tabs (the
// two-row layout: category on top, examples underneath).
type Tab = { name: string; sub?: string[] };

type Section = {
  id: string;
  num: number;
  title: string;
  // Placeholder art for the elephant-in-the-room sections.
  art?: string;
  // A shouted line that reads as a sign, not a sentence. The prose underneath
  // starts fresh rather than trying to flow out of it.
  sign?: string;
  body: string;
  hook?: string;
  // The business section has neither: no explorer, no deep-dive link.
  readMore?: string;
  tabs?: (string | Tab)[];
  // Adoption is a two-axis explorer instead of tabs: pick a BAML feature and a
  // host language, see that one definition cross the bridge.
  matrix?: { features: string[]; languages: string[] };
};

const SECTIONS: Section[] = [
  {
    body: 'Agents write more code than any human can read. Telemetry is the only way to understand what happened. BAML traces every function instead of sampling a percentage. It’s 6x faster than OpenTelemetry in Rust, 200x faster in Python, and the traces 1000x smaller.',
    id: 'observability',
    num: 1,
    readMore: 'How BAML keeps tracing fast enough to always leave on',
    tabs: [
      'Always-on Observability',
      'Data Enrichment',
      'Agents Using Traces',
      'Runs at Scale',
    ],
    title: 'Observability and profiling',
  },
  {
    body: 'Calling a model should feel like calling a function, not wiring up an SDK. In BAML it’s a typed function: declare the input and output types, get structured data back. Malformed output gets repaired against your types by schema-aligned parsing, so every model gets better at structured output.',
    id: 'ai-functions',
    num: 2,
    readMore: 'AI functions in BAML',
    // Flat by design. The variants inside these (Heal JSON's malformed /
    // missing / wrong-type cases, Switch Models' provider list) are toggles
    // that swap one line of the snippet, not sub-tabs.
    tabs: [
      'AI Functions',
      'Heal JSON',
      'Streaming',
      'Switch Models',
      'AI Classes',
    ],
    title: 'AI functions',
  },
  {
    body: 'Every invariant you can’t enforce is one an agent will eventually violate. Start with type erasure: we don’t do it, so there’s no `any` or unchecked cast for a model to hide behind. In BAML, invalid states don’t compile.',
    hook: 'there are no escape hatches for an agent to grab.',
    id: 'language',
    num: 3,
    readMore: 'The BAML language',
    tabs: ['No Any', 'switch < match', 'Typed Errors', 'Local Reasoning'],
    title: 'The anti-slop type system',
  },
  {
    body: 'For decades, tooling was built for humans: LSPs, autocomplete, hover docs. It’s about time the real author of the code got fair treatment. An LSP for you, `baml describe` and friends for them. And a really fast compiler for both.',
    id: 'tooling',
    num: 4,
    readMore: 'The BAML toolchain',
    tabs: [
      'Compile Faster',
      'Build Cheaper',
      'Search Better',
      'Run Directly',
      'Ship Anywhere',
    ],
    title: 'Agent-first toolchain',
  },
  {
    body: 'How has everyone accepted async as a good idea? Half your codebase ends up as two copies of the same function, one sync and one async. Like Go, BAML has green threads. Unlike Go, you can await what they return.',
    id: 'workflows',
    num: 5,
    readMore: 'Workflows in BAML',
    tabs: [
      'No Function Coloring',
      'Cancel Anything',
      'Limit Concurrency',
      'Customize Execution',
    ],
    title: 'Workflow primitives',
  },
  {
    body: 'Engineers spent twenty years squashing flaky tests. Then AI made everything flaky. BAML tests can grade distributions. Cases can be hard-coded examples, golden datasets, or real prod traces.',
    id: 'evals',
    num: 6,
    readMore: 'Evals in BAML',
    tabs: ['Batteries Included', 'Tests from Data', 'Handle Flakiness'],
    title: 'Better testing',
  },
  {
    // {elephant} marks the four elephant-in-the-room sections: will I have to
    // rewrite everything, can models write this, where are the packages, and
    // how do these people make money. The art is a placeholder until the
    // generated lamb/elephant images land.
    art: '/elephants/elephant-adoption.png',
    body: 'You can if you want. But we went the extra mile in the other direction: every type, every function, every method crosses the bridge to your language. Even generics. Even lambdas. Even tracing. Sh{lamb}t just works.',
    id: 'adopting',
    matrix: {
      features: ['Function', 'Type', 'Error', 'Method', 'Generic', 'Lambda'],
      languages: [
        'Python',
        'TypeScript',
        'Go',
        'Rust',
        'Java',
        'C#',
        'C++',
        'Kotlin',
        'Swift',
      ],
    },
    num: 7,
    readMore: 'Incremental adoption',
    sign: 'Do not rewrite your codebase in BAML.',
    title: '{elephant} I’m hyped. How do I port my codebase to BAML?',
  },
  {
    art: '/elephants/elephant-models.png',
    body: 'Decide for yourself. BAML reads like TypeScript and is already an official language on GitHub, so models should know most of it. We don’t leave the rest up to vibes. We measure how agents write programs: how long it takes, what it costs, how many turns. `baml describe` wasn’t a coincidence. It’s a science.\nAnd remember, today is the worst the models will ever be at BAML.',
    id: 'models-write',
    num: 8,
    readMore: 'Agent tries BAML',
    title: '{elephant} Can models actually write BAML?',
  },
  {
    art: '/elephants/elephant-packages.png',
    body: '`npm install is-even`. It depends on `is-odd`? That depends on `is-number`?! 🤯 We’re rethinking package management from first principles. For now, we’ve shipped a thorough standard library. For anything else, ask Claude to port what you need, or pass functions over the bridge.\nBesides, no packages means no supply chain attacks. QED.',
    id: 'packages',
    num: 9,
    title: '{elephant} Is there an ecosystem?',
  },
  {
    art: '/elephants/elephant-money.png',
    body: 'No thank you.\nThe BAML language is and will always be open. Apache-2, free, works offline.\nWe make money on Boundary Web Services. Build the language, the runtime, and the tracing layer, and you can make things nobody else can. We think you’ll love paying for some of them.\nSome of you may wish to build your own cloud. Good luck and Claudespeed.\nFor the rest, we’re launching with observability, and saving you from Datadog. [Reach out](mailto:vbv@boundaryml.com?subject=I%20want%20to%20send%20BAML%20monies) if you want in early.',
    id: 'money',
    num: 10,
    title: '{elephant} Are you trying to get bought by {acquirer}',
  },
];

const CSS = `
.wib {
  /* Very light purple ground, replacing the cream that read as AI-generated. */
  --ink: ${INK}; --muted: ${MUTED}; --faint: #9E9AAB; --border: #DDD8E8; --panel: #F7F5FC; --accent: #6D28D9;
  --mono: var(--font-geist-mono), ui-monospace, SFMono-Regular, Menlo, monospace;
  /* type scale */
  --fs-display: clamp(30px, 4.5vw, 42px);
  --fs-h2: 26px;
  --fs-h3: 20px;
  --fs-lead: clamp(17px, 2vw, 20px);
  --fs-body: 17px;
  --fs-meta: 14px;
  --fs-label: 12px;
  /* spacing scale (4px grid) */
  --sp-1: 4px; --sp-2: 8px; --sp-3: 12px; --sp-4: 16px; --sp-5: 20px;
  --sp-6: 24px; --sp-7: 28px; --sp-8: 32px; --sp-10: 40px; --sp-12: 48px;
  --sp-14: 56px; --sp-16: 64px; --sp-20: 80px; --sp-24: 96px; --sp-32: 128px;
  /* radii */
  --r-xs: 4px; --r-sm: 8px; --r-md: 14px; --r-lg: 16px;
  /* clears the sticky banner + navbar (~105px) with room to breathe, so
     jump links don't land tucked under the header */
  --anchor-offset: 160px;
  margin: 0 auto; max-width: 760px; padding: var(--sp-16) var(--sp-6) var(--sp-32); color: var(--ink);
  font-size: var(--fs-body); line-height: 1.7;
}
.wib a { color: var(--accent); text-decoration: none; }
.wib a:hover { text-decoration: underline; }
.wib-code { background: rgba(0,0,0,0.06); border-radius: var(--r-xs); padding: 1px 5px;
  font-family: var(--mono); font-size: 0.86em; }
.wib-code .tok-kw { color: #7C3AED; }
.wib-code .tok-type { color: #0E7490; }

.wib-hero h1 { font-size: var(--fs-display); font-weight: 640; letter-spacing: -0.02em;
  line-height: 1.06; margin: 0 0 var(--sp-5); }
/* Says what BAML is before anything says how it is different. Everything below
   is contrast, and contrast needs a subject. */
.wib-acro { color: var(--accent); font-weight: 700; }
/* reads as a sign rather than a shouted sentence: its own line, spaced out */
.wib-sign { margin: 0 0 var(--sp-3); font-weight: 700; text-transform: uppercase;
  letter-spacing: 0.06em; color: var(--accent); font-size: var(--fs-body); }
.wib-deck { margin: 0 0 var(--sp-7); font-size: var(--fs-lead); font-weight: 400;
  line-height: 1.5; color: var(--ink); }
.wib-hero p { color: #2b2620; margin: 0 0 var(--sp-4); font-size: var(--fs-lead); line-height: 1.5; }
.wib-borrow-list { margin: var(--sp-3) 0 var(--sp-2); padding: 0; list-style: none; font-size: var(--fs-lead);
  display: flex; flex-direction: column; gap: var(--sp-2); color: var(--ink); }
.wib-borrow-list li { display: flex; align-items: center; }
/* One color per row (--c) drives both the language name and the trait underline. */
.wib-borrow-list li.ts { --c: #3178C6; }
.wib-borrow-list li.rs { --c: #B7410E; }
.wib-borrow-list li.go { --c: #0091AC; }
.wib-borrow-list li.py { --c: #C08A1E; }
.wib-borrow-list .lang { font-weight: 600; color: var(--c); }
/* The underline draws itself in, staggered per row, like the list is being
   annotated. Background-gradient (not text-decoration) so it can animate. */
.wib-borrow-list .trait, .wib-borrow-list .strike {
  background-image: linear-gradient(var(--c), var(--c));
  background-repeat: no-repeat; background-size: 100% 2px; }
.wib-borrow-list .trait { padding-bottom: 3px; background-position: 0 100%; }
/* struck through the middle instead of underlined: it is the one thing we did
   not take from Python. */
.wib-borrow-list .strike { background-position: 0 58%; }
/* One mark at a time: each draw takes 550ms and the next starts 650ms after
   the previous one began, so nothing overlaps. */
.wib-borrow-list li:nth-child(1) .trait { --d: 300ms; }
.wib-borrow-list li:nth-child(2) .trait { --d: 1600ms; }
.wib-borrow-list li:nth-child(3) .trait { --d: 2900ms; }
.wib-borrow-list li:nth-child(4) .trait { --d: 4200ms; }
.wib-borrow-list li .trait:nth-of-type(2) { --stagger: 650ms; }
/* the strikeout lands last, as the punchline */
.wib-borrow-list li:nth-child(4) .strike { --d: 4200ms; --stagger: 650ms; }

@media (prefers-reduced-motion: no-preference) {
  .wib-borrow-list .trait, .wib-borrow-list .strike { background-size: 0% 2px;
    animation: wib-draw 550ms cubic-bezier(0.25, 0.46, 0.45, 0.94) forwards;
    animation-delay: calc(var(--d, 0ms) + var(--stagger, 0ms)); }
}
@keyframes wib-draw { to { background-size: 100% 2px; } }
.wib .baml-mark { color: var(--accent); font-weight: 600; white-space: nowrap; }
.wib .baml-mark img { height: 1em; width: auto; display: inline-block; vertical-align: -0.15em; margin-right: 0.26em; }
/* stands in for a letter, so it sits at x-height with no side spacing */
.wib-inline-lamb { height: 0.72em; width: auto; display: inline-block; vertical-align: -0.02em; }
.wib-borrow-list .ico { display: inline-flex; align-items: center; justify-content: center; width: 2em; height: 1.5em; margin-right: var(--sp-1); flex-shrink: 0; }
.wib-borrow-list .lang-ico { height: 1.3em; width: auto; max-width: 100%; }

/* The a. prefix matters: it matches the specificity of .wib a:hover, which
   would otherwise underline every line of text inside the card on hover. */
a.wib-feature { display: block; margin: 0; padding: var(--sp-5) var(--sp-6);
  border: 1px solid var(--accent); border-radius: var(--r-md);
  background: color-mix(in srgb, var(--accent) 6%, #fff); color: var(--ink);
  text-decoration: none;
  transition: transform 140ms ease, box-shadow 140ms ease; }
a.wib-feature:hover { text-decoration: none; transform: translateY(-2px);
  box-shadow: 0 6px 18px rgba(109,40,217,0.12); }
.wib-feature-t { display: block; font-size: var(--fs-h3); font-weight: 640; letter-spacing: -0.01em; color: var(--accent); }
.wib-feature-l { margin: var(--sp-2) 0 0; font-size: var(--fs-body); line-height: 1.5; color: var(--ink); }
/* row-gap leaves room for the corner badges, which overhang each chip's top. */
.wib-hosts { display: flex; flex-wrap: wrap; column-gap: var(--sp-2); row-gap: var(--sp-4);
  margin: var(--sp-3) 0 var(--sp-3); padding: 0; list-style: none; }
.wib-hosts li { position: relative; display: inline-flex; align-items: center; gap: var(--sp-1);
  text-decoration: none; font-size: var(--fs-meta); font-weight: 550; color: var(--ink);
  background: #fff; border: 1px solid var(--border); border-radius: var(--r-sm);
  padding: var(--sp-1) var(--sp-2); }
.wib-hosts img { width: 1.15em; height: 1.15em; object-fit: contain; }
.wib-hosts .host-tag { position: absolute; top: -0.72em; right: -0.4em;
  font-size: 0.7em; font-weight: 600; line-height: 1.5; letter-spacing: 0.02em;
  color: color-mix(in srgb, var(--accent) 78%, #000);
  background: color-mix(in srgb, var(--accent) 13%, #fff);
  border: 1px solid color-mix(in srgb, var(--accent) 22%, #fff);
  border-radius: 999px; padding: 0 0.5em; white-space: nowrap; }
.wib-hosts-note { margin: calc(-1 * var(--sp-1)) 0 0; font-size: var(--fs-meta); color: var(--muted); }
.wib-hosts li.soon { color: var(--faint); border-color: #EDE7D9; background: transparent; }
.wib-hosts li.soon img { opacity: 0.3; filter: saturate(0.5); }
.wib-hosts li.soon .host-tag { background: #fff; border-color: #E4DCCB; color: var(--faint); }
/* Sits right after the incremental-adoption card, so the ask lands the moment
   the reader learns they don't have to rewrite anything, rather than only at
   the foot of the page. The full unit, with the editor picker, is at #try-baml.
   Width matches the install unit's own 560px so the header lines up with it. */
.wib-try-top { margin: var(--sp-5) 0 0; max-width: 560px; }
.wib-try-h { display: flex; flex-wrap: wrap; align-items: baseline;
  justify-content: space-between; gap: var(--sp-2); margin: 0 0 var(--sp-3); }
.wib-try-h-t { font-family: var(--mono); font-size: var(--fs-label);
  letter-spacing: 0.1em; text-transform: uppercase; color: #a79a80; }
/* the way out for anyone not ready to install yet */
.wib-try-h-alt { font-size: var(--fs-meta); color: var(--muted); }
.wib-lead { margin: var(--sp-8) 0 var(--sp-4); font-size: var(--fs-lead); color: var(--ink); }
.wib-cards { display: grid; grid-template-columns: repeat(2, 1fr); gap: var(--sp-3); margin: 0 0 var(--sp-2); }
a.wib-card { display: block; padding: var(--sp-4) var(--sp-5); border: 1px solid var(--border); border-radius: var(--r-md);
  background: var(--panel); color: var(--ink); transition: transform 140ms ease, box-shadow 140ms ease, border-color 140ms ease; }
a.wib-card:hover { text-decoration: none; border-color: #cdbfa4; transform: translateY(-2px);
  box-shadow: 0 6px 18px rgba(26,22,18,0.07); }
.wib-card-t { font-weight: 640; letter-spacing: -0.01em; }
.wib-card-l { margin: var(--sp-2) 0 0; font-size: var(--fs-meta); line-height: 1.5; color: var(--muted); }

/* ROOM with an elephant sitting inside each O. The O's stay legible so the word
   still reads; the elephants are small enough to sit in the counter. */
.wib-room { margin: var(--sp-20) 0 var(--sp-8); text-align: center; line-height: 1;
  font-size: clamp(48px, 10vw, 88px); font-weight: 600; letter-spacing: 0.04em; }
.wib-room-o { position: relative; display: inline-block; }
.wib-room-eleph { position: absolute; left: 50%; top: 52%; transform: translate(-50%, -50%);
  font-size: 0.36em; line-height: 1; }
/* The word lands first, then the elephants walk in, so the reader reads ROOM and
   then watches what is in it. Gated on .is-seen (set when the word scrolls into
   view) rather than page load, since the rebus sits far down the page. */
@media (prefers-reduced-motion: no-preference) {
  .wib-room.is-armed .wib-room-eleph { opacity: 0; }
  .wib-room.is-seen .wib-room-eleph {
    animation: wib-eleph-in 700ms cubic-bezier(0.34, 1.4, 0.64, 1) forwards;
    animation-delay: var(--d); }
  .wib-room-o:nth-of-type(1) .wib-room-eleph { --d: 500ms; }
  .wib-room-o:nth-of-type(2) .wib-room-eleph { --d: 850ms; }
}
@keyframes wib-eleph-in {
  from { opacity: 0; transform: translate(-180%, -50%) scale(0.55); }
  to { opacity: 1; transform: translate(-50%, -50%) scale(1); }
}
/* Appears on hover, or on its own after a few seconds (which is what covers
   touch, where there is no hover). */
.wib-room-cap { display: block; margin-top: var(--sp-3);
  font-size: var(--fs-label); letter-spacing: 0.12em; text-transform: uppercase;
  color: var(--faint); opacity: 0; transition: opacity 600ms ease; }
.wib-room:hover .wib-room-cap,
.wib-room.is-explained .wib-room-cap { opacity: 1; }
.wib-divider { border: none; border-top: 1px solid #D9D3C4; margin: var(--sp-12) 0 0; }
.wib-divider--big { margin-top: var(--sp-20); }
.wib-section { margin: var(--sp-12) 0 0; scroll-margin-top: var(--anchor-offset); }
.wib-section h2 { font-size: var(--fs-h2); font-weight: 640; letter-spacing: -0.015em; margin: 0 0 var(--sp-2); }
/* body paragraphs: a real gap between them, none after the last */
.wib-section > p { margin: 0 0 var(--sp-4); }
.wib-section > p:last-of-type { margin-bottom: 0; }
/* placeholder art for the elephant sections; swap the images, not the CSS */
.wib-elephant { height: 1.1em; width: auto; display: inline-block;
  vertical-align: -0.12em; margin-right: 0.3em; }
/* cycling acquirer name: the ghost copy holds the width so the line never reflows */
.acq { position: relative; display: inline-grid; vertical-align: baseline; }
.acq-ghost, .acq-name { grid-area: 1 / 1; white-space: nowrap; }
.acq-ghost { visibility: hidden; }
.acq-name { color: var(--accent); transition: opacity 260ms ease, transform 260ms ease; }
.acq-name--out { opacity: 0; transform: translateY(-0.18em); }
@media (prefers-reduced-motion: reduce) {
  .acq-name { transition: none; }
  .acq-name--out { opacity: 1; transform: none; }
}

.wib-art { margin: var(--sp-6) 0 0; border: 1px dashed #c9bfac; border-radius: var(--r-md); background: #fdfbf5;
  padding: var(--sp-5); }
.wib-art-h { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3);
  font-family: var(--mono); font-size: var(--fs-label); letter-spacing: 0.1em;
  text-transform: uppercase; color: #a79a80; margin: 0 0 var(--sp-4); }
.wib-art-tabs { display: flex; flex-wrap: wrap; gap: var(--sp-2); }
.wib-art-tab { font-family: var(--mono); font-size: var(--fs-label); color: var(--muted);
  border: 1px solid var(--border); border-radius: var(--r-sm); padding: var(--sp-1) var(--sp-3); background: #fff; }
.wib-art-tab:first-child { color: var(--ink); border-color: #c2b490; background: var(--panel); }
/* observability: the same event stream side by side. OTEL forgets each
   ripple as it passes; BAML keeps its light. */
.lev { margin: var(--sp-5) 0 0; display: grid;
  grid-template-columns: 1fr 1fr; gap: var(--sp-6); }
@media (max-width: 560px) { .lev { grid-template-columns: 1fr; } }
.lev-col { display: flex; flex-direction: column; align-items: flex-start;
  gap: var(--sp-3); min-width: 0; }
.lev-tag { font-size: 15px; font-weight: 600; }
.lev-tag--otel { color: #B4342B; }
.lev-tag--baml { color: var(--accent); }
/* rectangular field so the two columns align cleanly */
.lev-field { display: grid; grid-template-columns: repeat(18, 1fr); gap: 6px;
  width: 100%; aspect-ratio: 2 / 1; }
.lev-dot { aspect-ratio: 1; border-radius: 50%;
  transition: transform 140ms ease; }
.lev-feed { width: 100%; font-size: 14px; line-height: 2; }
.lev-feed-row { display: flex; justify-content: space-between; gap: 8px;
  animation: lev-feed-in 320ms ease backwards; }
.lev-feed-ev { color: var(--muted); }
.lev-feed-verdict.ok { color: var(--accent); }
.lev-feed-verdict.drop { color: #B4342B; }
@keyframes lev-feed-in { from { opacity: 0; transform: translateY(4px); }
  to { opacity: 1; transform: none; } }
@media (prefers-reduced-motion: reduce) { .lev-feed-row { animation: none; } }
.lev-note { margin: 0; font-size: var(--fs-meta); color: var(--muted);
  text-wrap: balance; }

.wib-footnote { margin: 0 0 var(--sp-4);
  font-size: var(--fs-label); color: var(--muted); font-variant-numeric: tabular-nums; }

.wib-art-subrow { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-2);
  margin: var(--sp-2) 0 0; padding-left: var(--sp-4); }
.wib-art-subrow-k { font-family: var(--mono); font-size: var(--fs-label); color: #a79a80;
  min-width: 7.5em; }
.wib-art-tab--sub { background: transparent; border-style: dashed; }
.wib-art-note { margin: var(--sp-4) 0 0; font-size: var(--fs-meta); color: #a79a80; }
/* adoption matrix: wide, so it scrolls inside itself rather than the page */
.wib-matrix-scroll { overflow-x: auto; }
.wib-matrix { border-collapse: collapse; font-family: var(--mono);
  font-size: var(--fs-label); color: var(--muted); }
.wib-matrix th { font-weight: 500; text-align: left; padding: var(--sp-1) var(--sp-2);
  white-space: nowrap; }
.wib-matrix thead th { color: var(--faint); }
.wib-matrix tbody th { color: var(--ink); padding-right: var(--sp-3); }
.wib-matrix td { padding: 2px; }
.wib-matrix-cell { display: block; width: 100%; height: 18px; min-width: 42px;
  border: 1px dashed var(--border); border-radius: var(--r-xs); background: #fff; }

/* Floating Try BAML. Rides along while you read, steps aside when the real
   install unit is on screen. */
a.wib-fab { position: fixed; right: var(--sp-6); bottom: var(--sp-6); z-index: 40;
  display: inline-flex; align-items: center; gap: var(--sp-2);
  padding: var(--sp-3) var(--sp-5); border-radius: 999px;
  background: var(--accent); color: #fff; text-decoration: none;
  font-size: var(--fs-meta); font-weight: 600;
  box-shadow: 0 6px 20px rgba(109, 40, 217, 0.28);
  opacity: 0; transform: translateY(8px); pointer-events: none;
  transition: opacity 240ms ease, transform 240ms ease; }
a.wib-fab.is-on { opacity: 1; transform: none; pointer-events: auto; }
a.wib-fab:hover { text-decoration: none; transform: translateY(-2px); }
a.wib-fab img { height: 1.1em; width: auto; filter: brightness(0) invert(1); }
@media (max-width: 640px) {
  a.wib-fab { right: var(--sp-4); bottom: var(--sp-4); }
}

.wib-tail { margin-top: var(--sp-12); border-top: 1px solid var(--border); padding-top: var(--sp-12); }
.wib-tail h2 { font-size: var(--fs-h3); font-weight: 640; margin: 0 0 var(--sp-3); }
.wib-try { scroll-margin-top: var(--anchor-offset); }
/* the install unit spans the column so it lines up with the CTA cards */
.wib-try > div:not(.wib-cta-row) { max-width: none; }
.wib-try h2 { margin-top: 0; }
.wib-aside { margin: 0; color: var(--muted); }
.wib-cta-row { display: flex; flex-wrap: wrap; gap: var(--sp-3); margin-top: var(--sp-5); }
.wib-cta-row > * { width: 100%; }

@media (max-width: 640px) {
  .wib-cards { grid-template-columns: 1fr; }
  .wib { padding: var(--sp-12) var(--sp-4) var(--sp-24); }
}
`;

// Section titles can carry two tokens: {elephant} renders the section's art
// (placeholder until the generated lamb/elephant images land) and {acquirer}
// renders the cycling company name.
function renderTitle(s: Section) {
  return s.title.split(/(\{elephant\}|\{acquirer\})/).map((part, i) => {
    if (part === '{elephant}') {
      return s.art ? (
        // biome-ignore lint/suspicious/noArrayIndexKey: positional split
        <img alt="" className="wib-elephant" key={i} src={s.art} />
      ) : null;
    }
    if (part === '{acquirer}') {
      // biome-ignore lint/suspicious/noArrayIndexKey: positional split
      return <Acquirer key={i} />;
    }
    // biome-ignore lint/suspicious/noArrayIndexKey: positional split
    return <span key={i}>{part}</span>;
  });
}

// Adoption's explorer: one axis of BAML features, one of host languages. Click
// a cell to see that definition cross the bridge. Grid shown so the shape of
// the thing is legible before it is interactive.
// biome-ignore lint/correctness/noUnusedVariables: parked until the two-axis explorer returns
function MatrixPlaceholder({
  features,
  languages,
}: NonNullable<Section['matrix']>) {
  return (
    <div className="wib-art">
      <div className="wib-art-h">
        <span>two-axis explorer</span>
        <span>placeholder</span>
      </div>
      <div className="wib-matrix-scroll">
        <table className="wib-matrix">
          <thead>
            <tr>
              <th />
              {languages.map((l) => (
                <th key={l}>{l}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {features.map((f) => (
              <tr key={f}>
                <th scope="row">{f}</th>
                {languages.map((l) => (
                  <td key={l}>
                    <span className="wib-matrix-cell" />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="wib-art-note">
        Pick a feature and a language. One BAML definition on the left, the
        generated interface and usage on the right. Click a cell to zoom in,
        escape to zoom back out.
      </p>
    </div>
  );
}

// biome-ignore lint/correctness/noUnusedVariables: parked until every section has a real explorer
function Placeholder({ tabs }: { tabs: (string | Tab)[] }) {
  const rows = tabs.map((t) => (typeof t === 'string' ? { name: t } : t));
  const nested = rows.filter((t) => t.sub?.length);
  return (
    <div className="wib-art">
      <div className="wib-art-h">
        <span>interactive explorer</span>
        <span>placeholder</span>
      </div>
      <div className="wib-art-tabs">
        {rows.map((t) => (
          <span className="wib-art-tab" key={t.name}>
            {t.name}
          </span>
        ))}
      </div>
      {nested.map((t) => (
        <div className="wib-art-subrow" key={t.name}>
          <span className="wib-art-subrow-k">{t.name}</span>
          {t.sub?.map((s) => (
            <span className="wib-art-tab wib-art-tab--sub" key={s}>
              {s}
            </span>
          ))}
        </div>
      ))}
      <p className="wib-art-note">
        Shared tabbed explorer goes here: header + Run, a swappable body
        (code-run / terminal / graph / diagram / diff), self-scrolling code.
      </p>
    </div>
  );
}

export default function WhatIsBamlPage() {
  return (
    <>
      {/* biome-ignore lint/security/noDangerouslySetInnerHtml: static page CSS */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <Navbar />
      <main className="wib">
        <div className="wib-hero">
          <h1>Modern programming languages weren&rsquo;t built for agents.</h1>
          <p className="wib-deck">
            {inline('BAML is ')}
            {/* the initials spell it out, so they carry the mark's color */}
            <span className="wib-acro">B</span>asically{' '}
            <span className="wib-acro">A</span>{' '}
            <span className="wib-acro">M</span>ade-up{' '}
            <span className="wib-acro">L</span>anguage.
          </p>
          <p>
            We designed it to fight slop. As you read the code, it should feel
            like TypeScript (unions, generics, lambdas), but with no escape
            hatches (like{' '}
            <code className="wib-code">
              <span className="tok-kw">as</span>{' '}
              <span className="tok-type">any</span>
            </code>
            ).
          </p>
          <p>{inline('BAML has the:')}</p>
          <ul className="wib-borrow-list">
            {BORROWS.map((b) => (
              <li className={b.cls} key={b.lang}>
                <span className="ico">
                  <img alt="" className="lang-ico" src={b.icon} />
                </span>
                <span>
                  <span className="trait">{b.traits[0]}</span> and{' '}
                  {/* the Python punchline gets struck out, not underlined */}
                  <span className={b.plain ? 'strike' : 'trait'}>
                    {b.traits[1]}
                  </span>{' '}
                  of <span className="lang">{b.lang}</span>
                </span>
              </li>
            ))}
          </ul>
        </div>

        <p className="wib-lead">The parts different from TypeScript are:</p>

        <div className="wib-cards">
          {CARDS.map((c) => (
            <a className="wib-card" href={`#${c.id}`} key={c.id}>
              <span className="wib-card-t">{c.title}</span>
              <p className="wib-card-l">{inline(c.line)}</p>
            </a>
          ))}
        </div>

        <p className="wib-lead">And, most importantly:</p>

        <a className="wib-feature" href="#adopting">
          <span className="wib-feature-t">Incremental adoption</span>
          <p className="wib-feature-l">{inline('BAML runs standalone on:')}</p>
          <ul className="wib-hosts">
            {PLATFORMS.map((p) => (
              <li key={p.name}>
                <img alt="" src={p.icon} />
                {p.name}
              </li>
            ))}
          </ul>
          <p className="wib-feature-l">or inside your existing projects in:</p>
          <ul className="wib-hosts">
            {HOSTS.map((h) => (
              <li
                className={h.soon ? 'soon' : undefined}
                key={`${h.name}-${h.tag ?? ''}`}
              >
                <img alt="" src={h.icon} />
                {h.name}
                {h.tag ? <span className="host-tag">{h.tag}</span> : null}
              </li>
            ))}
          </ul>
          <p className="wib-hosts-note">
            But not JavaScript. Never JavaScript. Plz stop.
          </p>
          <p className="wib-feature-l">
            Type-safe like OpenAPI, but capable like FFI.
          </p>
        </a>

        <hr className="wib-divider" />

        {SECTIONS.map((s) => (
          <Fragment key={s.id}>
            {/* rebus: the two elephants sit inside the O's */}
            {s.id === 'adopting' ? (
              <>
                <hr className="wib-divider wib-divider--big" />
                <RoomRebus />
              </>
            ) : null}
            <section className="wib-section" id={s.id}>
              <h2>{renderTitle(s)}</h2>
              {s.sign ? <p className="wib-sign">{s.sign}</p> : null}
              {/* a newline in body starts a new paragraph */}
              {s.body.split('\n').map((para, i, paras) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: positional split
                <p key={i}>
                  {inline(para)}
                  {/* trailing read-more flows straight out of the prose */}
                  {!s.tabs?.length && s.readMore && i === paras.length - 1 ? (
                    <>
                      {' '}
                      Read more &rarr; <a href="/techdocs">{s.readMore}</a>
                    </>
                  ) : null}
                </p>
              ))}
              {s.id === 'observability' ? <LostEvents /> : null}
              {s.tabs?.length ? (
                <CodeExplorer readMore={s.readMore} sectionId={s.id} />
              ) : null}
            </section>
          </Fragment>
        ))}

        <div className="wib-tail">
          {/* The anchor wraps the whole block, not just the heading, so jumping
              here lands with the install unit itself in view. */}
          <div className="wib-try" id="try-baml">
            <h2>Try BAML</h2>
            <DroppedFootnote />
            <TryBaml />
            <div className="wib-cta-row">
              <EapCta />
              <DiscordCta />
            </div>
          </div>
        </div>
        <TryBamlFab />
      </main>
      <FooterSection />
    </>
  );
}
