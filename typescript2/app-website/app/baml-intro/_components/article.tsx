/** biome-ignore-all lint/a11y/useAnchorContent: legacy article markup, icon links are aria-labeled elsewhere */
/** biome-ignore-all lint/a11y/noSvgWithoutTitle: decorative inline svgs */
'use client';

// CONTENT PARITY: this component renders / and /explore. Keep substantive
// copy in sync with content/index.md and content/explore.md. Interactive
// components need complete text/code fallbacks in the Markdown files.

import Image from 'next/image';
import Link from 'next/link';
import { type ReactNode, useCallback, useState } from 'react';
import BamlEditor from '@/app/learn2/_components/baml-editor-lazy';
import { DiscordCta } from '@/components/discord-cta';
import { Navbar } from '@/components/navbar';
import { BamlCode } from '../../learn2/_components/BamlCode';
import LivePlayground from '../../learn2/_components/LivePlaygroundLazy';
import {
  CODE_THEMES,
  type CodeThemeName,
  CodeThemeProvider,
} from '../../learn2/_lib/code-theme';
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
  BAML_EVAL,
  BAML_HTTP_TESTS,
  BAML_IMAGE,
  BAML_MATCH,
  // BAML_METRIC, // metrics section hidden
  BAML_PACKED,
  BAML_RUNNER,
  BAML_SANDBOX,
  BAML_SPAWN,
  BAML_UNKNOWN,
  BAML_UNREACHABLE,
  BAML_WF_FANOUT,
  BAML_WF_TALLY,
  BENCH_BAML,
  DESCRIBE_EVENTS,
  GREP_EVENTS,
  LS_EVENTS,
  NAV_CODEBASE,
  NS_BAD,
  NS_GOOD,
  PACK_EVENTS,
  RUN_E_EVENTS,
  RUN_FN_EVENTS,
  TS_CATCH,
  TS_INSTANCEOF,
  TS_LIES,
} from './snippets';
import { TenetsAccordion } from './TenetsAccordion';
import { TryBaml } from './try-baml';

/* All sections share one reading-column width so every header aligns.
 * Editors, playgrounds, and side-by-side pairs break out wider via
 * .l6-pair / .l6-breakout — text never does. */
/* A share affordance on each header: revealed on hover, it links to the
 * section anchor and copies the full URL to the clipboard on click. */
function AnchorLink({ id }: { id: string }) {
  return (
    <a
      aria-label="Link to this section"
      className="l6-anchor"
      href={`#${id}`}
      onClick={() => {
        const url = `${window.location.origin}${window.location.pathname}#${id}`;
        navigator.clipboard?.writeText(url).catch(() => {});
      }}
    >
      <svg
        aria-hidden
        fill="none"
        height="14"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="14"
      >
        <path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
        <path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
      </svg>
    </a>
  );
}

/* A small "spec" chip linking the section to its design proposal (BEP) on
 * beps.boundaryml.com. The route uses the zero-padded, min-two-digit number
 * (/beps/02, /beps/16); the label shows the canonical BEP-002 form. Opens in
 * a new tab. */
function BepLink({ n }: { n: number }) {
  const label = `BEP-${String(n).padStart(3, '0')}`;
  return (
    <a
      aria-label={`${label} — read the design proposal`}
      className="l6-bep font-mono"
      href={`https://beps.boundaryml.com/beps/${String(n).padStart(2, '0')}`}
      rel="noreferrer"
      target="_blank"
    >
      {label}
      <span aria-hidden>{'↗'}</span>
    </a>
  );
}

function Section({
  id,
  num,
  title,
  bep,
  children,
}: {
  id: string;
  num?: string;
  title: string;
  bep?: number;
  children: ReactNode;
}) {
  return (
    <section className="l6-section" id={id}>
      <h2>
        {num ? <span className="l6-num font-mono">{num}</span> : null}
        {title}
        {bep ? <BepLink n={bep} /> : null}
        <AnchorLink id={id} />
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
        {id ? <AnchorLink id={id} /> : null}
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
      <h2 className="l6-part-title">
        {title}
        <AnchorLink id={id} />
      </h2>
      {children}
    </section>
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
  { id: 'nav-viz', label: '1 · Workflow View', sub: true },
  { id: 'observability', label: '2 · Profiler', sub: true },
  { id: 'usable', label: 'Part 4 · Adopting BAML' },
  { id: 'adoption', label: '1 · Drops into your stack', sub: true },
  { id: 'self-improvement', label: '2 · Self-improvement', sub: true },
  { id: 'supply-chain', label: '3 · No supply chain attacks', sub: true },
  { id: 'agents', label: 'Part 5 · Building agents' },
  { id: 'llm-functions', label: '1 · LLM functions', sub: true },
  { id: 'claude-code', label: '2 · Agents & harnesses', sub: true },
  { id: 'testing', label: '3 · Tests', sub: true },
  { id: 'eval', label: '4 · eval / codemode', sub: true },
  { id: 'sandboxing', label: '5 · Sandboxing', sub: true },
  { id: 'close', label: 'Try BAML out!' },
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

    // On a hash load (e.g. /explore#describe), the browser's anchor jump fires
    // before the ssr:false editors/playgrounds above the target mount and grow.
    // That growth pushes the target down, leaving the viewport parked on the
    // previous section. Re-pin to the anchor as the layout settles, backing off
    // the instant the user scrolls, and stop once things stabilize.
    const hashId = decodeURIComponent(window.location.hash.slice(1));
    const target = hashId ? document.getElementById(hashId) : null;
    let pinObserver: ResizeObserver | null = null;
    let settleTimer: ReturnType<typeof setTimeout> | undefined;
    const stopPin = () => {
      pinObserver?.disconnect();
      pinObserver = null;
      clearTimeout(settleTimer);
      window.removeEventListener('wheel', stopPin);
      window.removeEventListener('touchmove', stopPin);
      window.removeEventListener('keydown', stopPin);
    };
    if (target) {
      const pin = () =>
        target.scrollIntoView({ behavior: 'instant', block: 'start' });
      pinObserver = new ResizeObserver(pin);
      pinObserver.observe(node);
      window.addEventListener('wheel', stopPin, { passive: true });
      window.addEventListener('touchmove', stopPin, { passive: true });
      window.addEventListener('keydown', stopPin);
      settleTimer = setTimeout(stopPin, 2500);
      pin();
    }

    return () => {
      io.disconnect();
      stopPin();
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
              {TOC.map((item) => (
                <li key={item.id}>
                  <a
                    className={`${item.sub ? 'l6-toc-sub' : ''}${
                      activeId === item.id ? ' l6-toc-active' : ''
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
              {view === 'intro' && (
                <div className="l6-cta-wrap">
                  <div className="l6-cta-row">
                    <Link className="l6-cta l6-cta--primary" href="/explore">
                      <Image
                        alt=""
                        aria-hidden
                        height={18}
                        src="/baml-lamb-white.png"
                        style={{ filter: 'brightness(0) invert(1)' }}
                        width={18}
                      />
                      {'Explore BAML'}
                      <span aria-hidden>{'→'}</span>
                    </Link>
                  </div>
                </div>
              )}
              {view === 'intro' && <DesignGoalsCard />}
              {view === 'intro' && (
                <p>
                  {'What they meant was '}
                  <em>
                    <strong style={{ color: 'var(--l6-accent)' }}>human</strong>
                  </em>
                  {' productivity.'}
                  <br />
                  {
                    'AI agents are a new paradigm that requires a new programming language, just like in the past:'
                  }
                </p>
              )}
              <ul className="l6-tl">
                <li className="l6-tl-item">
                  <span className="l6-tl-node" />
                  <span className="l6-tl-era">Hardware</span>
                  <span className="l6-tl-arrow">→</span>
                  <span className="l6-tl-lang">Assembly</span>
                </li>
                <li className="l6-tl-item">
                  <span className="l6-tl-node" />
                  <span className="l6-tl-era">Operating Systems</span>
                  <span className="l6-tl-arrow">→</span>
                  <span className="l6-tl-lang">Java</span>
                </li>
                <li className="l6-tl-item">
                  <span className="l6-tl-node" />
                  <span className="l6-tl-era">Web</span>
                  <span className="l6-tl-arrow">→</span>
                  <span className="l6-tl-lang">Javascript</span>
                </li>
                <li className="l6-tl-item l6-tl-item--now">
                  <span className="l6-tl-node l6-tl-node--now" />
                  <span className="l6-tl-era">Agentic Coding</span>
                  <span className="l6-tl-arrow">→</span>
                  <span className="l6-tl-lang">BAML</span>
                </li>
              </ul>
              <p>
                {
                  'BAML is a language designed to prevent context pollution and churn when coding with AI. Every feature opts to prevent mistakes at runtime (like Rust), while maintaining the dynamism necessary for writing and running code (like Python).'
                }
              </p>
              <p>
                {'In one sentence: '}
                <strong style={{ color: 'var(--l6-accent)' }}>BAML</strong>
                {' feels like TypeScript, but with better error handling, no '}
                <code>any</code>
                {', and more.'}
              </p>
            </section>
          </>
        )}

        {/* Part 1 onward: /explore (deep) and legacy /baml-intro */}
        {view !== 'intro' && (
          <>
            {/* ---- design philosophy (opens the deep dive) ---- */}
            <Section id="philosophy" title="Our design philosophy">
              <TenetsAccordion />
            </Section>

            {/* ---- quick install pointer (top of /explore) ----
               The homepage "Explore" CTA lands here; give impatient readers an
               install path before the deep dive instead of only at the "Try
               BAML out!" close. The full unit lives at #close and on
               /quickstart. */}
            {view === 'deep' && (
              <section className="l6-section" id="try">
                <p className="l6-part-eyebrow font-mono">Try BAML</p>
                <TryBaml compact />
                <p>
                  <Link className="l6-link" href="/quickstart">
                    Full quickstart: editor setup and more options →
                  </Link>
                </p>
              </section>
            )}

            {/* ===== Part 1 · A better language ===== */}
            <Part eyebrow="Part 1" id="language" title="A better language">
              <p>
                BAML aims to be an agent-friendly language. In this overview,
                we'll start with the{' '}
                <a className="l6-xref" href="#types">
                  syntax and type system decisions
                </a>{' '}
                we made. Then explore the{' '}
                <a className="l6-xref" href="#agent-tools">
                  agent-first cli tooling
                </a>
                .
              </p>
              <p>
                As much as we want agents to write code, human trust is still a
                vital part of a healthy software system. The third section
                focuses on{' '}
                <a className="l6-xref" href="#human-tools">
                  tooling for humans
                </a>
                , and the fourth shares how we made{' '}
                <a className="l6-xref" href="#adoption">
                  BAML incrementally adoptable
                </a>
                , so you won't need to re-write your whole codebase in BAML.
              </p>
              <p>
                And lastly, not only has the way we write code changed, but also
                the <i>kind</i> of code we write as well. More and more code is
                agentic loops, created by LLMs on the fly, and probabilistic. We
                added a few syntax constructs to help{' '}
                <a className="l6-xref" href="#agents">
                  rein in the non-determinism
                </a>
                .
              </p>
            </Part>

            {/* ---- type system ---- */}
            <Section
              id="types"
              num="1"
              title="A type-system like TypeScript, but without type erasure"
            >
              <p>
                {
                  'BAML has a type system like TypeScript, but persists it at runtime. TypeScript '
                }
                <a
                  className="l6-link"
                  href="https://github.com/Microsoft/TypeScript/wiki/TypeScript-Design-Goals#non-goals"
                  rel="noreferrer"
                  target="_blank"
                >
                  explicitly chose not to be sound
                </a>
                {
                  ", trading it away for productivity. That was the right move for humans, but it's the wrong default when agents are writing the code. It's not a coincidence there are 5 different schema validation libraries for TS: the type system doesn't mean enough."
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
                        message: 'runtime TypeError: email is undefined',
                        severity: 'error',
                      },
                    ]}
                    filename="load.ts"
                    highlightLines={[7]}
                    lang="typescript"
                    wrap
                  />
                </div>
                <div>
                  <p className="l6-pane-label l6-pane-label--after">
                    baml — caught at compile time
                  </p>
                  <BamlEditor
                    editHint
                    filename="load.baml"
                    initialCode={BAML_UNKNOWN}
                  />
                </div>
              </div>
            </Section>

            {/* ---- match ---- */}
            <Section
              bep={15}
              id="match"
              num="2"
              title="Match on types, or values"
            >
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
              bep={2}
              id="error-handling"
              num="3"
              title="Error handling (it reads like match)"
            >
              <p>
                {
                  'TypeScript exceptions have no types, so catching the right one means ugly code. BAML reads every '
                }
                <code>throws</code>
                {
                  ' statement and tells you every single error a function can throw. Hover '
                }
                <code className="l6-glow">fetch_page</code>
                {
                  ' below to see its full inferred error set. That live warning is the compiler proving the '
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
              bep={34}
              id="threads"
              num="4"
              title="Green threads a.k.a 'async without async'"
            >
              <p>
                Doing work in parallel is important. But we always hated having
                an <code>async</code> and non-async version of our code. We
                chose Go's approach to concurrency, but with a typescript feel.
              </p>
              <p>
                BAML supports lightweight green threads via <code>spawn</code>
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
                    'Promise.all only parallelizes I/O; compute still runs on one thread. In BAML you can split 38 GB of logs into chunks and scan them in parallel, 9x faster.'
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

            {/* community CTA between Part 1 and Part 2 */}
            <div className="l6-section l6-join">
              <Link
                className="l6-join-link"
                href="https://boundaryml.com/discord"
                rel="noreferrer"
                target="_blank"
              >
                {'Join the community on Discord →'}
              </Link>
            </div>

            {/* ===== Part 2 · Tools for agents ===== */}
            <Part eyebrow="Part 2" id="agent-tools" title="Tools for agents">
              <p>
                BAML ships with various tools to make agents find, test and
                distribute code more easily.
              </p>
            </Part>

            {/* ---- namespaces ---- */}
            <Section
              bep={8}
              id="namespaces"
              num="1"
              title="ls — the filesystem is the namespace structure"
            >
              <p>
                {
                  'AI agents spend too much time searching for things in large projects. In BAML the project structure is self-describing: an agent can '
                }
                <code>ls</code>
                {
                  " a BAML project and know how it's laid out, because namespaces are just directories with a "
                }
                <code>ns_</code>
                {
                  ' prefix. There are no imports, since everything is referred to by its fully qualified name, like Go. Inside a namespace directory, all types, functions and objects are available in every file by default.'
                }
              </p>
              <div className="l6-block">
                <TermPlay
                  events={LS_EVENTS}
                  title="the filesystem is the map"
                />
              </div>
              <div className="l6-block">
                <BamlCode
                  code={NS_BAD}
                  diagnostics={[
                    {
                      line: 2,
                      message:
                        'unresolved type: Product. Did you mean `root.catalog.Product`?',
                      severity: 'error',
                    },
                  ]}
                  filename="ns_orders/order.baml"
                />
              </div>
              <div className="l6-block">
                <BamlCode
                  code={NS_GOOD}
                  filename="ns_orders/order.baml"
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
                  " is easier for agents to use than an LSP, and more informative than grep. Agents writing BAML code don't need to search through 10 files to figure out how things work. Here's a transcript of an agent searching with grep, versus with baml describe:"
                }
              </p>
              <div className="l6-pair">
                <div>
                  <p className="l6-pane-label">agent with grep</p>
                  <TermPlay
                    events={GREP_EVENTS}
                    title="agent without describe"
                  />
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
            <Section bep={27} id="run-fn" num="3" title="baml run <function>">
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
                  "We also built tools to keep humans in the loop. Even if most code isn't being read, these tools can help humans dive deep and iterate quickly when they need to."
                }
              </p>
            </Part>

            {/* ---- navigating codebases ---- */}
            <Section
              id="nav-viz"
              num="1"
              title="BAML Workflow View — navigate and understand your code"
            >
              <p>
                {
                  "Here's a fuller BAML project: an LLM “Heads Up” guessing game with an agent loop, a non-LLM binary search, classes, and a couple of testsets. The graph view is the visual counterpart to "
                }
                <code>baml describe</code>
                {
                  ': a map you can click through instead of grepping. Open the graph tab and jump around.'
                }
              </p>
              <div className="l6-breakout l6-breakout--xl">
                <LivePlayground
                  filename="game.baml"
                  initialCode={NAV_CODEBASE}
                  initialFunction="GuessGameAgent"
                  initialSidebarOpen={false}
                  initialTab="graph"
                  isolated
                  loadingLabel="Loading codebase…"
                />
              </div>
            </Section>

            {/* ---- flame graphs / observable code ---- */}
            <Section id="observability" num="2" title="BAML Profiler">
              <p>
                {
                  "We also shipped a profiler to help you visualize flame graphs and see what's causing potential slowness. Agents can use this tool too, but humans can also visualize and dive into the nitty-gritty details."
                }
              </p>
              <div style={{ margin: '0.5rem auto 0', maxWidth: 600 }}>
                <Image
                  alt="BAML playground Flame tab — a flame graph of a run with a per-function self-time table on the left."
                  className="l6-shot"
                  height={1077}
                  sizes="(max-width: 640px) 100vw, 600px"
                  src="/flamegraph-flame-view.png"
                  width={1160}
                />
              </div>
            </Section>

            {/* ===== Part 4 · Adopting BAML ===== */}
            <Part eyebrow="Part 4" id="usable" title="Adopting BAML">
              <p>
                {
                  "Although we are still pre-1.0, BAML is ready to use today. Here's how we make it easier to use and trust in production."
                }
              </p>
            </Part>

            {/* ---- incremental adoption ---- */}
            <Section
              bep={30}
              id="adoption"
              num="1"
              title="Drops into your existing stack"
            >
              <p>
                {
                  'When we first made BAML 2 years ago we decided it had to be callable from other languages, with an amazing developer experience.'
                }
              </p>
              <p>
                {
                  'BAML can generate SDKs for your favorite language, and call your functions using these type-safe interfaces, even if they include generics or class methods. Think of an OpenAPI client generator, except the contract carries real business logic, not just data shapes. (For more details, check out our '
                }
                <a
                  className="l6-link"
                  href="https://www.youtube.com/watch?v=ve33hCLHbcg"
                  rel="noreferrer"
                  target="_blank"
                >
                  talk at rust conf
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
                  href="/atb"
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
                  href="/atb/arena"
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
                  "BAML is still < 1.0, but we're close to reaching full stability. Feel free to join our language experiments if you're curious about this process."
                }
              </p>
              <SelfImprove />
            </Section>

            {/* ---- supply chain (aside) ---- */}
            <Section id="supply-chain" num="3" title="No supply chain attacks">
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

            {/* ---- claude apis as functions ---- */}
            <Section
              id="claude-code"
              num="2"
              title="Build harnesses, agents, or delegate to Claude Code"
            >
              <p>
                {
                  "We're currently building our first-class standard library to build AI agents and harnesses, or call other kinds of agents (Claude Code). It will support anything from realtime voice agents to batched APIs. "
                }
                <a
                  className="l6-link"
                  href="https://boundaryml.com/discord"
                  rel="noreferrer"
                  target="_blank"
                >
                  Let us know
                </a>
                {" if you're interested in an early preview!"}
              </p>
            </Section>

            {/* ---- testing ---- */}
            <Section
              bep={23}
              id="testing"
              num="3"
              title="Write tests anywhere, or load them at runtime"
            >
              <p>{'Write tests anywhere, in any file.'}</p>
              <p>
                {
                  'Create arbitrary groups and add tests dynamically — generate tests for each item in an array, create tests from a CSV file, or from S3:'
                }
              </p>
              <TestExampleTabs />
              <p className="l6-dim">
                {
                  'View tests in the Playground: in case a human needs to see things, we have nice utilities. Or just have agents run '
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

            {/* ---- eval / codemode (coming soon) ---- */}
            <Section id="eval" num="4" title="eval(), but type-safe">
              <p>
                Agents don't just call tools, they also write and run code.{' '}
                <s>Twitter</s> X calls it codemode.
              </p>
              <p>
                In python, you would write{' '}
                <code>eval('print("hello world")')</code> to do codemode. But{' '}
                <code>eval</code> is unsafe and loses all type-safety and
                predictability.
              </p>
              <p>
                BAML's reflection APIs give you eval, but with typed compiler
                errors. If the string has the wrong signature, you can get a
                runtime-compiler error that you can feed back to the agent so it
                can fix its code.
              </p>
              <p className="l6-note">
                Coming soon: the reflection API below isn't available yet.
              </p>
              <div className="l6-block">
                <BamlCode code={BAML_EVAL} filename="codemode.baml" />
              </div>
            </Section>

            {/* ---- sandboxing (coming soon) ---- */}
            <Section bep={58} id="sandboxing" num="5" title="Sandboxing">
              <p>
                Running code an agent just wrote is scary. We've started using
                machine sandboxing to isolate the code from the rest of the
                system, but what if we wanted to guarantee that the code doesn't
                make any network calls? We could just prompt it, but...
              </p>
              <p>
                We can do a bit better. BAML supports mocking any function,
                whether it's in the standard library or in your own package. You
                can swap it out with another implementation, and it only works
                in a certain scope.
              </p>
              <p>
                This doesn't replace the need for machine sandboxing.{' '}
                <code>mock</code> can't sandbox machine state (though vfs's are
                much simpler now). However, it does give you an option to not
                require machine sandboxing for every problem.
              </p>
              <p className="l6-note">
                Coming soon: the mocking primitive below isn't available yet.
              </p>
              <div className="l6-block">
                <BamlCode code={BAML_SANDBOX} filename="sandbox.baml" />
              </div>
            </Section>

            {/* ---- close ---- */}
            <Section id="close" title="Try BAML out!">
              <TryBaml />
              <p>
                <a
                  className="l6-link"
                  href="https://boundaryml.com/quickstart"
                  rel="noreferrer"
                  target="_blank"
                >
                  boundaryml.com/quickstart →
                </a>
              </p>
              <DiscordCta />
            </Section>
          </>
        )}
      </div>
    </CodeThemeProvider>
  );
}
