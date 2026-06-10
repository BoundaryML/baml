'use client';

import Image from 'next/image';
import { type ReactNode, useState } from 'react';
import { BamlCode } from '../../learn2/_components/BamlCode';
import BamlEditor from '../../learn2/_components/BamlEditorLazy';
import LivePlayground from '../../learn2/_components/LivePlaygroundLazy';
import { Terminal } from '../../learn2/_components/primitives';
import { InfectionGraph } from '../../learn3/_components/InfectionGraph';
import { MetricsDag } from '../../learn3/_components/MetricsDag';
import { TermPlay } from '../../learn3/_components/TermPlay';
import { SdkPipeline } from '../../learn4/_components/SdkPipeline';
import { PackChart, SpawnChart } from './PackChart';
import { Scheduler } from './Scheduler';
import { SdkSwitcher } from './SdkSwitcher';
import { SelfImprove } from './SelfImprove';
import {
  BAML_CSV_TESTS,
  BAML_HTTP_TESTS,
  BAML_IMAGE,
  BAML_MATCH,
  BAML_METRIC,
  BAML_PACKED,
  BAML_RUNNER,
  BAML_SPAWN,
  BAML_TEST,
  BAML_UNKNOWN,
  BAML_UNREACHABLE,
  BENCH_BAML,
  DESCRIBE_EVENTS,
  GREP_EVENTS,
  LS_EVENTS,
  NS_BAD,
  NS_GOOD,
  PACK_EVENTS,
  RUN_E_EVENTS,
  TS_CATCH,
  TS_INSTANCEOF,
  TS_LIES,
} from './snippets';
import { TypeTabs } from './TypeTabs';

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
  num,
  title,
  children,
}: {
  num?: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="l6-sub">
      <h3>
        {num ? <span className="l6-num font-mono">{num}</span> : null}
        {title}
      </h3>
      {children}
    </div>
  );
}

/* "Try it out!" install tabs — humans get brew, agents get the plugin
 * commands (mirrors the homepage hero's install paths). */
const TRY_TABS = [
  {
    id: 'humans',
    label: 'for humans',
    lines: ['brew install boundaryml/tap/baml'],
  },
  {
    id: 'agents',
    label: 'for agents',
    lines: [
      '/plugin marketplace add BoundaryML/baml-skill',
      '/plugin install baml@boundaryml-baml',
    ],
  },
] as const;

function TryItTabs() {
  const [tab, setTab] = useState<'humans' | 'agents'>('humans');
  const active = TRY_TABS.find((t) => t.id === tab) ?? TRY_TABS[0];
  return (
    <div className="l6-block">
      <div aria-label="Install path" className="l6-sdk-tabs" role="tablist">
        {TRY_TABS.map((t) => (
          <button
            aria-selected={tab === t.id}
            className={`l6-sdk-tab font-mono${tab === t.id ? ' l6-sdk-tab--on' : ''}`}
            key={t.id}
            onClick={() => setTab(t.id)}
            role="tab"
            type="button"
          >
            {t.label}
          </button>
        ))}
      </div>
      <Terminal lines={[...active.lines]} />
    </div>
  );
}

export function Article() {
  return (
    <div className="l6">
      <header className="l6-head">
        <a className="font-mono" href="/">
          BAML <span>· the programming language for agents</span>
        </a>
        <span className="l6-head-install font-mono">
          brew install boundaryml/tap/baml
        </span>
      </header>

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
        <p className="l6-lead">{'BAML is meant to be written by agents'}</p>
        <p>
          {
            'Every language feature is meant to prevent context pollution and churn when coding with AI. We opt for features that make agents make less mistakes at runtime (like Rust), but without fighting the borrow-checker. BAML should still be comprehensible to the millions of non-technical people now coding with AI.'
          }
        </p>
        <p>
          {
            'Our goal is to make BAML feel like TypeScript, but without the sins of Javascript: with better error handling, without type-erasure, no '
          }
          <code>any</code>
          {', and more.'}
        </p>
        <p>{'Here’s a few of the agent-centric language features:'}</p>
      </section>

      {/* ---- 1 · type system ---- */}
      <Section
        id="types"
        num="1"
        title="A type-system like TypeScript, but as reliable as Rust"
      >
        <p>
          {
            'BAML supports advanced features like generics on day 1. Types also exist at runtime, so you don’t need to choose between 5 different schema validation libraries. Your objects always match their annotated type. And there is no '
          }
          <code>any</code>
          {' — all code must be fully typed.'}
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
                  line: 10,
                  message: 'TypeError: undefined — at runtime, far from here',
                  severity: 'error',
                },
              ]}
              filename="load.ts"
              highlightLines={[7]}
              lang="typescript"
            />
          </div>
          <div>
            <p className="l6-pane-label l6-pane-label--after">baml</p>
            <BamlEditor filename="load.baml" initialCode={BAML_UNKNOWN} />
          </div>
        </div>

        <Sub title="Better error handling">
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
                highlightLines={[3]}
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
        </Sub>

        <Sub title="Match on types, or values">
          <p>
            {'Any of ’em work. No need for '}
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
                highlightLines={[3, 5, 7]}
                lang="typescript"
              />
            </div>
            <div>
              <p className="l6-pane-label l6-pane-label--after">baml</p>
              <BamlEditor filename="match.baml" initialCode={BAML_MATCH} />
            </div>
          </div>
        </Sub>
      </Section>

      {/* ---- 2 · namespaces ---- */}
      <Section
        id="namespaces"
        num="2"
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

      {/* ---- 3 · native testing ---- */}
      <Section id="testing" num="3" title="Native testing framework">
        <p>
          {'Write tests anywhere, in any file. (More on testing '}
          <a className="l6-link" href="#workflows">
            below
          </a>
          {'.)'}
        </p>
        <div className="l6-block">
          <BamlEditor filename="tests.baml" initialCode={BAML_TEST} />
        </div>
      </Section>

      {/* ---- 4 · describe ---- */}
      <Section
        id="describe"
        num="4"
        title="baml describe — a built-in AST-based grep, to find things faster"
      >
        <p>
          <code>describe</code>
          {
            ' is easier for agents to use than an LSP, and more informative than grep. Here’s a transcript of an agent searching with grep, versus with baml describe:'
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
            <TermPlay events={DESCRIBE_EVENTS} title="agent with describe" />
          </div>
        </div>
        <p className="l6-note">
          {
            'The reference list is the part grep can’t give you: every call site, resolved — handy for spotting near-duplicates before writing a second copy of a function. We’ll keep making improvements to this tool.'
          }
        </p>
      </Section>

      {/* ---- 5 · pack ---- */}
      <Section
        id="pack"
        num="5"
        title="baml pack — ship a function as a tiny binary"
      >
        <p>
          {
            'BAML pack is a CLI that takes your baml program and auto-creates a CLI for you from the function signature. It can compile and run on any target architecture.'
          }
        </p>
        <div className="l6-pair">
          <div>
            <p className="l6-pane-label">the source — one parameter</p>
            <BamlCode
              code={BAML_PACKED}
              filename="main.baml"
              highlightLines={[1]}
              notes={[{ line: 1, text: 'name: string → --name <flag>' }]}
            />
          </div>
          <div>
            <p className="l6-pane-label l6-pane-label--after">
              pack it, run it, ask it for help
            </p>
            <TermPlay events={PACK_EVENTS} title="baml pack" />
          </div>
        </div>
        <Sub title="The packed binary is 87% smaller than Bun’s, and starts ~30% faster">
          <p>
            {
              'Here’s a comparison of BAML vs Bun in creating a compiled binary — the same hello world, measured back-to-back on an idle machine (median of 20 runs). The binary size is just 7.9 MB:'
            }
          </p>
          <PackChart />
          <p className="l6-dim">
            {
              'Bun 1.3.14, BAML release toolchain, aarch64-apple-darwin. Bun embeds a whole JavaScript engine; the BAML runtime is 7.9 MB.'
            }
          </p>
        </Sub>
      </Section>

      {/* ---- 6 · run -e ---- */}
      <Section
        id="run-e"
        num="6"
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

      {/* ---- 7 · green threads ---- */}
      <Section
        id="threads"
        num="7"
        title="Green threads a.k.a ‘make any function run in parallel’"
      >
        <p>
          {'Like Go, BAML supports lightweight green threads via '}
          <code>spawn</code>
          {' and '}
          <code>await</code>
          {'. Run any function asynchronously without having to write '}
          <code>async function</code>
          {
            ' in 10 other files everywhere. Easy to parallelize slow LLM http requests and tool calls.'
          }
        </p>
        <div className="l6-block l6-breakout">
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

        <Sub title="spawn can run cpu-bound code in parallel">
          <p>
            {
              'This is the part Promise.all cannot do: JavaScript fans out I/O, but compute still shares one thread. We scanned 38 GB of log-like text for an error marker — 16 shards of ~48 MB, each scanned 50 times — with the same code in both runtimes:'
            }
          </p>
          <SpawnChart />
          <p className="l6-dim">
            {
              'BAML’s stdlib string search is native Rust, so even one thread edges out Bun here — and spawn turns the same code into a 9× improvement. (The one place Bun still wins per core is tight arithmetic loops, where its JIT beats our interpreter.)'
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
            'We also have built-in primitives for managing concurrency (task groups etc), which we’ll get to later!'
          }
        </p>
        <Scheduler />
      </Section>

      {/* ---- 8 · supply chain ---- */}
      <Section id="supply-chain" num="8" title="No supply chain attacks">
        <p>
          {'Okay, to be fair, BAML doesn’t '}
          <em>yet</em>
          {
            ' have a package manager. We’re working on it! In the meantime, just make AI agents write all the code you need.'
          }
        </p>
      </Section>

      {/* ---- self improvement ---- */}
      <Section id="self-improvement" num="9" title="Recursive self-improvement">
        <p>
          {
            'We take a data-driven approach to improving BAML, using feedback from agents themselves. We built '
          }
          <a
            className="l6-link"
            href="https://bench3-ui.fly.dev/"
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
            href="https://bench3-ui.fly.dev/cohorts"
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
            'We aim to keep BAML stable for production uses — but feel free to join our language experiments if you’re curious about this process.'
          }
        </p>
        <SelfImprove />
      </Section>

      {/* ---- AI workflows ---- */}
      <Section id="workflows" title="BAML for AI workflows">
        <p>
          {'Writing code is one thing, but in the future '}
          <em>every</em>
          {
            ' software program will interact with non-deterministic AI code. Whilst BAML supports writing anything from a web-server to a data-processing library, our main focus is to provide primitives to help teams deal with nondeterminism. To do this we made sure BAML programs are observable, testable, and measurable.'
          }
        </p>
        <InfectionGraph />
        <p style={{ marginTop: '1.2rem' }}>{'Here are some highlights:'}</p>

        <Sub
          num="1"
          title="Native LLM Functions — composable building blocks for agents and harnesses"
        >
          <p>
            {
              'An LLM call in BAML is just a function: the prompt is the body, the return type is the schema. Because it’s a real function, it can be evaluated, optimized, and tracked at runtime by observability platforms.'
            }
          </p>
          <p>
            {
              'If you’ve used BAML in the last 2 years, you’ll be happy to hear we still have our error-correcting JSON parser — super useful for working with small language models.'
            }
          </p>
          <p>
            {
              'BAML ships with tooling to observe LLM function inputs and outputs, like our workflow visualizer in VSCode. It’s especially helpful when working with multimodal outputs, like images.'
            }
          </p>
          <div className="l6-breakout l6-breakout--xl">
            <LivePlayground
              initialCode={BAML_IMAGE}
              initialFunction="illustrate"
            />
          </div>
        </Sub>

        <Sub num="2" title="BAML Tests">
          <p>{'Write tests anywhere, in any file.'}</p>
          <p>
            {
              'Create arbitrary groups and add tests dynamically — generate tests for each item in an array, create tests from a CSV file, or from S3:'
            }
          </p>
          <div className="l6-pair">
            <div>
              <p className="l6-pane-label l6-pane-label--after">
                tests from a csv · run it here
              </p>
              <BamlEditor
                filename="csv_tests.baml"
                initialCode={BAML_CSV_TESTS}
              />
            </div>
            <div className="l6-stackv">
              <div>
                <p className="l6-pane-label">or from S3, at collection time</p>
                <BamlCode code={BAML_HTTP_TESTS} filename="golden_tests.baml" />
              </div>
              <p className="l6-dim">
                {
                  'View tests in the Playground — in case a human needs to see things, we have nice utilities — or just have agents run '
                }
                <code>baml test</code>
                {'.'}
              </p>
            </div>
          </div>
          <p>
            {
              'Create evals — LLM-as-judge, statistical analysis, etc. In other frameworks that’s a YAML schema and a hosted UI. In BAML, it’s all just code. Pass a test when at least N% of runs do, using custom test runners:'
            }
          </p>
          <div className="l6-block l6-breakout">
            <BamlEditor filename="evals.baml" initialCode={BAML_RUNNER} />
          </div>
          <p className="l6-note">
            {
              'Custom test runners go further: retries, uploading reports, running things in parallel or synchronously.'
            }
          </p>
        </Sub>

        <Sub num="3" title="Built-in metrics primitive (design stage)">
          <p>
            {
              'Metrics today live in dashboards, bound to code by strings — rename a function and the metric dies silently. We are designing metric blocks: attach one to a function and it carries typed measurements, wired into a dependency graph that computes as data arrives — even hours later, when a human label shows up.'
            }
          </p>
          <MetricsDag />
          <div className="l6-pair" style={{ marginTop: '1.2rem' }}>
            <BamlCode
              code={BAML_METRIC}
              filename="resume.baml (proposed)"
              highlightLines={[10, 15]}
            />
            <p className="l6-dim" style={{ marginTop: 0 }}>
              {
                'Parameter names are the edges of the graph: quality(judge) depends on judge; f1(precision, recall) waits for both. The function call is the root. This is the part we want torn apart — the runtime already records a trace of every call; this design is the layer on top.'
              }
            </p>
          </div>
        </Sub>
      </Section>

      {/* ---- incremental adoption ---- */}
      <Section id="adoption" title="Incremental Adoption">
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
            'The types come out native. A BAML class is a pydantic model in Python and a typed class in TypeScript — methods included:'
          }
        </p>
        <TypeTabs />
        <p>
          {
            'Here’s what that looks like in Node, Python, Go, Rust (with many more supported):'
          }
        </p>
        <div className="l6-breakout">
          <SdkSwitcher />
        </div>
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
    </div>
  );
}
