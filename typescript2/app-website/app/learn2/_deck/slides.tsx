'use client';

import Image from 'next/image';
import { BamlCode } from '../_components/BamlCode';
import BamlEditor from '../_components/baml-editor-lazy';
import LivePlayground from '../_components/LivePlaygroundLazy';
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
import { SpawnRace } from '../_components/SpawnRace';
import type { Slide } from '../_lib/types';

/* ------------------------------------------------------------------ *
 * Code snippets. The BAML ones (BAML_*) are real, self-contained, and
 * each passes `baml check` with zero errors (verified against the baml
 * CLI, toolchain 0.11.x — the same dialect the live editor runs). Keep
 * them self-contained (every type/fn/client defined in the snippet) and
 * re-verify with `baml check` if you edit them. Canonical dialect: class
 * fields `name: Type,`; type aliases end `;`; snake_case fns; LLM fns use
 * `client: "openai/gpt5.5"` + `{{ ctx.output_format }}`.
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

const BAML_AGENT = `// A tiny agent loop: decide -> execute -> observe, in a typed turn loop.
// The model returns a typed Step; a match dispatches to ordinary tool
// functions; results are observed back into the transcript; the loop ends
// when the model chooses to respond.

class Step {
  thought: string,
  action: "read_file" | "run_bash" | "respond",
  path: string?,
  command: string?,
  message: string?,
}

// The brain: given the transcript so far, pick the next single step.
function decide(transcript: string) -> Step {
  client: "openai/gpt5.5"
  prompt: #"
    You are a coding agent. Take ONE action at a time, then observe its result.
    Actions: read_file (path), run_bash (command), respond (message).

    Transcript so far:
    {{ transcript }}

    Decide the next step.
    {{ ctx.output_format }}
  "#
}

// Tools -- ordinary pure functions returning text the model can read.
function read_file(path: string) -> string {
  "contents of " + path + " (stub)"
}

function run_bash(command: string) -> string {
  "$ " + command + " (exit 0, stub)"
}

// Dispatch a decided Step to its tool via a match on the action.
function execute(step: Step) -> string {
  match (step.action) {
    "read_file" => read_file(step.path ?? ""),
    "run_bash" => run_bash(step.command ?? ""),
    "respond" => step.message ?? "",
  }
}

// One turn: decide -> act -> observe, until the model responds.
function run_turn(history: string, msg: string) -> string {
  let transcript = history + " | user: " + msg;
  let steps = 0;
  let max_steps = 8;

  while (steps < max_steps) {
    steps = steps + 1;
    let step = decide(transcript);
    if (step.action == "respond") {
      return step.message ?? "";
    };
    let result = execute(step);
    transcript = transcript + " | " + step.action + " -> " + result;
  }

  "(stopped after " + baml.unstable.string(max_steps) + " steps)"
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

const BAML_SPAWN = `// Pure-compute "work" -- stands in for any slow task (LLM call, IO, ...).
function work(i: int) -> int {
  i * i
}

// Per-call helper so each spawn captures its OWN i. (spawn captures by
// reference + let is function-scoped, so spawning inline in the loop races.)
function spawn_work(i: int) -> baml.future.Future<int, null> {
  spawn { work(i) }
}

// A PLAIN function -- no async, no "color". It launches N tasks
// concurrently, then joins their results.
function run_all(n: int) -> int[] {
  let tasks = [];
  for (let i = 0; i < n; i += 1) {
    tasks.push(spawn_work(i));      // launch concurrently
  }

  let results: int[] = [];
  for (let task in tasks) {
    let r = await task;             // join each result
    results.push(r);
  }
  results
}

// Any caller just calls it normally -- no await, no coloring.
function main() -> int[] {
  run_all(5)
}`;

const TS_COLOR = `async function serve() {        // 'async' colors this fn...
  for await (const sock of listener) {
    handle(sock);                 // ...and every caller, all the way up
  }
}`;

const BAML_TEST = `class Sentiment {
  label: "positive" | "negative" | "neutral",
}

function classify(text: string) -> Sentiment {
  let t = text.to_lower_case();
  if (t.includes("love") || t.includes("great") || t.includes("amazing")) {
    Sentiment { label: "positive" }
  } else if (t.includes("hate") || t.includes("terrible") || t.includes("awful")) {
    Sentiment { label: "negative" }
  } else {
    Sentiment { label: "neutral" }
  }
}

testset "basics" {
  test "clearly positive" {
    let v = classify("absolutely loved it!");
    assert.equal(v.label, "positive");
  }

  test "clearly negative" {
    let v = classify("this was terrible.");
    assert.equal(v.label, "negative");
  }
}`;

const BAML_JUDGE = `type Label = "positive" | "negative" | "neutral";

class Verdict {
  accept: bool,
  reason: string,
}

class Case {
  name: string,
  text: string,
  expected: Label,
}

function classify(text: string) -> Label {
  let t = text.to_lower_case();
  if (t.includes("love") || t.includes("great")) {
    "positive"
  } else if (t.includes("hate") || t.includes("terrible")) {
    "negative"
  } else {
    "neutral"
  }
}

// An LLM judge is just another typed function + assert.
function judge(text: string, label: Label) -> Verdict {
  client: "openai/gpt5.5"
  prompt: #"
    A classifier labeled this "{{ label }}". Text: {{ text }}
    Would a careful reader accept that label? {{ ctx.output_format }}
  "#
}

// Data-driven: load rows from a fixture (or fetch them), one case per row.
testset "from_fixture" {
  let cases = baml.json.from_string<Case[]>(#"[
    { "name": "loved",   "text": "I love this", "expected": "positive" },
    { "name": "hated",   "text": "I hate this", "expected": "negative" },
    { "name": "neutral", "text": "it is okay",  "expected": "neutral" }
  ]"#);

  for (let c in cases) {
    testset c.name {
      test "classifier matches expected label" {
        assert.equal(classify(c.text), c.expected);
      }
    }
  }
}`;

const BAML_WITH = `// Flaky/nondeterministic results? Handle them in-language: run the function
// many times and assert a pass-rate / quorum threshold with real asserts.
// trial() is a deterministic stand-in for a flaky call (fails on multiples of 7).
function trial(seed: int) -> bool {
  seed % 7 != 0
}

function pass_count(trials: int) -> int {
  let passed = 0;
  for (let i = 0; i < trials; i += 1) {
    if (trial(i)) { passed += 1; };
  }
  passed
}

testset "flaky_handled_deterministically" {
  test "pass-rate over 20 runs is at least 0.8" {
    let rate = pass_count(20) * 1.0 / 20.0;
    assert.is_true(rate >= 0.8);
  }

  test "quorum: a majority of 5 trials agree" {
    assert.is_true(pass_count(5) >= 3);
  }
}`;

const BAML_TYPES = `type Label = "positive" | "negative" | "neutral";

// type + literal unions with | ; recursive types are fine --
// the compiler guards the cycle automatically.
type Json = int | float | string | bool
          | Json[] | map<string, Json>;`;

const BAML_CLASSES = `class Greeting {
  message: string,
  letters: int,

  // factory: no self, called as Greeting.new(...)
  function new(name: string) -> Greeting {
    Greeting { message: "hi, " + name, letters: name.length() }
  }

  // instance method: takes self
  function shout(self) -> string {
    self.message.to_upper_case()
  }
}

function demo() -> string {
  // call the factory, then the instance method
  Greeting.new("vaibhav").shout()
}`;

const BAML_ERRORS = `class BuildError {
  reason: string,
}

// throws is inferred across the call graph
function generate(ok: bool) -> string throws BuildError {
  if (!ok) {
    throw BuildError { reason: "generate failed" };
  };
  "ok"
}

function build(ok: bool) -> string throws BuildError {
  generate(ok)
}

// catch by type, exhaustively, with catch_all
function run(ok: bool) -> string {
  build(ok) catch_all (e) {
    BuildError => "recovered: " + e.reason,
  }
}`;

const PY_EMBED = `from baml_sdk import b               # generated, typed client

g = b.greet("vaibhav")              # call a BAML function
print(g.shout())                    # instance method on the result

verdict = await b.classify_async("loved it")   # async twin, free`;

/* ------------------------------------------------------------------ */

export function getSlides(): Slide[] {
  return [
    {
      id: 'cover',
      node: (
        <div className="l2-cover">
          <div className="l2-cover-mark">
            <Image
              alt=""
              className="l2-cover-sheep"
              height={128}
              priority
              src="/baml-sheep.png"
              width={128}
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
      section: 'Intro',
      title: 'BAML',
    },

    {
      id: 'agenda',
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
      section: 'Intro',
      title: 'Agenda',
    },

    {
      id: 'why-language',
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
      section: 'Why',
      title: 'Why a new language?',
    },

    {
      id: 'fake-types',
      node: (
        <SlideShell
          kicker="Why · types"
          title="Types are the basis for observability"
          wide
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
                code={PY_FAKE_TYPES}
                diagnostics={[
                  {
                    line: 4,
                    message: 'no error — runs fine',
                    severity: 'warning',
                  },
                ]}
                filename="add.py"
                lang="python"
              />
            </div>
            <div className="l2-example">
              <p className="l2-example-label font-mono">
                TypeScript — every type is erased after compile
              </p>
              <BamlCode
                code={TS_FAKE_TYPES}
                diagnostics={[
                  {
                    line: 2,
                    message: 'any swallows everything downstream',
                    severity: 'warning',
                  },
                  {
                    line: 4,
                    message: 'ships to prod, crashes at runtime',
                    severity: 'error',
                  },
                ]}
                filename="fetch.ts"
                lang="typescript"
              />
            </div>
          </div>
        </SlideShell>
      ),
      section: 'Why',
      title: 'Why not just improve Python / TS?',
    },

    {
      id: 'great-for',
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
      section: 'What it is',
      title: 'What is BAML great for?',
    },

    {
      id: 'three-things',
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
      section: 'What it is',
      title: 'Three things we need',
    },

    {
      id: 'roadmap',
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
      section: 'Roadmap',
      title: 'What this talk covers',
    },

    {
      id: 'section-viz',
      node: (
        <SectionDivider
          blurb="See the pipeline, not just the prompt."
          index="Section 1"
          title="Playground Visualization"
        />
      ),
      section: 'Visualization',
      title: 'Playground Visualization',
    },

    {
      id: 'just-typescript',
      node: (
        <SlideShell
          kicker="Visualization"
          title="It reads like a language you know"
          wide
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
      section: 'Visualization',
      title: 'It reads like TypeScript',
    },

    {
      id: 'image-pipeline',
      node: (
        <SlideShell kicker="Visualization" title="A simple pipeline" wide>
          <Lead>
            {
              'Generate an image, then have an LLM describe it — one typed pipeline. Edit it, then run it live.'
            }
          </Lead>
          <LivePlayground initialCode={BAML_IMAGE} />
        </SlideShell>
      ),
      section: 'Visualization',
      title: 'A simple pipeline',
    },

    {
      id: 'claude-code',
      node: (
        <SlideShell kicker="Visualization" title="A more complex pipeline" wide>
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
      section: 'Visualization',
      title: 'A more complex pipeline',
    },

    {
      id: 'section-agents',
      node: (
        <SectionDivider
          blurb="A language built for a model’s context window."
          index="Section 2"
          title="How agents read & write BAML"
        />
      ),
      section: 'Agents & BAML',
      title: 'How agents read & write BAML',
    },

    {
      id: 'minified-skill',
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
      section: 'Agents & BAML',
      title: 'The whole skill fits here',
    },

    {
      id: 'baml-describe',
      node: (
        <SlideShell kicker="Agents & BAML" title="baml describe" wide>
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
              <BamlCode code={SH_DESCRIBE} filename="terminal" lang="bash" />
            }
          />
        </SlideShell>
      ),
      section: 'Agents & BAML',
      title: 'baml describe',
    },

    {
      id: 'baml-run',
      node: (
        <SlideShell kicker="Agents & BAML" title="run · fmt · generate" wide>
          <Split
            left={
              <Callout tone="warn">
                {
                  'Outline mentions `baml pack <fn>` / `--target` — not in this toolchain yet. Distribute via `baml generate`. (verify before shipping)'
                }
              </Callout>
            }
            right={<BamlCode code={SH_RUN} filename="terminal" lang="bash" />}
          />
        </SlideShell>
      ),
      section: 'Agents & BAML',
      title: 'run · fmt · generate',
    },

    {
      id: 'section-parallel',
      node: (
        <SectionDivider
          blurb="Concurrency without the colored functions."
          index="Section 3"
          title="Parallelism"
        />
      ),
      section: 'Parallelism',
      title: 'Parallelism',
    },

    {
      id: 'spawn',
      node: (
        <SlideShell
          kicker="Parallelism"
          title="spawn, and no function coloring"
          wide
        >
          <Split
            left={
              <BamlEditor filename="server.baml" initialCode={BAML_SPAWN} />
            }
            right={
              <BamlCode
                code={TS_COLOR}
                filename="server.ts"
                lang="typescript"
                notes={[
                  { line: 1, text: 'async spreads up the whole call stack' },
                ]}
              />
            }
          />
        </SlideShell>
      ),
      section: 'Parallelism',
      title: 'spawn — no function coloring',
    },

    {
      id: 'throughput',
      node: (
        <SlideShell
          kicker="Parallelism"
          title="Throughput: spawn vs async/await"
          wide
        >
          <SpawnRace />
        </SlideShell>
      ),
      section: 'Parallelism',
      title: 'Throughput',
    },

    {
      id: 'section-evals',
      node: (
        <SectionDivider
          blurb="Tests that live with the code."
          index="Section 4"
          title="Evals"
        />
      ),
      section: 'Evals',
      title: 'Evals',
    },

    {
      id: 'test-testset',
      node: (
        <SlideShell kicker="Evals" title="test & testset" wide>
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
      section: 'Evals',
      title: 'test & testset',
    },

    {
      id: 'llm-judge',
      node: (
        <SlideShell
          kicker="Evals"
          title="LLM-as-judge, and tests from data"
          wide
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
      section: 'Evals',
      title: 'LLM-as-judge & data-driven tests',
    },

    {
      id: 'with-clauses',
      node: (
        <SlideShell
          kicker="Evals"
          title="Flaky results? Assert a threshold"
          wide
        >
          <Split
            left={
              <>
                <Lead>
                  {
                    'No magic runner DSL — pass-rates and quorums are just code.'
                  }
                </Lead>
                <Bullets
                  items={[
                    'Run a nondeterministic call many times in a loop',
                    'Assert a pass-rate or quorum threshold with assert.*',
                  ]}
                />
              </>
            }
            right={<BamlEditor filename="flaky.baml" initialCode={BAML_WITH} />}
          />
        </SlideShell>
      ),
      section: 'Evals',
      title: 'Flaky results',
    },

    {
      id: 'section-types',
      node: (
        <SectionDivider
          blurb="Unions, recursion, named everything."
          index="Section 5"
          title="Type system"
        />
      ),
      section: 'Type system',
      title: 'Type system',
    },

    {
      id: 'unions',
      node: (
        <SlideShell kicker="Type system" title="Unions & recursive types" wide>
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
      section: 'Type system',
      title: 'Unions & recursive types',
    },

    {
      id: 'classes',
      node: (
        <SlideShell kicker="Type system" title="Classes, methods, names" wide>
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
      section: 'Type system',
      title: 'Classes & methods',
    },

    {
      id: 'section-errors',
      node: (
        <SectionDivider
          blurb="Errors are types, inferred for you."
          index="Section 6"
          title="Error handling"
        />
      ),
      section: 'Errors',
      title: 'Error handling',
    },

    {
      id: 'typed-errors',
      node: (
        <SlideShell kicker="Errors" title="Errors are part of the type" wide>
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
      section: 'Errors',
      title: 'Typed errors',
    },

    {
      id: 'section-adoption',
      node: (
        <SectionDivider
          blurb="BAML as an embedded language."
          index="Section 7"
          title="Incremental adoption"
        />
      ),
      section: 'Adoption',
      title: 'Incremental adoption',
    },

    {
      id: 'embed-python',
      node: (
        <SlideShell kicker="Adoption" title="Just call it from Python" wide>
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
            right={<BamlCode code={PY_EMBED} filename="app.py" lang="python" />}
          />
        </SlideShell>
      ),
      section: 'Adoption',
      title: 'Just call it from Python',
    },

    {
      id: 'section-start',
      node: (
        <SectionDivider
          blurb="Five minutes to your first function."
          index="Section 8"
          title="Getting started"
        />
      ),
      section: 'Getting started',
      title: 'Getting started',
    },

    {
      id: 'getting-started',
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
      section: 'Getting started',
      title: 'Try it',
    },

    {
      id: 'agents-better',
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
      section: 'Roadmap',
      title: 'Making agents better',
    },

    {
      id: 'closing',
      node: (
        <div className="l2-close">
          <Terminal lines={['brew install baml']} />
          <a
            className="l2-close-link font-mono"
            href="https://new.boundaryml.com/quickstart"
            rel="noreferrer"
            target="_blank"
          >
            new.boundaryml.com/quickstart →
          </a>
        </div>
      ),
      section: 'Getting started',
      title: 'Get started',
    },
  ];
}
