import { createMetadata } from '@/app/_lib/metadata';
import { TryBaml } from '@/app/baml-intro/_components/TryBaml';
import { DiscordCta } from '@/components/discord-cta';
import { EapCta } from '@/components/eap-cta';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

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

// Render inline `code` spans inside a plain string.
function inline(text: string) {
  return text.split(/`([^`]+)`/).map((part, i) =>
    i % 2 === 1 ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: positional split output
      <code key={i} className="wib-code">
        {part}
      </code>
    ) : (
      // biome-ignore lint/suspicious/noArrayIndexKey: positional split output
      <span key={i}>{part}</span>
    ),
  );
}

type Card = { id: string; title: string; line: string };

const CARDS: Card[] = [
  {
    id: 'ai-functions',
    title: 'AI functions',
    line: 'Streaming, batching, websockets, voice, any provider.',
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
  {
    id: 'language',
    title: 'The anti-slop type system',
    line: 'No `any`, no unchecked casts, no imports. But we did add pattern matching.',
  },
];

type Section = {
  id: string;
  num: number;
  title: string;
  body: string;
  hook?: string;
  readMore: string;
  tabs: string[];
};

const SECTIONS: Section[] = [
  {
    id: 'ai-functions',
    num: 1,
    title: 'AI functions',
    body: 'Calling a model should feel like calling a function, not wiring up an SDK. In BAML an LLM call is a typed AI function: declare the inputs and the output type, get structured, validated data back. Everything is built in: streaming, batching, prompt caching, tts/stt, native tool calling, and any provider you want (including a bespoke endpoint).',
    readMore: 'AI functions in BAML',
    tabs: ['typed call', 'streaming', 'tool calling', 'batching', 'swap provider'],
  },
  {
    id: 'observability',
    num: 2,
    title: 'Observability and profiling',
    body: 'Agent systems are non-deterministic, so debugging them relies on first-class observability. Normally that is OpenTelemetry work: threading spans through every call. BAML traces and profiles every function and LLM call automatically. It is all local, and a coding agent can read the traces the same way you can.',
    readMore: 'How BAML keeps tracing fast enough to always leave on',
    tabs: ['traced run', 'profiler', 'agent reads trace'],
  },
  {
    id: 'tooling',
    num: 3,
    title: 'Agent-first toolchain',
    body: 'The toolchain is built for agents. It compiles fast (faster than Go), which keeps agent loops from stalling on the compiler between edits. `baml describe` is ripgrep for your BAML: an agent queries the real shape of the code (definitions, signatures, references) instead of opening every file. `baml run` executes any function or expression directly, so an agent skips the "write-a-script, run-it, delete-it" loop. And `baml pack` bundles a function into a standalone binary for any platform, so you can build a Mac binary from Windows. A packed binary is 14 MB; the same thing with Bun is 64 MB.',
    hook: 'compiles a 295k-line project in `[N]`s, faster than `go build`.',
    readMore: 'The BAML toolchain',
    tabs: ['baml describe', 'baml run', 'baml run -e', 'baml pack'],
  },
  {
    id: 'workflows',
    num: 4,
    title: 'Workflow primitives',
    body: 'Production agents fan out, race, retry, hedge, and cancel. BAML puts that in the language: green-thread concurrency with `spawn`, cascading cancellation, and retries you configure inline. And of course, sandboxes and codemode are natively supported, so models can safely run generated code in their environment. Standard libraries are nice.',
    hook: 'spawn 16 workers, cancel all of them with one timeout.',
    readMore: 'Workflows in BAML',
    tabs: ['concurrency', 'cancellation', 'retries', 'hedge', 'sandbox / codemode'],
  },
  {
    id: 'evals',
    num: 5,
    title: 'Better testing',
    body: 'You need evals to trust an agent, and BAML keeps them in the code, next to the functions and in the same pull request. A testset builds its cases from your data or from a real production trace, so you can turn a bug you hit in prod into a regression test. To score an output, you assert on it or hand it to an LLM judge, which is just another AI function you write. For flaky models, you grade over several runs: `Quorum(5, 3)` passes if three of five pass, and `PassRate(0.7)` succeeds if 70% of tests pass.',
    hook: 'Evals ship in the same PR as your code.',
    readMore: 'Evals in BAML',
    tabs: ['testset from data', 'from prod trace', 'Quorum', 'PassRate', 'LLM judge'],
  },
  {
    id: 'language',
    num: 6,
    title: 'The anti-slop type system',
    body: 'When agents write code, they reach for every escape hatch to get the job done. All of this sits on a language built to prevent slop. Types exist at runtime, so there is no `any` or unchecked cast for a model to hide behind. Invalid states do not compile. It reads like TypeScript, but an agent cannot quietly paper over a mistake and move on.',
    hook: 'there are no escape hatches for an agent to grab.',
    readMore: 'The BAML language',
    tabs: ['no any', 'exhaustive match', 'typed errors', 'invalid state'],
  },
];

const CSS = `
.wib {
  --ink: ${INK}; --muted: ${MUTED}; --border: #D9D3C4; --panel: #FBF8F1; --accent: #6D28D9;
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
.wib-hero .wib-borrow { margin: var(--sp-2) 0 var(--sp-7); color: var(--ink); line-height: 1.65; }
.wib-borrow .lang { font-weight: 600; white-space: nowrap; }
.wib-borrow .lang-ts { color: #3178C6; }
.wib-borrow .lang-rs { color: #B7410E; }
.wib-borrow .lang-go { color: #0091AC; }
.wib-borrow .lang-py { color: #3B72A0; }
.wib-borrow .lang-baml { color: var(--accent); }
.wib .baml-mark { color: var(--accent); font-weight: 600; white-space: nowrap; }
.wib .baml-mark img { height: 1em; width: auto; display: inline-block; vertical-align: -0.15em; margin-right: 0.26em; }
.wib-borrow .lang-ico { height: 1em; width: auto; display: inline-block; vertical-align: -0.15em; margin-right: 0.26em; }

.wib-feature { display: block; margin: 0; padding: var(--sp-5) var(--sp-6);
  border: 1px solid var(--accent); border-radius: var(--r-md);
  background: color-mix(in srgb, var(--accent) 6%, #fff); color: var(--ink);
  transition: transform 140ms ease, box-shadow 140ms ease; }
.wib-feature:hover { text-decoration: none; transform: translateY(-2px); box-shadow: 0 6px 18px rgba(109,40,217,0.12); }
.wib-feature-t { display: block; font-size: var(--fs-h3); font-weight: 640; letter-spacing: -0.01em; color: var(--accent); }
.wib-feature-l { margin: var(--sp-2) 0 0; font-size: var(--fs-body); line-height: 1.5; color: var(--ink); }
.wib-lead { margin: var(--sp-8) 0 var(--sp-4); font-size: var(--fs-lead); color: var(--ink); }
.wib-cards { display: grid; grid-template-columns: repeat(2, 1fr); gap: var(--sp-3); margin: 0 0 var(--sp-2); }
a.wib-card { display: block; padding: var(--sp-4) var(--sp-5); border: 1px solid var(--border); border-radius: var(--r-md);
  background: var(--panel); color: var(--ink); transition: transform 140ms ease, box-shadow 140ms ease, border-color 140ms ease; }
a.wib-card:hover { text-decoration: none; border-color: #cdbfa4; transform: translateY(-2px);
  box-shadow: 0 6px 18px rgba(26,22,18,0.07); }
.wib-card-t { font-weight: 640; letter-spacing: -0.01em; }
.wib-card-l { margin: var(--sp-2) 0 0; font-size: var(--fs-meta); line-height: 1.5; color: var(--muted); }

.wib-section { margin: var(--sp-16) 0 0; scroll-margin-top: 90px; }
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
.wib-art-note { margin: var(--sp-4) 0 0; font-size: var(--fs-meta); color: #a79a80; }

.wib-install { margin: var(--sp-14) 0; padding: var(--sp-6); border: 1px solid var(--border); border-radius: var(--r-lg); background: var(--panel); }
.wib-install-line { margin: var(--sp-4) 0 0; font-size: var(--fs-meta); color: var(--muted); }

.wib-tail { margin-top: var(--sp-20); border-top: 1px solid var(--border); padding-top: var(--sp-10); }
.wib-tail h2 { font-size: var(--fs-h3); font-weight: 640; margin: var(--sp-10) 0 var(--sp-3); scroll-margin-top: 90px; }
.wib-cta-row { display: flex; flex-wrap: wrap; gap: var(--sp-3); margin-top: var(--sp-5); }

@media (max-width: 640px) {
  .wib-cards { grid-template-columns: 1fr; }
  .wib { padding: var(--sp-12) var(--sp-4) var(--sp-24); }
}
`;

function Placeholder({ tabs }: { tabs: string[] }) {
  return (
    <div className="wib-art">
      <div className="wib-art-h">
        <span>interactive explorer</span>
        <span>placeholder</span>
      </div>
      <div className="wib-art-tabs">
        {tabs.map((t) => (
          <span className="wib-art-tab" key={t}>
            {t}
          </span>
        ))}
      </div>
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
            lambdas).
          </p>
          <p className="wib-borrow">
            <span className="lang lang-baml">
              <img className="lang-ico" src="/bamllogopurple.svg" alt="" />
              BAML
            </span>{' '}
            has the syntax of{' '}
            <span className="lang lang-ts">
              <img className="lang-ico" src="/logos/typescript.svg" alt="" />
              TypeScript
            </span>
            , the correctness of{' '}
            <span className="lang lang-rs">
              <img className="lang-ico" src="/rust-crab.svg" alt="" />
              Rust
            </span>
            , the compile times of{' '}
            <span className="lang lang-go">
              <img className="lang-ico" src="/logos/go.svg" alt="" />
              Go
            </span>
            , and the dynamism of{' '}
            <span className="lang lang-py">
              <img className="lang-ico" src="/logos/python.svg" alt="" />
              Python
            </span>
            .
          </p>
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
          <span className="wib-feature-t">Incrementally adoptable</span>
          <p className="wib-feature-l">
            Run BAML standalone on any platform, or embedded from within Python,
            TypeScript, Go, Rust, Java, C++, or Swift.
          </p>
        </a>

        {SECTIONS.map((s, i) => (
          <section className="wib-section" id={s.id} key={s.id}>
            <h2>{s.title}</h2>
            <p>{inline(s.body)}</p>
            <Placeholder tabs={s.tabs} />
            <p className="wib-more">
              Read more &rarr; <a href="#">{s.readMore}</a>
            </p>

            {/* Install checkpoint partway down the page */}
            {i === 1 || i === 3 ? (
              <div className="wib-install">
                <TryBaml compact />
                <p className="wib-install-line">
                  Install in one command, run your first function in a minute.{' '}
                  <a href="/quickstart">Full quickstart &rarr;</a>
                </p>
              </div>
            ) : null}
          </section>
        ))}

        <div className="wib-tail">
          <h2 id="adopting">Adopting BAML</h2>
          <p>
            BAML drops into a stack you already have. Generate a Python or
            TypeScript SDK, adopt one function at a time, keep the rest of your
            code as is. No rewrite, no lock-in.
          </p>
          <p className="wib-more">
            Read more &rarr; <a href="#">Adopting BAML</a>
          </p>

          <h2>Try BAML</h2>
          <TryBaml />
          <div className="wib-cta-row">
            <EapCta />
            <DiscordCta />
          </div>
        </div>
      </main>
      <FooterSection />
    </>
  );
}
