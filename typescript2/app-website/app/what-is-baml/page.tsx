import { Fragment } from 'react';
import { createMetadata } from '@/app/_lib/metadata';
import { TryBaml } from '@/app/baml-intro/_components/TryBaml';
import { DiscordCta } from '@/components/discord-cta';
import { EapCta } from '@/components/eap-cta';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { DroppedFootnote } from './_components/dropped-footnote';
import { LostEvents } from './_components/lost-events';
import { RoomRebus } from './_components/room-rebus';
import { TryBamlFab } from './_components/try-baml-fab';

// Staging page for the /explore redesign. Copy is the single source of truth in
// EXPLORE_COPY_DRAFT.md. Each section's interactive "explorer" artifact is a
// placeholder for now (the shared tabbed-explorer shell comes next).

export const metadata = createMetadata({
  description:
    "Why agents need a new language, and the parts of BAML that are different: AI functions, observability, agent-first tooling, workflows, evals, and an anti-slop base language.",
  ogTitle: 'What is BAML',
  path: '/what-is-baml',
  title: 'What is BAML',
});

const INK = '#1A1612';
const MUTED = '#5C5852';

// {lamb} renders the BAML mark inline, standing in for a letter.
function withLamb(text: string, prefix: number) {
  const chunks = text.split('{lamb}');
  return chunks.map((chunk, i) => (
    // biome-ignore lint/suspicious/noArrayIndexKey: positional split output
    <span key={`${prefix}-${i}`}>
      {chunk}
      {i < chunks.length - 1 ? (
        <img alt="" className="wib-inline-lamb" src="/bamllogopurple.svg" />
      ) : null}
    </span>
  ));
}

// Body copy supports `code` spans, [label](href) links, and {lamb}.
function inline(text: string) {
  const parts = text.split(/(`[^`]+`|\[[^\]]+\]\([^)]+\))/);
  return parts.map((part, i) => {
    if (part.startsWith('`') && part.endsWith('`')) {
      return (
        // biome-ignore lint/suspicious/noArrayIndexKey: positional split output
        <code key={i} className="wib-code">
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
    id: 'ai-functions',
    title: 'AI functions',
    line: 'Streaming, batching, websockets, voice, any provider.',
  },
  {
    id: 'language',
    title: 'The anti-slop type system',
    line: 'No `any`, no unchecked casts, no imports. But we did add pattern matching.',
  },
  {
    id: 'observability',
    title: 'Observability and profiling',
    line: 'Every function traced and profiled, locally. Agents can read the traces.',
  },
  {
    id: 'tooling',
    title: 'Agent-first toolchain',
    line: 'Compiles faster than Go, searches better than ripgrep, packs smaller binaries than Bun.',
  },
  {
    id: 'workflows',
    title: 'Workflow primitives',
    line: 'Concurrency, retries, cancellation, codemode, sandbox. It’s a lot, just read the code.',
  },
  {
    id: 'evals',
    title: 'Better testing',
    line: 'Generate tests from data or prod traces. Grade flaky (AI) functions over many runs.',
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
  { icon: '/logos/wasm.svg', name: 'TypeScript', tag: 'WASM' },
  { icon: '/logos/java.svg', name: 'Java' },
  { icon: '/logos/csharp.svg', name: 'C#' },
  { icon: '/logos/dotnet.svg', name: '.NET' },
  { icon: '/logos/cplusplus.svg', name: 'C++' },
  { icon: '/logos/go.svg', name: 'Go' },
  { icon: '/logos/rust.svg', name: 'Rust' },
  { icon: '/logos/kotlin.svg', name: 'Kotlin', tag: 'Android' },
  { icon: '/logos/swift.svg', name: 'Swift', tag: 'iOS' },
  { icon: '/logos/php.svg', name: 'PHP', tag: 'soon', soon: true },
  { icon: '/logos/ruby.svg', name: 'Ruby', tag: 'soon', soon: true },
];

// A tab is either a plain name or a category with its own sub-tabs (the
// two-row layout: category on top, examples underneath).
type Tab = { name: string; sub?: string[] };

type Section = {
  id: string;
  num: number;
  title: string;
  body: string;
  hook?: string;
  // The business section has neither: no explorer, no deep-dive link.
  readMore?: string;
  tabs?: (string | Tab)[];
};

const SECTIONS: Section[] = [
  {
    id: 'ai-functions',
    num: 1,
    title: 'AI functions',
    body: 'Calling a model should feel like calling a function, not wiring up an SDK. In BAML it’s a typed function: declare the input and output types, get structured data back. Malformed output gets repaired against your types by schema-aligned parsing, so every model gets better at structured output.',
    readMore: 'AI functions in BAML',
    tabs: [
      'Typed call',
      {
        name: 'Self-healing',
        sub: ['malformed JSON', 'missing field', 'wrong type'],
      },
      { name: 'Streaming', sub: ['partial objects', 'tokens'] },
      { name: 'Reliability', sub: ['swap models', 'retries', 'fallbacks'] },
      { name: 'Agents', sub: ['tool calling', 'voice'] },
    ],
  },
  {
    id: 'language',
    num: 2,
    title: 'The anti-slop type system',
    body: 'Every invariant you can’t enforce is one an agent will eventually violate. Start with type erasure: we don’t do it, so there’s no `any` or unchecked cast for a model to hide behind. In BAML, invalid states don’t compile.',
    hook: 'there are no escape hatches for an agent to grab.',
    readMore: 'The BAML language',
    tabs: ['no any', 'exhaustive match', 'typed errors', 'invalid state'],
  },
  {
    id: 'observability',
    num: 3,
    title: 'Observability and profiling',
    body: 'Observability only pays off in hindsight: if you knew what to trace, you’d have traced it already. BAML traces every function by default. 6x faster than OTEL in Rust, 200x faster than in Python, and writes traces 1000x smaller, which is why it can stay on, even in prod.',
    readMore: 'How BAML keeps tracing fast enough to always leave on',
    tabs: ['traced run', 'profiler', 'agent reads trace'],
  },
  {
    id: 'tooling',
    num: 4,
    title: 'Agent-first toolchain',
    body: 'For ten years, tooling was built for humans: LSPs, autocomplete, breakpoints, hover docs. It’s about time the real author of the code got fair treatment. An LSP for you, `baml describe` and friends for them. And a really fast compiler for both.',
    readMore: 'The BAML toolchain',
    tabs: ['baml describe', 'baml run', 'baml run -e', 'baml pack'],
  },
  {
    id: 'workflows',
    num: 4,
    title: 'Workflow primitives',
    body: 'How has everyone accepted async as a good idea? Every agent framework is trying to do concurrency on top of languages that lack it. Like Go, BAML has green threads. Unlike Go, you can await what they return.',
    readMore: 'Workflows in BAML',
    tabs: ['concurrency', 'cancellation', 'retries', 'why not async'],
  },
  {
    id: 'evals',
    num: 5,
    title: 'Better testing',
    body: 'Engineers spent twenty years squashing flaky tests. Then models made every test flaky by definition. BAML tests can grade distributions. Cases can be hard-coded examples, golden datasets, or real prod traces. Yesterday’s outage becomes today’s regression test.',
    readMore: 'Evals in BAML',
    tabs: ['testset from data', 'from prod trace', 'Quorum', 'PassRate', 'LLM judge'],
  },
  {
    id: 'adopting',
    num: 6,
    // The 🐘 marks the two elephant-in-the-room sections: will I have to
    // rewrite everything, and how do these people make money.
    title: '🐘 Incremental adoption',
    body: 'We’re not going to pretend you should rewrite your codebase in BAML. That’s how working systems become broken ones. You can write a whole app in BAML if you want. But we went the extra mile in the other direction: every type, every function, every method crosses the bridge to your language. Even generics. Even lambdas. Sh{lamb}t just works.',
    readMore: 'Incremental adoption',
    tabs: ['call from Python', 'call from TypeScript', 'pass a lambda', 'generics'],
  },
  {
    id: 'money',
    num: 7,
    title: '🐘 Making money?',
    body: 'Yes please. BAML is and always will be open: Apache-2, free, no internet required. The Boundary Cloud starts with observability, but when you create the language, the runtime, and the tracing layer, you can build things nobody else can. And we think you’ll love paying for some of them. The language took two years to build, the cloud needs about three more months. [Reach out](mailto:vbv@boundaryml.com?subject=I%20want%20to%20send%20BAML%20monies) if you want in early.',
  },
];

const CSS = `
.wib {
  --ink: ${INK}; --muted: ${MUTED}; --faint: #A79E90; --border: #D9D3C4; --panel: #FBF8F1; --accent: #6D28D9;
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
  line-height: 1.06; margin: 0 0 var(--sp-7); }
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
.wib-room { margin: var(--sp-24) 0 var(--sp-8); text-align: center; line-height: 1;
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
.wib-room-cap { display: block; margin-top: var(--sp-3); font-family: var(--mono);
  font-size: var(--fs-label); letter-spacing: 0.12em; text-transform: uppercase;
  color: var(--faint); opacity: 0; transition: opacity 600ms ease; }
.wib-room:hover .wib-room-cap,
.wib-room.is-explained .wib-room-cap { opacity: 1; }
.wib-section { margin: var(--sp-16) 0 0; scroll-margin-top: var(--anchor-offset); }
.wib-section h2 { font-size: var(--fs-h2); font-weight: 640; letter-spacing: -0.015em; margin: 0 0 var(--sp-4); }
.wib-more { margin: var(--sp-4) 0 0; font-size: var(--fs-meta); }

.wib-art { margin: var(--sp-6) 0 0; border: 1px dashed #c9bfac; border-radius: var(--r-md); background: #fdfbf5;
  padding: var(--sp-5); }
.wib-art-h { display: flex; align-items: center; justify-content: space-between; gap: var(--sp-3);
  font-family: var(--mono); font-size: var(--fs-label); letter-spacing: 0.1em;
  text-transform: uppercase; color: #a79a80; margin: 0 0 var(--sp-4); }
.wib-art-tabs { display: flex; flex-wrap: wrap; gap: var(--sp-2); }
.wib-art-tab { font-family: var(--mono); font-size: var(--fs-label); color: var(--muted);
  border: 1px solid var(--border); border-radius: var(--r-sm); padding: var(--sp-1) var(--sp-3); background: #fff; }
.wib-art-tab:first-child { color: var(--ink); border-color: #c2b490; background: var(--panel); }
/* falling trace events for the observability section */
.lev { margin: var(--sp-4) 0 0; }
.lev-stage { position: relative; height: 132px; overflow: hidden; }
.lev-dot { position: absolute; top: -24px; white-space: nowrap; cursor: default;
  font-family: var(--mono); font-size: 11px; line-height: 1;
  padding: 4px 9px; border-radius: 999px;
  color: #B4342B; background: #FDF0EE; border: 1px solid #F0C8C2;
  transition: color 240ms ease, background 240ms ease, border-color 240ms ease,
    box-shadow 140ms ease; }
.lev-dot--on { color: var(--accent); background: #F3EEFE; border-color: #DACCF7; }
/* never instrumented: amber, and dashed because it never existed as data */
.lev-dot--ghost { color: #8A5A00; background: #FDF3DC; border-color: #E3C97F;
  border-style: dashed; }
/* hover: hold the event still so you can read it. On a dropped event, that
   reveals nothing, which is the point. */
.lev-dot:hover { animation-play-state: paused; z-index: 2;
  box-shadow: 0 2px 10px rgba(26,22,18,0.14); border-color: currentColor; }
.lev-alt { display: none; }
.lev-dot:hover .lev-face { display: none; }
.lev-dot:hover .lev-alt { display: inline; }

@media (prefers-reduced-motion: no-preference) {
  .lev-dot { opacity: 0; animation: lev-fall 11s linear infinite; }
}
@media (prefers-reduced-motion: reduce) {
  .lev-dot { position: static; display: inline-block; margin: 0 6px 6px 0; }
  .lev-stage { height: auto; }
}
@keyframes lev-fall {
  0% { transform: translateY(0); opacity: 0; }
  10% { opacity: 1; }
  82% { opacity: 1; }
  100% { transform: translateY(160px); opacity: 0; }
}

.lev-bar { display: flex; align-items: center; gap: var(--sp-3); min-height: 34px; }
.lev-seg { display: inline-flex; flex-shrink: 0; border: 1px solid var(--border);
  border-radius: var(--r-sm); overflow: hidden; background: #fff; }
.lev-segbtn { font-family: var(--mono); font-size: var(--fs-label); color: var(--muted);
  background: transparent; border: 0; padding: var(--sp-1) var(--sp-3); cursor: pointer;
  min-width: 4.6em; transition: background 140ms ease, color 140ms ease; }
.lev-segbtn--on { font-weight: 600; }
.lev-segbtn--otel.lev-segbtn--on { background: #FDF0EE; color: #B4342B; }
.lev-segbtn--baml.lev-segbtn--on { background: #F3EEFE; color: var(--accent); }
.lev-note { margin: 0; font-size: var(--fs-meta); color: var(--muted); }
.wib-footnote { margin: 0 0 var(--sp-4); font-family: var(--mono);
  font-size: var(--fs-label); color: var(--muted); font-variant-numeric: tabular-nums; }

.wib-art-subrow { display: flex; flex-wrap: wrap; align-items: center; gap: var(--sp-2);
  margin: var(--sp-2) 0 0; padding-left: var(--sp-4); }
.wib-art-subrow-k { font-family: var(--mono); font-size: var(--fs-label); color: #a79a80;
  min-width: 7.5em; }
.wib-art-tab--sub { background: transparent; border-style: dashed; }
.wib-art-note { margin: var(--sp-4) 0 0; font-size: var(--fs-meta); color: #a79a80; }

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
a.wib-fab img { height: 1.1em; width: auto; }
@media (max-width: 640px) {
  a.wib-fab { right: var(--sp-4); bottom: var(--sp-4); }
}

.wib-tail { margin-top: var(--sp-20); border-top: 1px solid var(--border); padding-top: var(--sp-10); }
.wib-tail h2 { font-size: var(--fs-h3); font-weight: 640; margin: var(--sp-10) 0 var(--sp-3); }
.wib-try { scroll-margin-top: var(--anchor-offset); }
.wib-try h2 { margin-top: 0; }
.wib-aside { margin: 0; color: var(--muted); }
.wib-cta-row { display: flex; flex-wrap: wrap; gap: var(--sp-3); margin-top: var(--sp-5); }

@media (max-width: 640px) {
  .wib-cards { grid-template-columns: 1fr; }
  .wib { padding: var(--sp-12) var(--sp-4) var(--sp-24); }
}
`;

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
      {/* eslint-disable-next-line react/no-danger */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <Navbar />
      <main className="wib">
        <div className="wib-hero">
          <h1>Modern programming languages weren&rsquo;t built for agents.</h1>
          <p>
            We built{' '}
            <span className="baml-mark">
              <img src="/bamllogopurple.svg" alt="" />
              BAML
            </span>{' '}
            to fight slop. That means no escape hatches (like{' '}
            <code className="wib-code">
              <span className="tok-kw">as</span>{' '}
              <span className="tok-type">any</span>
            </code>
            ). Almost everything else feels like TypeScript (unions, generics,
            lambdas). BAML has the:
          </p>
          <ul className="wib-borrow-list">
            {BORROWS.map((b) => (
              <li className={b.cls} key={b.lang}>
                <span className="ico">
                  <img className="lang-ico" src={b.icon} alt="" />
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
          <p className="wib-feature-l">BAML runs standalone on:</p>
          <ul className="wib-hosts">
            {PLATFORMS.map((p) => (
              <li key={p.name}>
                <img src={p.icon} alt="" />
                {p.name}
              </li>
            ))}
          </ul>
          <p className="wib-feature-l">
            or inside your existing projects in:
          </p>
          <ul className="wib-hosts">
            {HOSTS.map((h) => (
              <li
                className={h.soon ? 'soon' : undefined}
                key={`${h.name}-${h.tag ?? ''}`}
              >
                <img src={h.icon} alt="" />
                {h.name}
                {h.tag ? <span className="host-tag">{h.tag}</span> : null}
              </li>
            ))}
          </ul>
          <p className="wib-hosts-note">
            But not JavaScript. We only support languages that took more than 10
            days to build.
          </p>
          <p className="wib-feature-l">
            Type-safe like OpenAPI, but performant like FFI.
          </p>
        </a>

        {SECTIONS.map((s) => (
          <Fragment key={s.id}>
            {/* rebus: the two elephants sit inside the O's */}
            {s.id === 'adopting' ? <RoomRebus /> : null}
            <section className="wib-section" id={s.id}>
            <h2>{s.title}</h2>
            <p>{inline(s.body)}</p>
            {s.id === 'observability' ? <LostEvents /> : null}
            {s.tabs?.length ? <Placeholder tabs={s.tabs} /> : null}
            {s.readMore ? (
              <p className="wib-more">
                Read more &rarr; <a href="#">{s.readMore}</a>
              </p>
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
