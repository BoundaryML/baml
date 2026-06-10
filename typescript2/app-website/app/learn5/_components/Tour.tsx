'use client';

import { type ReactNode, useCallback, useState } from 'react';
import { BamlCode } from '../../learn2/_components/BamlCode';
import BamlEditor from '../../learn2/_components/BamlEditorLazy';
import LivePlayground from '../../learn2/_components/LivePlaygroundLazy';
import { Terminal } from '../../learn2/_components/primitives';
import { CoreUsage } from '../../learn3/_components/CoreUsage';
import { InfectionGraph } from '../../learn3/_components/InfectionGraph';
import { TermPlay } from '../../learn3/_components/TermPlay';
import {
  BAML_AGENT,
  BAML_HELLO,
  BAML_IMAGE,
  BAML_PACKED,
  BAML_PASSRATE,
  BAML_SENTIMENT,
  BAML_SPAWN,
  BAML_SPAWN_ADV,
  BAML_TALLY,
  BAML_TEST,
  BAML_UNREACHABLE,
  DESCRIBE_EVENTS,
  LS_EVENTS,
  NS_BAD,
  NS_GOOD,
  PACK_EVENTS,
  PY_CALLBACK,
  PY_SDK,
} from './snippets';

const NAV: { group: string; items: { id: string; label: string }[] }[] = [
  {
    group: 'Getting started',
    items: [{ id: 'overview', label: 'Overview' }],
  },
  {
    group: 'The language',
    items: [
      { id: 'basics', label: 'Functions & classes' },
      { id: 'llm-functions', label: 'LLM functions' },
      { id: 'errors', label: 'Error handling' },
      { id: 'concurrency', label: 'Concurrency' },
      { id: 'tests', label: 'Tests' },
    ],
  },
  {
    group: 'Working with models',
    items: [
      { id: 'nondeterminism', label: 'Nondeterminism' },
      { id: 'evals', label: 'Evals' },
      { id: 'playground', label: 'The playground' },
      { id: 'agents', label: 'An agent loop' },
    ],
  },
  {
    group: 'Interop',
    items: [
      { id: 'sdks', label: 'Generated SDKs' },
      { id: 'host-functions', label: 'Host functions' },
    ],
  },
  {
    group: 'Toolchain',
    items: [
      { id: 'describe', label: 'baml describe' },
      { id: 'namespaces', label: 'Namespaces' },
      { id: 'pack', label: 'baml pack' },
    ],
  },
  {
    group: 'Project',
    items: [{ id: 'status', label: 'Status' }],
  },
];

function Doc({
  id,
  title,
  children,
}: {
  id: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <section id={id}>
      <h2>{title}</h2>
      {children}
    </section>
  );
}

export function Tour() {
  const [active, setActive] = useState('overview');

  // Scroll spy: a thin IntersectionObserver band near the top of the viewport
  // decides the "current" section. Ref callback with cleanup — no useEffect.
  const spyRef = useCallback((node: HTMLElement | null) => {
    if (!node) return undefined;
    const sections = Array.from(
      node.querySelectorAll<HTMLElement>('section[id]'),
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
        if (best) setActive(best);
      },
      { rootMargin: '0px 0px -75% 0px' },
    );
    for (const s of sections) io.observe(s);
    return () => io.disconnect();
  }, []);

  return (
    <div className="l5">
      <nav aria-label="Language tour" className="l5-nav">
        <a className="l5-nav-mark font-mono" href="/">
          BAML
        </a>
        <p className="l5-nav-sub">Language tour</p>
        {NAV.map((g) => (
          <div key={g.group}>
            <p className="l5-nav-group font-mono">{g.group}</p>
            <ul>
              {g.items.map((item) => (
                <li key={item.id}>
                  <a
                    className={active === item.id ? 'l5-active' : undefined}
                    href={`#${item.id}`}
                  >
                    {item.label}
                  </a>
                </li>
              ))}
            </ul>
          </div>
        ))}
        <a
          className="l5-nav-quickstart font-mono"
          href="https://new.boundaryml.com/quickstart"
          rel="noreferrer"
          target="_blank"
        >
          quickstart →
        </a>
      </nav>

      <main className="l5-main" ref={spyRef}>
        <div className="l5-doc">
          <Doc id="overview" title="BAML">
            <p className="l5-lead">
              BAML is a programming language for AI software. An LLM call is a
              typed function: the return type is the schema sent to the model,
              the parser for its reply, and the type your code receives.
            </p>
            <p>
              The compiler, VM, test runner, and language server ship as one
              binary. The same toolchain runs in this page — every editor below
              is live. Hover for types, click <b>▶ Run test</b> to execute, and
              edit anything.
            </p>
            <div className="l5-block">
              <Terminal lines={['brew install boundaryml/tap/baml']} />
            </div>
            <p className="l5-meta">
              This page is a single-file tour of the language. It reads top to
              bottom, but every section stands alone.
            </p>
          </Doc>

          <Doc id="basics" title="Functions and classes">
            <p>
              BAML reads like TypeScript. Classes hold typed fields and methods;
              a method without <code>self</code> is a factory, called as{' '}
              <code>Greeting.new(...)</code>. The last expression of a function
              is its return value — no <code>return</code> needed. Tests are
              part of the language.
            </p>
            <div className="l5-block">
              <BamlEditor filename="greeting.baml" initialCode={BAML_HELLO} />
            </div>
          </Doc>

          <Doc id="llm-functions" title="LLM functions">
            <p>
              A function whose body is a <code>client</code> and a{' '}
              <code>prompt</code> calls a model. The return type does three
              jobs: it is the schema shown to the model (via{' '}
              <code>{'{{ ctx.output_format }}'}</code>), the parser for the
              reply, and the static type at every call site.
            </p>
            <div className="l5-block">
              <BamlEditor
                filename="classify.baml"
                highlightLines={[8, 14]}
                initialCode={BAML_SENTIMENT}
              />
            </div>
            <p className="l5-meta">
              Replies go through schema-aligned parsing: malformed JSON —
              trailing commas, missing quotes, prose around the payload — is
              repaired and coerced into <code>Verdict</code>, or fails with a
              typed error. There is no <code>any</code> in BAML; there is{' '}
              <code>unknown</code>, and the compiler makes you handle it.
            </p>
          </Doc>

          <Doc id="errors" title="Error handling">
            <p>
              You never write <code>throws</code> on the functions you call —
              the compiler infers the complete error set of every function, and{' '}
              <code>catch</code> matches on the error type. Arms that can never
              fire are flagged as dead code.
            </p>
            <div className="l5-block">
              <BamlEditor
                filename="fetch.baml"
                initialCode={BAML_UNREACHABLE}
              />
            </div>
            <p className="l5-meta">
              The warning above is real: the compiler proves the{' '}
              <code>ParseError</code> arm is unreachable. Declaring{' '}
              <code>throws ParseError</code> on <code>fetch_page</code> is also
              an error — a declaration may not hide what the function actually
              throws.
            </p>
          </Doc>

          <Doc id="concurrency" title="Concurrency">
            <p>
              <code>spawn</code> runs any function concurrently and returns a
              future; <code>await</code> joins it. There is no{' '}
              <code>async</code> keyword, so adding concurrency inside a
              function never changes its signature or its callers.
            </p>
            <div className="l5-block">
              <BamlEditor
                filename="spawn.baml"
                highlightLines={[6, 11]}
                initialCode={BAML_SPAWN}
              />
            </div>
            <p>
              Spawned tasks are scheduled across cores, so this works for
              CPU-bound work as well as IO:
            </p>
            <div className="l5-block l5-figure">
              <CoreUsage />
            </div>
            <h3>Task groups</h3>
            <p>
              A <code>TaskGroup</code> caps how many tasks run at once; extras
              queue in order, and the group cancels as a unit. The future's
              error side carries its body's error set, so <code>catch</code> at
              the await site is type-checked too.
            </p>
            <div className="l5-block">
              <BamlEditor
                filename="shards.baml"
                highlightLines={[3, 5, 17]}
                initialCode={BAML_SPAWN_ADV}
              />
            </div>
          </Doc>

          <Doc id="tests" title="Tests">
            <p>
              <code>test</code> and <code>testset</code> are language
              constructs, not a separate framework. A test is a block of
              ordinary code with asserts; the editor's <b>▶ Run test</b> lens
              and <code>baml test</code> run the same thing.
            </p>
            <div className="l5-block">
              <BamlEditor filename="tests.baml" initialCode={BAML_TEST} />
            </div>
          </Doc>

          <Doc id="nondeterminism" title="Nondeterminism">
            <p>
              If a function calls a model — even several layers down — its
              output can change from run to run.{' '}
              <code>assert.equal(output, expected)</code> stops working for
              everything above the call, and mainstream languages give you no
              warning about which functions those are.
            </p>
            <div className="l5-block l5-figure">
              <InfectionGraph />
            </div>
            <p className="l5-meta">
              BAML's answer is to make sampled, statistical testing as cheap as
              exact testing — see Evals, next.
            </p>
          </Doc>

          <Doc id="evals" title="Evals">
            <p>
              Evals are code, not configuration. Anything from an LLM judge to a
              sampled pass-rate is a typed function and an assert — with
              compiler errors when the code under test changes, and git for
              history.
            </p>
            <div className="l5-block">
              <BamlEditor
                filename="evals.baml"
                highlightLines={[5, 6]}
                initialCode={BAML_PASSRATE}
              />
            </div>
          </Doc>

          <Doc id="playground" title="The playground">
            <p>
              The toolchain includes a playground: pick a function, edit its
              arguments, run it, and inspect the call graph, the rendered
              prompt, and the equivalent curl. The compiler knows every call in
              the program, so the graph view is exact and never goes stale.
            </p>
            <p>
              This one is live. The pipeline generates an image, then has a
              second model describe it — edit the code and run{' '}
              <code>illustrate</code>.
            </p>
            <div className="l5-block">
              <LivePlayground
                initialCode={BAML_IMAGE}
                initialFunction="illustrate"
              />
            </div>
          </Doc>

          <Doc id="agents" title="An agent loop">
            <p>
              An agent is a typed turn loop: the model returns a{' '}
              <code>Step</code>, a <code>match</code> dispatches it to a tool
              function, and the result is appended to the transcript. Tools are
              plain functions; the loop below is bounded at eight steps.
            </p>
            <div className="l5-block">
              <LivePlayground
                initialCode={BAML_AGENT}
                initialFunction="run_turn"
              />
            </div>
          </Doc>

          <Doc id="sdks" title="Generated SDKs">
            <p>
              <code>baml generate</code> writes a typed client into an existing
              Python or TypeScript project, so you can adopt BAML one function
              at a time. A BAML class comes out as a pydantic model in Python
              and a typed class in TypeScript — methods included. Nothing else
              about your app changes.
            </p>
            <div className="l5-block">
              <BamlCode code={PY_SDK} filename="main.py" lang="python" />
            </div>
            <p className="l5-meta">
              Python and TypeScript are first-class today; Go and Ruby are in
              progress. The runtime also embeds the other way — this page runs
              the full compiler, VM, and language server in a browser worker.
            </p>
          </Doc>

          <Doc id="host-functions" title="Host functions">
            <p>
              The boundary runs both directions. A BAML function can take a
              function-typed parameter, and the host fills it with a plain
              Python function — BAML calls back into your code mid-run. An
              exception raised inside the callback surfaces back as the
              identical host object.
            </p>
            <div className="l5-pair">
              <div>
                <p className="l5-pane-label">baml</p>
                <BamlEditor
                  filename="tally.baml"
                  highlightLines={[2]}
                  initialCode={BAML_TALLY}
                />
              </div>
              <div>
                <p className="l5-pane-label">python</p>
                <BamlCode
                  code={PY_CALLBACK}
                  filename="app.py"
                  highlightLines={[6]}
                  lang="python"
                />
              </div>
            </div>
          </Doc>

          <Doc id="describe" title="baml describe">
            <p>
              <code>baml describe &lt;name&gt;</code> answers in one call what
              takes an agent several greps and a file read: the signature, the
              source, the dependencies, and every reference. It is generated
              from source, so it cannot drift, and it also answers for the
              stdlib and the grammar itself. <code>--budget</code> bounds the
              output to fit a context window.
            </p>
            <div className="l5-block">
              <TermPlay events={DESCRIBE_EVENTS} title="baml describe" />
            </div>
          </Doc>

          <Doc id="namespaces" title="Namespaces">
            <p>
              The directory layout of <code>baml_src/</code> is the namespace
              tree, and aliasing does not exist: a cross-namespace reference is
              always written in full, so every reference to a type greps. The
              compiler's errors teach the canonical name.
            </p>
            <div className="l5-pair">
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
              <BamlCode
                code={NS_GOOD}
                filename="ns_b/b.baml"
                highlightLines={[2, 3]}
              />
            </div>
            <div className="l5-block">
              <TermPlay events={LS_EVENTS} title="the filesystem is the map" />
            </div>
          </Doc>

          <Doc id="pack" title="baml pack">
            <p>
              <code>baml pack</code> compiles any set of functions into a
              self-contained binary — about 10&nbsp;MB, built in a fraction of a
              second. The CLI is derived from the function signatures: each
              parameter becomes a typed, documented flag, and{' '}
              <code>--help</code> is generated for free.
            </p>
            <div className="l5-pair">
              <div>
                <p className="l5-pane-label">the source</p>
                <BamlCode
                  code={BAML_PACKED}
                  filename="main.baml"
                  highlightLines={[1]}
                  notes={[{ line: 1, text: 'name: string → --name <flag>' }]}
                />
              </div>
              <div>
                <p className="l5-pane-label">pack it, run it</p>
                <TermPlay events={PACK_EVENTS} title="baml pack" />
              </div>
            </div>
          </Doc>

          <Doc id="status" title="Status">
            <p>What you are signing up for, plainly:</p>
            <ul className="l5-list">
              <li>
                A new language. Models have not seen much BAML in training; the
                toolchain (<code>describe</code>, generated docs) compensates,
                but it is a real cost.
              </li>
              <li>
                Two first-class SDK targets today — Python and TypeScript. Go
                and Ruby are in progress.
              </li>
              <li>Pre-1.0: the language still changes.</li>
            </ul>
            <p>
              Adoption is incremental by construction: one function behind a
              generated client. Your app does not move; one call site does.
            </p>
            <div className="l5-block">
              <Terminal lines={['brew install boundaryml/tap/baml']} />
            </div>
            <p>
              <a
                className="l5-link"
                href="https://new.boundaryml.com/quickstart"
                rel="noreferrer"
                target="_blank"
              >
                new.boundaryml.com/quickstart →
              </a>
            </p>
          </Doc>
        </div>
      </main>
    </div>
  );
}
