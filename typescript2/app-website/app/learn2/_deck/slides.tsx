'use client';

import Image from 'next/image';
import { BamlCode } from '../_components/BamlCode';
import BamlEditor from '../_components/BamlEditorLazy';
import LivePlayground from '../_components/LivePlaygroundLazy';
import { SpawnRace } from '../_components/SpawnRace';
import {
  Bullets,
  Callout,
  Lead,
  Quote,
  SectionDivider,
  SlideShell,
  Split,
  Terminal,
} from '../_components/primitives';
import type { Slide } from '../_lib/types';

/* ------------------------------------------------------------------ *
 * Code snippets. Most are lifted/adapted from ~/projects/baml-demos.
 * NOTE: the demos use `field: Type` (colon) class syntax; the local
 * baml2 compiler stdlib uses `field Type` (no colon). Snippet dialect +
 * which runtime we run them against is an open decision (see checkpoint).
 * ------------------------------------------------------------------ */

const PY_FAKE_TYPES = `def add_numbers(a: int, b: int) -> int:
    return a + b

# Python type hints are a suggestion, not a guarantee:
add_numbers("hello ", "world")   # runs fine -> "hello world"`;

const TS_FAKE_TYPES = `const res  = await fetch(url);
const data = await res.json();   // : any  — types gone

data.user.naem;                  // typo: still compiles
typeof data;                     // "object" — every type erased`;

const BAML_SENTIMENT = `type Label = "positive" | "negative" | "neutral";

class Verdict {
  label: Label,
  confidence: float,
}

function classify(text: string) -> Verdict {
  client: "openai/gpt5.5"
  prompt: #"
    Classify the sentiment of the text. Sarcasm counts
    as the sentiment actually expressed.
    Text: {{ text }}
    {{ ctx.output_format }}
  "#
}`;

const BAML_IMAGE = `function generate_image(thing: string) -> image {
  client: AiGatewayImagen
  prompt: #"
    Create an image from this prompt: {{ thing }}
    {{ ctx.output_format }}
  "#
}

function describe(img: image) -> string {
  client: "openai/gpt5.5"
  prompt: #"
    Describe this image in one vivid sentence.
    {{ img }}
    {{ ctx.output_format }}
  "#
}

// the pipeline: generate an image, then have an LLM describe it
function illustrate(thing: string) -> string {
  let img = generate_image(thing);
  describe(img)
}

client AiGatewayImagen {
  provider: ai-gateway-images,
  options: {
    model: "google/imagen-4.0-fast-generate-001",
    api_key: env.AI_GATEWAY_API_KEY,
  }
}`;

const BAML_AGENT = `function run_turn(history: string, msg: string) -> Turn {
  let transcript = start(history, msg);
  let steps = 0;
  while (steps < max_steps) {
    steps = steps + 1;
    let step = decide(transcript);        // an LLM call
    if (step.action == "respond") { return done(step) };
    let result = execute(step);           // dispatch a tool
    transcript = observe(transcript, step, result);
  }
  giveup(transcript)
}`;

const SH_DESCRIBE = `$ baml describe classify --budget 12
── function ── sentiment.baml:8
shape: function classify(text: string) -> Verdict
deps: Verdict, Label
refs: 3`;

const SH_RUN = `$ baml run classify -e '"late shipping, but support fixed it"'
Verdict { label: "neutral", confidence: 0.62 }

$ baml fmt sentiment.baml          # format in place
$ baml generate                    # -> typed python/ts client (baml_sdk)`;

const BAML_SPAWN = `function serve() -> null {
  let listener = baml.net.listen("127.0.0.1:8080");
  while (true) {
    let sock = listener.accept();
    let _ = spawn { handle(sock) };   // no async, no await, no color
  }
  null
}`;

const TS_COLOR = `async function serve() {        // 'async' colors this fn...
  for await (const sock of listener) {
    handle(sock);                 // ...and every caller, all the way up
  }
}`;

const BAML_TEST = `testset "basics" {
  test "clearly positive" {
    let v = classify("absolutely loved it!");
    assert.equal(v.label, "positive");
  }
}`;

const BAML_JUDGE = `function judge(text: string, label: Label) -> Judgement {
  client: "openai/gpt5.5"
  prompt: #"
    A classifier labeled this "{{ label }}". Text: {{ text }}
    Would a careful reader accept that? {{ ctx.output_format }}
  "#
}

testset "from_file" {                       // one test per row of a file
  for (let c in load_cases("cases.json")) {
    test c.name { assert.equal(classify(c.text).label, c.expected) }
  }
}`;

const BAML_WITH = `test "mostly passes"   with PassRate(0.9)   { /* ... */ }
testset "flaky-ok"     with WithRetry(3)    { /* ... */ }
testset "consensus"    with WithQuorum(5,3) { /* ... */ }`;

const BAML_TYPES = `type Label = "positive" | "negative" | "neutral";

// recursive types are fine — the compiler guards the cycle:
type JSON = int | float | string | bool
          | JSON[] | map<string, JSON>;`;

const BAML_CLASSES = `class Greeting {
  message: string,
  letters: int,

  function new(name: string) -> Greeting {     // static (no self)
    Greeting { message: "hi, " + name, letters: name.length() }
  }
  function shout(self) -> string {             // instance (has self)
    self.message.to_upper_case()
  }
}

let g = Greeting.new("vaibhav");   // fully-qualified, no aliases`;

const BAML_ERRORS = `function build() -> string throws string | baml.errors.Io {
  let res = baml.sys.shell("baml generate");
  if (res.exit_code != 0) { throw "generate failed" };
  "ok"
}

let out = build() catch (e) {
  _: string => "recovered: " + e,
};`;

const PY_EMBED = `from baml_sdk import b               # generated, typed client

g = b.greet("vaibhav")              # call a BAML function
print(g.shout())                    # instance method on the result

verdict = await b.classify_async("loved it")   # async twin, free`;

/* ------------------------------------------------------------------ */

export function getSlides(): Slide[] {
  return [
    {
      id: 'cover',
      section: 'Intro',
      title: 'BAML',
      node: (
        <div className="l2-cover">
          <div className="l2-cover-mark">
            <Image
              src="/baml-sheep.png"
              alt=""
              width={128}
              height={128}
              className="l2-cover-sheep"
              priority
            />
            <h1 className="l2-cover-title">BAML</h1>
          </div>
          <p className="l2-cover-sub">
            {
              'Typed software for LLMs — classes, control flow, typed errors, concurrency, and tests in one language.'
            }
          </p>
          <p className="l2-cover-hint font-mono">
            {'press → or space to begin'}
          </p>
        </div>
      ),
    },

    {
      id: 'agenda',
      section: 'Intro',
      title: 'Agenda',
      node: (
        <SlideShell kicker="Agenda" title="Where we are headed">
          <Bullets
            items={[
              'Why did we make a programming language?',
              'What is BAML great for?',
              'Visualization tools',
              'How agents read & write BAML',
              'Core language features — parallelism, evals, and more',
            ]}
          />
        </SlideShell>
      ),
    },

    {
      id: 'why-language',
      section: 'Why',
      title: 'Why a new language?',
      node: (
        <SlideShell kicker="Why" title="Why make a new language?">
          <Bullets
            items={[
              'Native code visualization',
              'First-class observability primitives',
              'Native testing / evals',
            ]}
          />
          <Quote>{'Existing languages weren’t built with LLMs in mind.'}</Quote>
        </SlideShell>
      ),
    },

    {
      id: 'fake-types',
      section: 'Why',
      title: 'Why not just improve Python / TS?',
      node: (
        <SlideShell
          wide
          kicker="Why · types"
          title="Types are the basis for observability"
        >
          <Lead>
            {
              'Python and TypeScript have “fake types.” The compiler will gladly accept lies.'
            }
          </Lead>
          <div className="l2-stack">
            <div className="l2-example">
              <p className="l2-example-label font-mono">
                Python — the hint is ignored at runtime
              </p>
              <BamlCode
                lang="python"
                filename="add.py"
                code={PY_FAKE_TYPES}
                diagnostics={[
                  {
                    line: 4,
                    severity: 'warning',
                    message: 'no error — runs fine',
                  },
                ]}
              />
            </div>
            <div className="l2-example">
              <p className="l2-example-label font-mono">
                TypeScript — every type is erased after compile
              </p>
              <BamlCode
                lang="typescript"
                filename="fetch.ts"
                code={TS_FAKE_TYPES}
                diagnostics={[
                  {
                    line: 2,
                    severity: 'warning',
                    message: 'any swallows everything downstream',
                  },
                  {
                    line: 4,
                    severity: 'error',
                    message: 'ships to prod, crashes at runtime',
                  },
                ]}
              />
            </div>
          </div>
        </SlideShell>
      ),
    },

    {
      id: 'great-for',
      section: 'What it is',
      title: 'What is BAML great for?',
      node: (
        <SlideShell kicker="What it is" title="What is BAML great for?">
          <Lead>{'Systems that need to be highly observable.'}</Lead>
          <Bullets
            items={[
              'Nondeterministic functions — like agents',
              'Codebases mostly written by AI, where humans reviewed only ~30% of the code',
            ]}
          />
        </SlideShell>
      ),
    },

    {
      id: 'three-things',
      section: 'What it is',
      title: 'Three things we need',
      node: (
        <SlideShell kicker="What it is" title="To get there, three things">
          <Bullets
            items={[
              '1 — An observability layer perfectly connected to the language',
              '2 — A language for AI models — easy to read, write, describe',
              '3 — Incremental adoption, so you don’t rewrite your whole app',
            ]}
          />
        </SlideShell>
      ),
    },

    {
      id: 'roadmap',
      section: 'Roadmap',
      title: 'What this talk covers',
      node: (
        <SlideShell kicker="Roadmap" title="What we’ll cover">
          <Bullets
            items={[
              'The first of our observability tools — playground visualization',
              'How the language is easy for models to pick up',
              'How incremental adoption works',
              'Agentic techniques that keep BAML evolving — and how you can contribute',
            ]}
          />
        </SlideShell>
      ),
    },

    {
      id: 'section-viz',
      section: 'Visualization',
      title: 'Playground Visualization',
      node: (
        <SectionDivider
          index="Section 1"
          title="Playground Visualization"
          blurb="See the pipeline, not just the prompt."
        />
      ),
    },

    {
      id: 'just-typescript',
      section: 'Visualization',
      title: 'It reads like TypeScript',
      node: (
        <SlideShell
          wide
          kicker="Visualization"
          title="It reads like a language you know"
        >
          <Split
            left={
              <>
                <Lead>
                  {'A function, a class, a literal union. That’s it.'}
                </Lead>
                <Bullets
                  items={[
                    'Structured output is just the return type',
                    'Prompts are first-class, with typed template values',
                  ]}
                />
              </>
            }
            right={
              <BamlEditor
                filename="sentiment.baml"
                initialCode={BAML_SENTIMENT}
              />
            }
          />
        </SlideShell>
      ),
    },

    {
      id: 'image-pipeline',
      section: 'Visualization',
      title: 'A simple pipeline',
      node: (
        <SlideShell wide kicker="Visualization" title="A simple pipeline">
          <Lead>
            {
              'Generate an image, then have an LLM describe it — one typed pipeline. Edit it, then run it live.'
            }
          </Lead>
          <LivePlayground initialCode={BAML_IMAGE} />
        </SlideShell>
      ),
    },

    {
      id: 'claude-code',
      section: 'Visualization',
      title: 'A more complex pipeline',
      node: (
        <SlideShell wide kicker="Visualization" title="A more complex pipeline">
          <Split
            left={
              <>
                <Lead>
                  {'“bamlcode” — a tiny Claude Code, written in BAML.'}
                </Lead>
                <Bullets
                  items={[
                    'decide → execute → observe, in a typed loop',
                    'Tools are ordinary functions; dispatch is a match',
                  ]}
                />
              </>
            }
            right={
              <BamlEditor filename="agent.baml" initialCode={BAML_AGENT} />
            }
          />
        </SlideShell>
      ),
    },

    {
      id: 'section-agents',
      section: 'Agents & BAML',
      title: 'How agents read & write BAML',
      node: (
        <SectionDivider
          index="Section 2"
          title="How agents read & write BAML"
          blurb="A language built for a model’s context window."
        />
      ),
    },

    {
      id: 'minified-skill',
      section: 'Agents & BAML',
      title: 'The whole skill fits here',
      node: (
        <SlideShell kicker="Agents & BAML" title="The skill fits in this text">
          <Lead>
            {
              'A model learns BAML from a handful of skill files — small enough to drop into context.'
            }
          </Lead>
          <Bullets
            items={[
              'baml-core — the language essentials',
              'baml-llm-functions, baml-pipelines',
              'baml-testing, baml-bridges (host interop)',
            ]}
          />
        </SlideShell>
      ),
    },

    {
      id: 'baml-describe',
      section: 'Agents & BAML',
      title: 'baml describe',
      node: (
        <SlideShell wide kicker="Agents & BAML" title="baml describe">
          <Split
            left={
              <>
                <Lead>
                  {'No docs to crawl — the toolchain describes itself.'}
                </Lead>
                <Bullets
                  items={[
                    'Describe any symbol, including the standard library',
                    '--budget bounds the output so it fits a token budget',
                  ]}
                />
              </>
            }
            right={
              <BamlCode lang="bash" filename="terminal" code={SH_DESCRIBE} />
            }
          />
        </SlideShell>
      ),
    },

    {
      id: 'baml-run',
      section: 'Agents & BAML',
      title: 'run · fmt · generate',
      node: (
        <SlideShell wide kicker="Agents & BAML" title="run · fmt · generate">
          <Split
            left={
              <Callout tone="warn">
                {
                  'Outline mentions `baml pack <fn>` / `--target` — not in this toolchain yet. Distribute via `baml generate`. (verify before shipping)'
                }
              </Callout>
            }
            right={<BamlCode lang="bash" filename="terminal" code={SH_RUN} />}
          />
        </SlideShell>
      ),
    },

    {
      id: 'section-parallel',
      section: 'Parallelism',
      title: 'Parallelism',
      node: (
        <SectionDivider
          index="Section 3"
          title="Parallelism"
          blurb="Concurrency without the colored functions."
        />
      ),
    },

    {
      id: 'spawn',
      section: 'Parallelism',
      title: 'spawn — no function coloring',
      node: (
        <SlideShell
          wide
          kicker="Parallelism"
          title="spawn, and no function coloring"
        >
          <Split
            left={
              <BamlEditor filename="server.baml" initialCode={BAML_SPAWN} />
            }
            right={
              <BamlCode
                lang="typescript"
                filename="server.ts"
                code={TS_COLOR}
                notes={[
                  { line: 1, text: 'async spreads up the whole call stack' },
                ]}
              />
            }
          />
        </SlideShell>
      ),
    },

    {
      id: 'throughput',
      section: 'Parallelism',
      title: 'Throughput',
      node: (
        <SlideShell
          wide
          kicker="Parallelism"
          title="Throughput: spawn vs async/await"
        >
          <SpawnRace />
        </SlideShell>
      ),
    },

    {
      id: 'section-evals',
      section: 'Evals',
      title: 'Evals',
      node: (
        <SectionDivider
          index="Section 4"
          title="Evals"
          blurb="Tests that live with the code."
        />
      ),
    },

    {
      id: 'test-testset',
      section: 'Evals',
      title: 'test & testset',
      node: (
        <SlideShell wide kicker="Evals" title="test & testset">
          <Split
            left={
              <Bullets
                items={[
                  'test blocks run real (or cached) calls',
                  'testset groups them; assert.* checks results',
                ]}
              />
            }
            right={<BamlEditor filename="tests.baml" initialCode={BAML_TEST} />}
          />
        </SlideShell>
      ),
    },

    {
      id: 'llm-judge',
      section: 'Evals',
      title: 'LLM-as-judge & data-driven tests',
      node: (
        <SlideShell
          wide
          kicker="Evals"
          title="LLM-as-judge, and tests from data"
        >
          <Split
            left={
              <Bullets
                items={[
                  'An LLM judge is just another function + assert',
                  'Load rows from a file — or fetch them over the network',
                ]}
              />
            }
            right={<BamlEditor filename="eval.baml" initialCode={BAML_JUDGE} />}
          />
        </SlideShell>
      ),
    },

    {
      id: 'with-clauses',
      section: 'Evals',
      title: 'with clauses',
      node: (
        <SlideShell wide kicker="Evals" title="Runners: the with clause">
          <Split
            left={
              <Callout tone="warn">
                {
                  'The `with <runner>` clause is real. `PassRate` / `WithRetry` / `WithQuorum` are illustrative — today you supply your own runner lambda. (verify)'
                }
              </Callout>
            }
            right={
              <BamlEditor filename="runners.baml" initialCode={BAML_WITH} />
            }
          />
        </SlideShell>
      ),
    },

    {
      id: 'section-types',
      section: 'Type system',
      title: 'Type system',
      node: (
        <SectionDivider
          index="Section 5"
          title="Type system"
          blurb="Unions, recursion, named everything."
        />
      ),
    },

    {
      id: 'unions',
      section: 'Type system',
      title: 'Unions & recursive types',
      node: (
        <SlideShell wide kicker="Type system" title="Unions & recursive types">
          <Split
            left={
              <Bullets
                items={[
                  'Literal and type unions with |',
                  'Recursive types, with a built-in cycle guard',
                  'No anonymous records — every product type is a named class',
                ]}
              />
            }
            right={
              <BamlEditor filename="types.baml" initialCode={BAML_TYPES} />
            }
          />
        </SlideShell>
      ),
    },

    {
      id: 'classes',
      section: 'Type system',
      title: 'Classes & methods',
      node: (
        <SlideShell wide kicker="Type system" title="Classes, methods, names">
          <Split
            left={
              <Bullets
                items={[
                  'Instance methods take self; static methods don’t',
                  'Fully-qualified names — grep is trivial, no alias confusion',
                  'root means your local package · no global variables',
                ]}
              />
            }
            right={
              <BamlEditor filename="greeting.baml" initialCode={BAML_CLASSES} />
            }
          />
        </SlideShell>
      ),
    },

    {
      id: 'section-errors',
      section: 'Errors',
      title: 'Error handling',
      node: (
        <SectionDivider
          index="Section 6"
          title="Error handling"
          blurb="Errors are types, inferred for you."
        />
      ),
    },

    {
      id: 'typed-errors',
      section: 'Errors',
      title: 'Typed errors',
      node: (
        <SlideShell wide kicker="Errors" title="Errors are part of the type">
          <Split
            left={
              <Bullets
                items={[
                  'throws is inferred across the call graph — even through lambdas',
                  'Catch by type with catch; exhaustively with catch_all',
                ]}
              />
            }
            right={
              <BamlEditor filename="build.baml" initialCode={BAML_ERRORS} />
            }
          />
        </SlideShell>
      ),
    },

    {
      id: 'section-adoption',
      section: 'Adoption',
      title: 'Incremental adoption',
      node: (
        <SectionDivider
          index="Section 7"
          title="Incremental adoption"
          blurb="BAML as an embedded language."
        />
      ),
    },

    {
      id: 'embed-python',
      section: 'Adoption',
      title: 'Just call it from Python',
      node: (
        <SlideShell wide kicker="Adoption" title="Just call it from Python">
          <Split
            left={
              <Bullets
                items={[
                  'Generated, typed client — import and go',
                  'Class instance methods callable from the host',
                  'Sync and async twins generated for you',
                  'Lambdas pass through, errors propagate',
                ]}
              />
            }
            right={<BamlCode lang="python" filename="app.py" code={PY_EMBED} />}
          />
        </SlideShell>
      ),
    },

    {
      id: 'section-start',
      section: 'Getting started',
      title: 'Getting started',
      node: (
        <SectionDivider
          index="Section 8"
          title="Getting started"
          blurb="Five minutes to your first function."
        />
      ),
    },

    {
      id: 'getting-started',
      section: 'Getting started',
      title: 'Try it',
      node: (
        <SlideShell kicker="Getting started" title="Try it now">
          <Bullets
            items={[
              'brew install baml',
              'Start with the Sheep Council walkthrough',
              'new.boundaryml.com/quickstart',
            ]}
          />
        </SlideShell>
      ),
    },

    {
      id: 'agents-better',
      section: 'Roadmap',
      title: 'Making agents better',
      node: (
        <SlideShell kicker="Roadmap" title="How we make agents better, daily">
          <Lead>
            {
              'An agent tries BAML — and the harness improves from what it learns.'
            }
          </Lead>
          <Bullets
            items={[
              'Agents exercise the language and tools end to end',
              'Failures feed back into skills, errors, and docs',
              'You can contribute the harnesses you build',
            ]}
          />
        </SlideShell>
      ),
    },

    {
      id: 'closing',
      section: 'Getting started',
      title: 'Get started',
      node: (
        <div className="l2-close">
          <Terminal lines={['brew install baml']} />
          <a
            className="l2-close-link font-mono"
            href="https://new.boundaryml.com/quickstart"
            target="_blank"
            rel="noreferrer"
          >
            new.boundaryml.com/quickstart →
          </a>
        </div>
      ),
    },
  ];
}
