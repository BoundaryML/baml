'use client';

import Image from 'next/image';
import Link from 'next/link';
import { type ReactNode, useCallback, useState } from 'react';
import { Navbar } from '@/components/navbar';
import {
  type CodeThemeName,
  CODE_THEMES,
  CodeThemeProvider,
} from '../../learn2/_lib/code-theme';
import { BamlCode } from '../../learn2/_components/BamlCode';
import BamlEditor from '../../learn2/_components/BamlEditorLazy';
import LivePlayground from '../../learn2/_components/LivePlaygroundLazy';
import { InfectionGraph } from '../../learn3/_components/InfectionGraph';
// import { MetricsDag } from '../../learn3/_components/MetricsDag'; // metrics section hidden
import { TermPlay } from '../../learn3/_components/TermPlay';
import { SdkPipeline } from '../../learn4/_components/SdkPipeline';
import { DesignGoalsCard } from './DesignGoalsCard';
import { PackChart, SpawnChart } from './PackChart';
import { Scheduler } from './Scheduler';
import { SdkExplorer } from './SdkExplorer';
import { SelfImprove } from './SelfImprove';
import {
  BAML_CSV_TESTS,
  BAML_HTTP_TESTS,
  BAML_IMAGE,
  BAML_MATCH,
  // BAML_METRIC, // metrics section hidden
  BAML_PACKED,
  BAML_RUNNER,
  BAML_SPAWN,
  BAML_UNKNOWN,
  BAML_UNREACHABLE,
  BAML_WF_FANOUT,
  BAML_WF_TALLY,
  BENCH_BAML,
  DESCRIBE_EVENTS,
  GREP_EVENTS,
  LS_EVENTS,
  NS_BAD,
  NS_GOOD,
  PACK_EVENTS,
  RUN_E_EVENTS,
  RUN_FN_EVENTS,
  TS_CATCH,
  TS_INSTANCEOF,
  TS_LIES,
} from './snippets';

/* All sections share one reading-column width so every header aligns.
 * Editors, playgrounds, and side-by-side pairs break out wider via
 * .l6-pair / .l6-breakout — text never does. */
function Section({
  id,
  num,
  title,
  children,
}: {
  id: string;
  num?: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="l6-section" id={id}>
      <h2>
        {num ? <span className="l6-num font-mono">{num}</span> : null}
        {title}
      </h2>
      {children}
    </section>
  );
}

function Sub({
  id,
  num,
  title,
  children,
}: {
  id?: string;
  num?: string;
  title: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="l6-sub" id={id}>
      <h3>
        {num ? <span className="l6-num font-mono">{num}</span> : null}
        {title}
      </h3>
      {children}
    </div>
  );
}

/* The two top-level divisions: "BAML for AI workflows" and "BAML for AI
 * Agents". A part title is bigger than the numbered subsection headers and
 * carries an eyebrow + a divider rule, so each half of the page announces
 * itself as its own section. */
function Part({
  id,
  eyebrow,
  title,
  children,
}: {
  id: string;
  eyebrow?: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="l6-section l6-part" id={id}>
      {eyebrow ? <p className="l6-part-eyebrow font-mono">{eyebrow}</p> : null}
      <h2 className="l6-part-title">{title}</h2>
      {children}
    </section>
  );
}

/* "Try it out!" install unit — tabs, command, and copy button in one
 * editor-like frame. Humans get brew, agents get the plugin commands
 * (mirrors the homepage hero's install paths). */
const TRY_TABS = [
  {
    id: 'humans',
    label: 'for humans',
    lines: ['brew install boundaryml/tap/baml'],
    prompt: '$ ',
  },
  {
    id: 'agents',
    label: 'for agents',
    lines: [
      '/plugin marketplace add BoundaryML/baml-skill',
      '/plugin install baml@boundaryml-baml',
    ],
    prompt: '',
  },
] as const;

function TryItTabs() {
  const [tab, setTab] = useState<'humans' | 'agents'>('humans');
  const [copied, setCopied] = useState(false);
  const active = TRY_TABS.find((t) => t.id === tab) ?? TRY_TABS[0];
  return (
    <div className="l6-block">
      <div className="l6-try font-mono">
        <div aria-label="Install path" className="l6-try-head" role="tablist">
          {TRY_TABS.map((t) => (
            <button
              aria-selected={tab === t.id}
              className={`l6-try-tab font-mono${tab === t.id ? ' l6-try-tab--on' : ''}`}
              key={t.id}
              onClick={() => {
                setTab(t.id);
                setCopied(false);
              }}
              role="tab"
              type="button"
            >
              {t.label}
            </button>
          ))}
          <button
            className="l6-try-copy font-mono"
            onClick={() => {
              navigator.clipboard.writeText(active.lines.join('\n'));
              setCopied(true);
              setTimeout(() => setCopied(false), 1600);
            }}
            type="button"
          >
            {copied ? 'copied!' : 'copy'}
          </button>
        </div>
        <div className="l6-try-body">
          {active.lines.map((line) => (
            <div key={line}>
              {active.prompt ? (
                <span className="l6-try-prompt">{active.prompt}</span>
              ) : null}
              {line}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/* "On this page" rail (top right on wide screens). */
const TOC: { id: string; label: string; sub?: boolean }[] = [
  { id: 'philosophy', label: 'Design philosophy' },
  { id: 'language', label: 'Part 1 · A better language' },
  { id: 'types', label: '1 · Type system', sub: true },
  { id: 'match', label: '2 · Match', sub: true },
  { id: 'error-handling', label: '3 · Error handling', sub: true },
  { id: 'threads', label: '4 · Green threads', sub: true },
  { id: 'agent-tools', label: 'Part 2 · Tools for agents' },
  { id: 'namespaces', label: '1 · Namespaces', sub: true },
  { id: 'describe', label: '2 · baml describe', sub: true },
  { id: 'run-fn', label: '3 · baml run <fn>', sub: true },
  { id: 'run-e', label: '4 · baml run -e', sub: true },
  { id: 'pack', label: '5 · baml pack', sub: true },
  { id: 'human-tools', label: 'Part 3 · Tools for humans' },
  { id: 'nav-viz', label: '1 · Navigation', sub: true },
  { id: 'observability', label: '2 · Observability', sub: true },
  { id: 'usable', label: 'Part 4 · Making BAML usable' },
  { id: 'adoption', label: '1 · Incremental adoption', sub: true },
  { id: 'self-improvement', label: '2 · Self-improvement', sub: true },
  { id: 'supply-chain', label: '3 · Supply chain', sub: true },
  { id: 'agents', label: 'Part 5 · Building agents' },
  { id: 'llm-functions', label: '1 · LLM functions', sub: true },
  { id: 'claude-code', label: '2 · Claude APIs', sub: true },
  { id: 'testing', label: '3 · Tests', sub: true },
  { id: 'eval', label: '4 · eval / codemode', sub: true },
  { id: 'close', label: 'Try it out!' },
];

/* BAML Tests examples — one at a time, switched with buttons, at text
 * width. The CSV one is a live editor; the S3 one is read-only. */
function TestExampleTabs() {
  const [tab, setTab] = useState<'csv' | 's3'>('csv');
  return (
    <div className="l6-block">
      <div aria-label="Test source" className="l6-sdk-tabs" role="tablist">
        <button
          aria-selected={tab === 'csv'}
          className={`l6-sdk-tab font-mono${tab === 'csv' ? ' l6-sdk-tab--on' : ''}`}
          onClick={() => setTab('csv')}
          role="tab"
          type="button"
        >
          from a CSV · run it here
        </button>
        <button
          aria-selected={tab === 's3'}
          className={`l6-sdk-tab font-mono${tab === 's3' ? ' l6-sdk-tab--on' : ''}`}
          onClick={() => setTab('s3')}
          role="tab"
          type="button"
        >
          from S3, at collection time
        </button>
      </div>
      {tab === 'csv' ? (
        <BamlEditor filename="csv_tests.baml" initialCode={BAML_CSV_TESTS} />
      ) : (
        <BamlCode code={BAML_HTTP_TESTS} filename="golden_tests.baml" />
      )}
    </div>
  );
}

/* Native LLM Functions playground — a button row switches the editor +
 * runtime between workflow examples. `illustrate` (the LLM image pipeline) is
 * the default; the others are non-LLM workflows you can actually run. Each
 * example highlights its entry function's lines. The LivePlayground is
 * remounted on switch (Monaco is uncontrolled, so new code only loads via a
 * key change). */
const WORKFLOW_EXAMPLES = [
  {
    code: BAML_IMAGE,
    filename: 'pipeline.baml',
    fn: 'illustrate',
    from: 2,
    id: 'illustrate',
    label: 'illustrate · LLM image pipeline',
    to: 5,
  },
  {
    code: BAML_WF_TALLY,
    filename: 'tally.baml',
    fn: 'summarize',
    from: 2,
    id: 'tally',
    label: 'summarize · tally (runnable)',
    to: 18,
  },
  {
    code: BAML_WF_FANOUT,
    filename: 'fanout.baml',
    fn: 'analyze',
    from: 2,
    id: 'fanout',
    label: 'analyze · parallel fan-out (runnable)',
    to: 9,
  },
] as const;

function lineRange(from: number, to: number): number[] {
  return Array.from({ length: to - from + 1 }, (_, i) => from + i);
}

type WorkflowExampleId = (typeof WORKFLOW_EXAMPLES)[number]['id'];

function WorkflowPlayground() {
  const [id, setId] = useState<WorkflowExampleId>(WORKFLOW_EXAMPLES[0].id);
  const ex = WORKFLOW_EXAMPLES.find((e) => e.id === id) ?? WORKFLOW_EXAMPLES[0];
  return (
    <div className="l6-breakout l6-breakout--xl">
      <div aria-label="Workflow example" className="l6-wf-tabs" role="tablist">
        {WORKFLOW_EXAMPLES.map((e) => (
          <button
            aria-selected={id === e.id}
            className={`l6-wf-tab font-mono${id === e.id ? ' l6-wf-tab--on' : ''}`}
            key={e.id}
            onClick={() => setId(e.id)}
            role="tab"
            type="button"
          >
            {e.label}
          </button>
        ))}
      </div>
      {/* Switching remounts the playground; LivePlayground veils only its graph
          pane (not the editor) until the new graph is ready. */}
      <LivePlayground
        filename={ex.filename}
        highlightLines={lineRange(ex.from, ex.to)}
        initialCode={ex.code}
        initialFunction={ex.fn}
        initialSidebarOpen={false}
        key={ex.id}
        loadingLabel={`Loading ${ex.label}…`}
      />
    </div>
  );
}

/* Design philosophy — the principles that decide every feature, stated as
 * bare aphorisms (Zen-of-Python style). Each one tees up a section below; the
 * article is where they get demonstrated, so the lines stay unexplained. */
const TENETS: string[] = [
  'No viral edits.',
  'Look like TypeScript.',
  'Make undesired state unrepresentable.',
  "Fix JavaScript's footguns.",
  'One obvious way.',
  'Tools for agents, not IDEs.',
  'Make nondeterminism observable and testable.',
];

/* `view` splits this one document across routes: the homepage (`intro`) shows
 * the hero + design philosophy + the two CTAs; `/explore` (`deep`) shows Part 1
 * onward with the "On this page" rail; `all` (the legacy /baml-intro) renders
 * everything. */
// Code-block theme for this page. Flip this to try IDE themes: 'paper' (the
// light default), 'dark' (VS Code Dark+), or 'midnight' (Tokyo Night-ish). It
// drives the Monaco editors, the Shiki static panes, and the frame CSS at once.
const CODE_THEME: CodeThemeName = 'dark';

export function Article({ view = 'all' }: { view?: 'all' | 'intro' | 'deep' }) {
  const [activeId, setActiveId] = useState(
    view === 'deep' ? 'language' : 'philosophy',
  );

  // Scroll spy for the rail: a band near the top of the viewport decides
  // the current section. Ref callback with cleanup — no useEffect.
  // Also publishes the page's code theme to <body data-vscode-theme-kind> so
  // the playground graph (pkg-playground reads this attr first to pick its
  // light/dark palette) matches the editor instead of probing the page's
  // light --background. Restored on unmount.
  const spyRef = useCallback((node: HTMLElement | null) => {
    if (!node) return undefined;
    const prevKind = document.body.getAttribute('data-vscode-theme-kind');
    document.body.setAttribute(
      'data-vscode-theme-kind',
      CODE_THEMES[CODE_THEME].dark ? 'vscode-dark' : 'vscode-light',
    );
    const sections = Array.from(
      node.querySelectorAll<HTMLElement>('section[id], .l6-sub[id]'),
    );
    const order = new Map(sections.map((s, i) => [s.id, i]));
    const inBand = new Set<string>();
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) inBand.add(e.target.id);
          else inBand.delete(e.target.id);
        }
        let best: string | null = null;
        for (const id of inBand) {
          if (best === null || (order.get(id) ?? 0) > (order.get(best) ?? 0)) {
            best = id;
          }
        }
        if (best) setActiveId(best);
      },
      { rootMargin: '0px 0px -70% 0px' },
    );
    for (const s of sections) io.observe(s);
    return () => {
      io.disconnect();
      if (prevKind === null) {
        document.body.removeAttribute('data-vscode-theme-kind');
      } else {
        document.body.setAttribute('data-vscode-theme-kind', prevKind);
      }
    };
  }, []);

  return (
    <CodeThemeProvider value={CODE_THEME}>
      <div className="l6" data-code-theme={CODE_THEME} ref={spyRef}>
        {/* The shared site nav — every page uses the same one. It ships its own
          spacer + banner-offset rules, so the l6 layout doesn't reserve top
          room itself. */}
        <Navbar />
        {view !== 'intro' && (
          <nav aria-label="On this page" className="l6-toc">
            <p className="l6-toc-cap font-mono">On this page</p>
            <ul>
              {(view === 'deep'
                ? TOC.filter((item) => item.id !== 'philosophy')
                : TOC
              ).map((item) => (
                <li key={item.id}>
                  <a
                    className={`${item.sub ? 'l6-toc-sub' : ''}${activeId === item.id ? ' l6-toc-active' : ''
                      }`}
                    href={`#${item.id}`}
                  >
                    {item.label}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
        )}
        {/* hero + design philosophy: homepage (intro) and legacy /baml-intro */}
        {view !== 'deep' && (
          <>
            {/* ---- intro ---- */}
            <section className="l6-section l6-hero" id="top">
              <h1 className="l6-hero-title">
                <Image
                  alt=""
                  className="l6-hero-sheep"
                  height={72}
                  priority
                  src="/baml-sheep.png"
                  width={72}
                />
                {'BAML is the programming language for agents'}
              </h1>
              {view === 'intro' && <DesignGoalsCard />}
              {view === 'intro' && (
                <p>
                  {'TypeScript was designed for '}
                  <em>
                    <strong>human</strong>
                  </em>
                  {
                    ' productivity. AI agents are a new paradigm that require a new programming language, just like in the past:'
                  }
                </p>
              )}
              <ul className="l6-tenets">
                <li className="l6-tenet" style={{ fontSize: 'inherit' }}>
                  Hardware -&gt; Assembly
                </li>
                <li className="l6-tenet" style={{ fontSize: 'inherit' }}>
                  Operating Systems -&gt; Java
                </li>
                <li className="l6-tenet" style={{ fontSize: 'inherit' }}>
                  Web -&gt; Javascript
                </li>
                <li className="l6-tenet" style={{ fontSize: 'inherit' }}>
                  Agentic Coding -&gt; ????
                </li>
              </ul>
              <p>
                {
                  'BAML is a language designed to prevent context pollution and churn when coding with AI. Every feature opts to prevent mistakes at runtime (like Rust), while maintaining the dynamism necessary for writing and running code (like Python).'
                }
              </p>
              <p>
                {
                  'In one sentence: BAML feels like TypeScript, but with better error handling, no '
                }
                <code>any</code>
                {', and more.'}
              </p>
            </section>

            {/* ---- design philosophy ---- */}
            <Section id="philosophy" title="Our design philosophy">
              <ul className="l6-tenets font-mono">
                {TENETS.map((t) => (
                  <li className="l6-tenet" key={t}>
                    {t}
                  </li>
                ))}
              </ul>
            </Section>

            {/* two CTAs — only on the homepage landing */}
            {view === 'intro' && (
              <div className="l6-section l6-cta-wrap">
                <div className="l6-cta-row">
                  <Link className="l6-cta" href="/built-with-baml">
                    {'Built with BAML'}
                    <span aria-hidden>{'→'}</span>
                  </Link>
                  <Link className="l6-cta l6-cta--primary" href="/explore">
                    {'Explore BAML'}
                    <span aria-hidden>{'→'}</span>
                  </Link>
                </div>
              </div>
            )}
          </>
        )}

        {/* Part 1 onward: /explore (deep) and legacy /baml-intro */}
        {view !== 'intro' && (
          <>
            {/* ===== Part 1 · A better language ===== */}
            <Part eyebrow="Part 1" id="language" title="A better language">
              <p>
                BAML aims to be an agent friendly language. We'll start with the <u>syntax and type system decisions</u> we made. Then explore the <u>agent-first cli tooling</u>.
              </p>
              <p>
                As much as we want agents to write code, humans trust is still a vital part of a healthy software system. The third section focuses on <u>tooling for humans</u>, and the fourth shares how we made <u>BAML incrementally adoptable</u>, so you won't need to re-write your whole codebase in BAML.
              </p>
              <p>
                And lastly, not only has the way we write code changed, but also the <i>kind</i> of code we write as well. More and more code is agentic loops, created by LLMs on the fly, and probabilstic. We added a few syntax constructs to help <u>reign in the non-determinism.</u>
              </p>
            </Part>

            {/* ---- type system ---- */}
            <Section
              id="types"
              num="1"
              title="A type-system like TypeScript, but without type erasure"
            >
              <p>
                {'BAML has a type system like TypeScript, but persists it at runtime. TypeScript '}
                <a
                  className="l6-link"
                  href="https://github.com/Microsoft/TypeScript/wiki/TypeScript-Design-Goals#non-goals"
                  rel="noreferrer"
                  target="_blank"
                >
                  explicitly chose not to be sound
                </a>
                {
                  ", trading it away for productivity. That was the right move for humans, but it's the wrong default when agents are writing the code. It's not a conicidence there are 5 different schema validation libraries for TS: the type system doesn't mean enough."
                }
              </p>
              <p>
                {'BAML has no '}
                <code>any</code>
                {
                  ', types are what the code says at runtime, and it includes advanced features like unions, generics, recursive types, and interfaces on day one.'
                }
              </p>
              <div className="l6-pair">
                <div>
                  <p className="l6-pane-label">
                    typescript — where the types could lie to you
                  </p>
                  <BamlCode
                    code={TS_LIES}
                    diagnostics={[
                      {
                        line: 8,
                        message:
                          'runtime TypeError: email is undefined — far from here',
                        severity: 'error',
                      },
                    ]}
                    filename="load.ts"
                    highlightLines={[7]}
                    lang="typescript"
                  />
                </div>
                <div>
                  <p className="l6-pane-label l6-pane-label--after">
                    baml — caught at compile time
                  </p>
                  <BamlEditor filename="load.baml" initialCode={BAML_UNKNOWN} />
                </div>
              </div>

            </Section>

            {/* ---- match ---- */}
            <Section id="match" num="2" title="Match on types, or values">
              <p>
                {"Any of 'em work. No need for "}
                <code>instanceof</code>
                {' or '}
                <code>has</code>
                {' everywhere:'}
              </p>
              <div className="l6-pair">
                <div>
                  <p className="l6-pane-label">typescript</p>
                  <BamlCode
                    code={TS_INSTANCEOF}
                    filename="route.ts"
                    highlightLines={[4, 5, 6]}
                    lang="typescript"
                  />
                </div>
                <div>
                  <p className="l6-pane-label l6-pane-label--after">baml</p>
                  <BamlEditor filename="match.baml" initialCode={BAML_MATCH} />
                </div>
              </div>
            </Section>

            {/* ---- error handling ---- */}
            <Section
              id="error-handling"
              num="3"
              title="Error handling (it reads like match)"
            >
              <p>
                {
                  'TypeScript exceptions have no types, resulting in ugly code to handle the right error. BAML analyzes of every '
                }
                <code>throws</code>
                {
                  ' statement, and tells you every single error a function could throw. Hover '
                }
                <code className="l6-glow">fetch_page</code>
                {
                  ' in the editor below — the tooltip shows its full inferred error set. The warning is real: the compiler proves the '
                }
                <code>ParseError</code>
                {' arm can never fire.'}
              </p>
              <div className="l6-pair">
                <div>
                  <p className="l6-pane-label">typescript</p>
                  <BamlCode
                    code={TS_CATCH}
                    filename="run.ts"
                    highlightLines={[4]}
                    lang="typescript"
                  />
                </div>
                <div>
                  <p className="l6-pane-label l6-pane-label--after">
                    baml · with a live warning
                  </p>
                  <BamlEditor
                    filename="fetch.baml"
                    initialCode={BAML_UNREACHABLE}
                  />
                </div>
              </div>
            </Section>

            {/* ---- green threads ---- */}
            <Section
              id="threads"
              num="4"
              title="Green threads a.k.a 'async without async'"
            >
              <p>
                Doing work in parallel is important. But we always hated having an {' '}<code>async</code> and non-async version of our code. We chose Go's approach to concurrency, but with a typescript feel.
              </p>
              <p>
                BAML supports lightweight green threads via {' '}
                <code>spawn</code>
                {' and '}
                <code>await</code>
                {'. Run any function asynchronously without having to write '}
                <code>async function</code>
                {
                  ' in 10 other files everywhere. Easy to parallelize slow LLM http requests and tool calls.'
                }
              </p>
              <div className="l6-block">
                <BamlEditor
                  filename="spawn.baml"
                  highlightLines={[4, 8]}
                  initialCode={BAML_SPAWN}
                />
              </div>
              <p className="l6-note">
                {
                  'BAML describe can help agents figure out which functions might run asynchronously, by inspecting the code.'
                }
              </p>

              <Sub
                title={
                  <>
                    <code>spawn</code>
                    {' can run cpu-bound code in parallel'}
                  </>
                }
              >
                <p>
                  {
                    'This is the part Promise.all cannot do: JavaScript fans out I/O, but compute still shares one thread. We scanned 38 GB of log-like text and got a 9x improvement when parallelized into chunks with BAML.'
                  }
                </p>
                <SpawnChart />
                <p className="l6-dim">
                  {
                    "BAML's stdlib string search is native Rust, so even one thread edges out Bun here. (The one place Bun still wins per core is tight arithmetic loops, where its JIT beats our interpreter.)"
                  }
                </p>
                <details className="l6-details">
                  <summary>show the benchmark source — bench.baml</summary>
                  <div className="l6-breakout">
                    <BamlCode code={BENCH_BAML} filename="bench.baml" />
                  </div>
                </details>
              </Sub>

              <p style={{ marginTop: '2rem' }}>
                {
                  "We also have built-in primitives for managing concurrency (task groups etc), which we'll get to later!"
                }
              </p>
              <Scheduler />
            </Section>

            {/* ===== Part 2 · Tools for agents ===== */}
            <Part eyebrow="Part 2" id="agent-tools" title="Tools for agents">
              <p>
                {
                  '[placeholder: intro — these are CLI tools built for an agent at a terminal, not a human in an IDE.]'
                }
              </p>
            </Part>

            {/* ---- namespaces ---- */}
            <Section
              id="namespaces"
              num="1"
              title="Namespaces are just directories, and there are no imports"
            >
              <p>
                {
                  'AI agents spend too much time searching for things in large projects. In BAML the project structure is self-describing. Namespaces are just directories with a '
                }
                <code>ns_</code>
                {
                  ' prefix. There are no imports because everything is referred with its fully qualified name, like Go. Inside a namespace directory, all types, functions and objects are available in every file by default.'
                }
              </p>
              <div className="l6-block">
                <TermPlay events={LS_EVENTS} title="the filesystem is the map" />
              </div>
              <div className="l6-block">
                <BamlCode
                  code={NS_BAD}
                  diagnostics={[
                    {
                      line: 2,
                      message:
                        'unresolved type: Widget. Did you mean `root.a.Widget`?',
                      severity: 'error',
                    },
                  ]}
                  filename="ns_b/b.baml"
                />
              </div>
              <div className="l6-block">
                <BamlCode
                  code={NS_GOOD}
                  filename="ns_b/b.baml"
                  highlightLines={[2, 3]}
                />
              </div>
              <p className="l6-note">
                {
                  'This is also why constructing a Class in BAML requires always adding the name of the class — '
                }
                <code>{'MyClass { }'}</code>
                {'. There are no anonymous records.'}
              </p>
            </Section>

            {/* ---- describe ---- */}
            <Section
              id="describe"
              num="2"
              title="baml describe — a built-in AST-based grep, to find things faster"
            >
              <p>
                <code>describe</code>
                {
                  " is easier for agents to use than an LSP, and more informative than grep — agents writing BAML code don't need to search through 10 files to figure out how things work. Here's a transcript of an agent searching with grep, versus with baml describe:"
                }
              </p>
              <div className="l6-pair">
                <div>
                  <p className="l6-pane-label">agent with grep</p>
                  <TermPlay events={GREP_EVENTS} title="agent without describe" />
                </div>
                <div>
                  <p className="l6-pane-label l6-pane-label--after">
                    agent with describe
                  </p>
                  <TermPlay
                    events={DESCRIBE_EVENTS}
                    title="agent with describe"
                  />
                </div>
              </div>
              <p className="l6-note">
                {
                  "The reference list is the part grep can't give you: every call site, resolved — handy for spotting near-duplicates before writing a second copy of a function. We'll keep making improvements to this tool."
                }
              </p>
            </Section>

            {/* ---- run <function> ---- */}
            <Section id="run-fn" num="3" title="baml run <function>">
              <p>
                {
                  'BAML makes it easy for agents to run any function in your project as if it were a CLI command. Function parameters get parsed automatically and can be set with CLI flags.'
                }
              </p>
              <div className="l6-block">
                <TermPlay events={RUN_FN_EVENTS} title="baml run <function>" />
              </div>
            </Section>

            {/* ---- run -e ---- */}
            <Section
              id="run-e"
              num="4"
              title="baml run -e — run small baml programs inline"
            >
              <p>
                {
                  'Run small baml programs inline, without having to write to a file. Small simple feature, but great for agents writing/testing small baml scripts.'
                }
              </p>
              <div className="l6-block">
                <TermPlay events={RUN_E_EVENTS} title="baml run -e" />
              </div>
            </Section>

            {/* ---- pack ---- */}
            <Section
              id="pack"
              num="5"
              title="baml pack — ship a function as a tiny binary"
            >
              <p>
                {
                  'BAML pack is a CLI that takes your baml program and auto-creates a CLI for you from the function signature. It can compile and run on any target architecture. Useful for agents creating shareable mini programs.'
                }
              </p>
              <div className="l6-block">
                <TermPlay events={PACK_EVENTS} title="baml pack" />
              </div>
              <details className="l6-details">
                <summary>show the source — main.baml</summary>
                <BamlCode
                  code={BAML_PACKED}
                  filename="main.baml"
                  highlightLines={[1]}
                  notes={[{ line: 1, text: 'name: string → --name <flag>' }]}
                />
              </details>
              <Sub title="The packed binary is 81% smaller than Bun's">
                <p>
                  {
                    "Here's a comparison of BAML vs Bun in creating a compiled binary. The binary size is just 12.1 MB:"
                  }
                </p>
                <PackChart />
                <p className="l6-dim">
                  {
                    'Bun 1.3.14, BAML release toolchain, aarch64-apple-darwin. Bun embeds a whole JavaScript engine; the BAML runtime is 12.1 MB.'
                  }
                </p>
              </Sub>
            </Section>

            {/* ===== Part 3 · Tools for humans ===== */}
            <Part eyebrow="Part 3" id="human-tools" title="Tools for humans">
              <p>
                {
                  "Now that most code isn't being read by humans, we created tooling to help humans understand what software is doing at a glance."
                }
              </p>
            </Part>

            {/* ---- navigating codebases (placeholder) ---- */}
            <Section
              id="nav-viz"
              num="1"
              title="[placeholder: navigating codebases]"
            >
              <p>
                {
                  '[placeholder: a visualization for navigating a BAML codebase — the visual counterpart to baml describe.]'
                }
              </p>
            </Section>

            {/* ---- flame graphs / observable code (placeholder) ---- */}
            <Section
              id="observability"
              num="2"
              title="[placeholder: flame graphs / observable code]"
            >
              <p>
                {
                  '[placeholder: flame graphs and observable execution for humans. Callout: this same data is accessible to agents too.]'
                }
              </p>
            </Section>

            {/* ===== Part 4 · How we make BAML usable ===== */}
            <Part eyebrow="Part 4" id="usable" title="How we make BAML usable">
              <p>
                {
                  "A language is only as strong as its community. Here's some features we added to help make it easier to adopt."
                }
              </p>
            </Part>

            {/* ---- incremental adoption ---- */}
            <Section
              id="adoption"
              num="1"
              title="BAML is incrementally adoptable"
            >
              <p>
                {
                  'When we first made BAML 2 years ago we decided it had to be callable from other languages, with an amazing developer experience.'
                }
              </p>
              <p>
                {
                  'BAML can now generate SDKs for your favorite language, and call your functions using these type-safe interfaces — even if they include generics, or class methods. Think of an OpenAPI client generator, except the contract carries real business logic, not just data shapes. (For a more in-depth technical write-up, please check out our '
                }
                <a
                  className="l6-link"
                  href="https://boundaryml.com/blog"
                  rel="noreferrer"
                  target="_blank"
                >
                  blog post
                </a>
                {'.)'}
              </p>
              <SdkPipeline />
              <p>
                {
                  'The types come out native — a pydantic model in Python, a typed class in TypeScript — with your functions, methods, and generics intact. Pick a feature and a language to see the same BAML file generate each SDK:'
                }
              </p>
              <div className="l6-breakout">
                <SdkExplorer />
              </div>
            </Section>

            {/* ---- self improvement ---- */}
            <Section
              id="self-improvement"
              num="2"
              title="Recursive self-improvement"
            >
              <p>
                {
                  "To help keep BAML stable and improving over time, we're simulating thousands of agents writing BAML code to get feedback from agents themselves. We built "
                }
                <a
                  className="l6-link"
                  href="https://new.boundaryml.com/atb"
                  rel="noreferrer"
                  target="_blank"
                >
                  agent-tries-baml
                </a>
                {
                  ' to recursively self-improve BAML and make it easier for agents to write. For example, we '
                }
                <a
                  className="l6-link"
                  href="https://new.boundaryml.com/atb/arena"
                  rel="noreferrer"
                  target="_blank"
                >
                  test our BAML skill against agents
                </a>
                {
                  ' to figure out which set of instructions helps agents write BAML faster.'
                }
              </p>
              <p>
                {
                  "BAML is still < 1.0, but we're close to reaching full stability — feel free to join our language experiments if you're curious about this process."
                }
              </p>
              <SelfImprove />
            </Section>

            {/* ---- supply chain (aside) ---- */}
            <Section
              id="supply-chain"
              num="3"
              title="No supply chain attacks (yet)"
            >
              <p>
                {"Okay, to be fair, BAML doesn't "}
                <em>yet</em>
                {
                  " have a package manager. We're working on it! In the meantime, just make AI agents write all the code you need."
                }
              </p>
            </Section>

            {/* ===== Part 5 · Building agents ===== */}
            <Part eyebrow="Part 5" id="agents" title="Building agents">
              <p>
                {'Writing code is one thing, but in the future '}
                <em>every</em>
                {
                  ' software program will interact with AI agents or non-deterministic AI code. Whilst BAML supports writing anything from a web-server to a data-processing library, our main focus is to provide primitives to help teams deal with nondeterminism. To do this we make sure BAML programs are observable, testable, and measurable.'
                }
              </p>
              <InfectionGraph />
            </Part>

            {/* ---- llm calls as functions ---- */}
            <Section
              id="llm-functions"
              num="1"
              title="Native LLM Functions — composable building blocks for agents and harnesses"
            >
              <p>
                {
                  "An LLM call in BAML is just a function: the prompt is the body, the return type is the schema. Because it's a real function, it can be evaluated, optimized, and tracked at runtime by observability platforms."
                }
              </p>
              <p>
                {
                  "If you've used BAML in the last 2 years, you'll be happy to hear we still have our error-correcting JSON parser, which reliably coaxes structured output out of small language models."
                }
              </p>
              <p>
                {
                  "BAML ships with tooling to observe LLM function inputs and outputs, like our workflow visualizer in VSCode. It's especially helpful when working with multimodal outputs, like images."
                }
              </p>
              <WorkflowPlayground />
            </Section>

            {/* ---- claude apis as functions (placeholder) ---- */}
            <Section
              id="claude-code"
              num="2"
              title="[placeholder: Claude APIs as functions]"
            >
              <p>
                {
                  "[placeholder: use claude-code as a client option, with its tools etc. Provider/client wrapping for Claude's agentic API surface.]"
                }
              </p>
            </Section>

            {/* ---- testing ---- */}
            <Section id="testing" num="3" title="BAML Tests">
              <p>{'Write tests anywhere, in any file.'}</p>
              <p>
                {
                  'Create arbitrary groups and add tests dynamically — generate tests for each item in an array, create tests from a CSV file, or from S3:'
                }
              </p>
              <TestExampleTabs />
              <p className="l6-dim">
                {
                  'View tests in the Playground — in case a human needs to see things, we have nice utilities — or just have agents run '
                }
                <code>baml test</code>
                {'.'}
              </p>
              <p>
                {
                  "Create evals — LLM-as-judge, statistical analysis, etc. In other frameworks that's a YAML schema and a hosted UI. In BAML, it's all just code. Pass a test when at least N% of runs do, using custom test runners:"
                }
              </p>
              <div className="l6-block">
                <BamlEditor filename="evals.baml" initialCode={BAML_RUNNER} />
              </div>
              <p className="l6-note">
                {
                  'Custom test runners go further: retries, uploading reports, running things in parallel or synchronously.'
                }
              </p>
            </Section>

            {/* ---- eval / codemode (placeholder) ---- */}
            <Section id="eval" num="4" title="[placeholder: eval / codemode]">
              <p>
                {
                  "[placeholder: a type-safe eval(), the equivalent of Python's eval — our answer to codemode, but with type-safety. Syntax TBD.]"
                }
              </p>
            </Section>

            {/* ---- close ---- */}
            <Section id="close" title="Try it out!">
              <TryItTabs />
              <p>
                <a
                  className="l6-link"
                  href="https://new.boundaryml.com/quickstart"
                  rel="noreferrer"
                  target="_blank"
                >
                  new.boundaryml.com/quickstart →
                </a>
              </p>
              <p>
                {'Join our '}
                <a
                  className="l6-link"
                  href="https://boundaryml.com/discord"
                  rel="noreferrer"
                  target="_blank"
                >
                  Discord
                </a>
                {'.'}
              </p>
            </Section>
          </>
        )}
      </div>
    </CodeThemeProvider>
  );
}
