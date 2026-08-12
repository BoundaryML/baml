'use client';

import Image from 'next/image';
import BamlEditor from '@/app/learn2/_components/baml-editor-lazy';
import { BamlCode } from '../../learn2/_components/BamlCode';
import LivePlayground from '../../learn2/_components/LivePlaygroundLazy';
import {
  Bullets,
  Callout,
  Lead,
  SlideShell,
  Split,
  Terminal,
} from '../../learn2/_components/primitives';
import type { Slide } from '../../learn2/_lib/types';
import { CoreUsage } from '../_components/CoreUsage';
import { InfectionGraph } from '../_components/InfectionGraph';
import { MetricsDag } from '../_components/MetricsDag';
import { type TermEvent, TermPlay } from '../_components/TermPlay';

/* ------------------------------------------------------------------ *
 * BAML snippets. Every BAML_* snippet is real and passes `baml check`
 * (toolchain 0.11.x — the dialect the live editors run). The one
 * intentional diagnostic (BAML_UNREACHABLE) is also verified: it
 * produces exactly warning E0063 "unreachable arm". Re-verify with
 * `baml check` if you edit any of them. Terminal transcripts in this
 * file are captured, unedited CLI output (only `Loading/Checking`
 * preamble lines trimmed).
 * ------------------------------------------------------------------ */

const BAML_HELLO = `class Greeting {
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
  Greeting.new("hn").shout()
}

test "shouts" {
  assert.equal(demo(), "HI, HN");
}`;

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

const TS_BOUNDARY = `const res = await openai.chat.completions.create({
  messages: [{ role: 'user', content: prompt }],
  // the schema lives in the prompt string; the compiler can't see it
});
const data = JSON.parse(res.choices[0].message.content!);

data.label;        // data: any
data.confidence;   // data: any`;

const BAML_ERRORS = `class BuildError {
  reason: string,
}

// Note what's missing: no throws declarations anywhere.
// The compiler infers the error type of every function
// from its body and everything it calls.
function generate(ok: bool) -> string {
  if (!ok) {
    throw BuildError { reason: "generate failed" };
  };
  "ok"
}

function build(ok: bool) -> string {
  generate(ok)    // hover: build can throw BuildError
}

// catch by type, exhaustively, with catch_all
function run(ok: bool) -> string {
  build(ok) catch_all (e) {
    BuildError => "recovered: " + e.reason,
  }
}`;

const BAML_UNREACHABLE = `class NetError {
  detail: string,
}

class ParseError {
  detail: string,
}

function fetch_page(ok: bool) -> string {
  if (!ok) {
    throw NetError { detail: "connect timeout" };
  };
  "<html>"
}

// Nothing is declared, yet the compiler knows fetch_page can
// ONLY throw NetError — so the ParseError arm is unreachable.
// Try it: add "throws ParseError" on line 9 and the compiler
// rejects the function for hiding the error it really throws.
function show(ok: bool) -> string {
  fetch_page(ok) catch (e) {
    ParseError => "unreachable",
    NetError => "recovered: " + e.detail,
  }
}`;

const TS_CATCH = `try {
  await pipeline(doc);
} catch (e) {
  // e: unknown — the type system has no idea
  // what pipeline() can actually throw
  if (e instanceof NetError) { /* guess */ }
}`;

const PY_CATCH = `try:
    pipeline(doc)
except Exception as e:
    # which exceptions can pipeline raise?
    # the signature doesn't say. grep and hope.
    ...`;

const GO_CATCH = `out, err := pipeline(doc)
if err != nil {
    var ne *NetError
    if errors.As(err, &ne) {
        // nothing checks these cases are
        // complete. forget one, and the
        // compiler stays quiet
    }
}`;

const TS_COLOR = `async function serve() {        // 'async' colors this fn...
  for await (const sock of listener) {
    handle(sock);                 // ...and every caller, all the way up
  }
}`;

const PY_COLOR = `async def fetch(url): ...       # async colors this fn

def handler(req):                # sync caller? tough.
    return asyncio.run(fetch(u)) # spin up a loop, block on it`;

const BAML_SPAWN = `// Pure-compute "work" -- stands in for any slow task (LLM call, IO, ...).
function work(i: int) -> int {
  i * i
}

// Per-call helper so each spawn captures its OWN i. (spawn captures by
// reference + let is function-scoped, so spawning inline in the loop races.)
function spawn_work(i: int) -> baml.future.Future<int, null> {
  spawn { work(i) }
}

// A plain function -- nothing marks it async. It launches N tasks
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

// Callers just call it -- they never learn it spawned anything.
function main() -> int[] {
  run_all(5)
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

const BAML_PASSRATE = `// Nondeterministic results? Statistical evaluation is plain code:
// run the function N times, assert a pass-rate or a quorum.
// trial() is a deterministic stand-in for a flaky call.
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

testset "nondeterminism_measured" {
  test "pass-rate over 20 runs is at least 0.8" {
    let rate = pass_count(20) * 1.0 / 20.0;
    assert.is_true(rate >= 0.8);
  }

  test "quorum: a majority of 5 trials agree" {
    assert.is_true(pass_count(5) >= 3);
  }
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

const PY_EMBED = `from baml_sdk import b               # generated, typed client

g = b.greet("vaibhav")              # call a BAML function
print(g.shout())                    # instance method on the result

verdict = await b.classify_async("loved it")   # async twin, free`;

// Design-stage syntax (metric blocks) — does NOT compile today, which is
// the point of the slide. Rendered read-only, never in a live editor.
const BAML_METRIC = `function extract_resume(raw: string) -> Resume { ... }

metric extract_resume {
  expected: Resume        // external data — may arrive hours later

  // parameter names ARE the dependency graph
  function field_count(output) -> int { output.fields.len() }

  function judge(input, output) -> Score { JudgeQuality(input, output) }
  function quality(judge) -> float       { judge.value }
  function faithfulness(judge) -> float  { judge.faithfulness }

  function precision(output, expected) -> float { ... }
  function recall(output, expected) -> float    { ... }
  function f1(precision, recall) -> float {
    2.0 * precision * recall / (precision + recall)
  }
}`;

const BAML_METRIC_USE = `let id = boundary.id()
let resume = extract_resume(doc, $id = id)
// field_count, judge, quality fire now — they only need output.
// precision, recall, f1 are pending: no ground truth yet.

// ...four hours later, a human labels the doc:
id.set_expected(ground_truth)     // type-checked: Resume
let f1 = await id.get_f1()        // -> float`;

/* ------------------- captured terminal transcripts ------------------- */

const PACK_EVENTS: TermEvent[] = [
  { cmd: 'baml pack -f greet -f main -o greet-bin' },
  { text: '   Packaging greet,main', tone: 'dim' },
  {
    text: '    Finished greet-bin [greet,main, aarch64-apple-darwin] in 0s',
    tone: 'ok',
  },
  { cmd: 'ls -lah greet-bin', pause: 0.4 },
  { text: '9.8M greet-bin' },
  { cmd: './greet-bin greet --name "hacker news"', pause: 0.4 },
  { text: '{"message":"hi, hacker news"}', tone: 'accent' },
  { cmd: './greet-bin --help', pause: 0.4 },
  { text: 'Usage: greet-bin <COMMAND>' },
  { text: '' },
  { text: 'Commands:' },
  { text: '  greet  function greet(name: string) -> Greeting' },
  { text: '  main   function main() -> string' },
];

// The agent without `describe`: every step below is a real command with its
// real output against the same project — this is the standard grep-and-read
// loop, and after four tool calls the caller list is still just text matches.
const GREP_EVENTS: TermEvent[] = [
  { cmd: 'grep -rn "greet" baml_src/' },
  { text: 'main.baml:5:function greet(name: string) -> Greeting {' },
  { text: 'main.baml:10:    greet("world").message' },
  { text: 'main.baml:14:    test "greets_world" {' },
  { text: 'main.baml:18:        assert.equal(greet("bob").message, …' },
  { cmd: 'cat baml_src/main.baml', pause: 0.5 },
  { text: 'class Greeting {' },
  { text: '    message: string,' },
  { text: '… 19 more lines read into context …', tone: 'dim' },
  { cmd: 'grep -rn "Greeting" baml_src/', pause: 0.5 },
  { text: 'main.baml:1:class Greeting {' },
  { text: 'main.baml:5:function greet(name: string) -> Greeting {' },
  { text: 'main.baml:6:    Greeting { message: "hi, " + name }' },
  { cmd: 'grep -rn "greet(" baml_src/', pause: 0.5 },
  { text: 'main.baml:5:function greet(name: string) -> Greeting {' },
  { text: 'main.baml:10:    greet("world").message' },
  { text: 'main.baml:18:        assert.equal(greet("bob").message, …' },
  {
    pause: 0.5,
    text: '# 4 tool calls, a whole file in context — and the',
    tone: 'dim',
  },
  { text: '# caller list is still just text matches', tone: 'dim' },
];

const DESCRIBE_EVENTS: TermEvent[] = [
  { cmd: 'baml describe greet' },
  { text: 'function greet  baml_src/main.baml:5-7', tone: 'accent' },
  { text: '' },
  { text: 'function greet(name: string) -> Greeting {' },
  { text: '    Greeting { message: "hi, " + name }' },
  { text: '}' },
  { text: '' },
  { text: 'dependencies:' },
  { text: '  class  Greeting  baml_src/main.baml:1' },
  { text: '' },
  { text: 'references (2):' },
  { text: '  baml_src/main.baml:10  greet("world").message' },
  { text: '  baml_src/main.baml:18  assert.equal(greet("bob")…' },
  { text: '' },
  {
    pause: 0.3,
    text: '✓ one call: signature, deps, every reference',
    tone: 'ok',
  },
];

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
              'A programming language for code that calls models. A model call is just a typed function — and errors, parallelism, tests, and tracing come with the language.'
            }
          </p>
          <p className="l2-cover-hint font-mono">
            {'press → or space to begin'}
          </p>
        </div>
      ),
      section: 'Thesis',
      title: 'BAML',
    },

    {
      id: 'infection',
      node: (
        <SlideShell title="One model call changes everything above it" wide>
          <Split
            left={
              <>
                <Lead>
                  {
                    'If a function calls a model — even three layers down — its output can change from run to run.'
                  }
                </Lead>
                <Bullets
                  items={[
                    'Not just LLMs: ML models, external APIs, randomness, human input',
                    'assert output == expected stops working for everything above the call',
                    'And the language gives you no warning — it looks like any other call',
                  ]}
                />
              </>
            }
            right={<InfectionGraph />}
          />
        </SlideShell>
      ),
      section: 'Thesis',
      title: 'One stochastic call infects the graph',
    },

    {
      id: 'why-language',
      node: (
        <SlideShell title="Why a language, and not a library">
          <Lead>{'A library cannot:'}</Lead>
          <Bullets
            items={[
              'Check the prompt against your types before anything runs — in BAML the return type is the schema sent to the model, and the parser on the way back',
              'Work out what errors every function can throw, automatically, across the whole program',
              'See every call before the program runs — which is why the visualization and tracing you are about to see are exact, not best-effort',
              'Get rid of the async/await split',
            ]}
          />
          <Callout tone="note">
            {
              'Everything in this deck up to the clearly-marked design section is shipped and runs — most of it live, in this browser tab.'
            }
          </Callout>
        </SlideShell>
      ),
      section: 'Thesis',
      title: 'Why a language, not a library',
    },

    {
      id: 'hello',
      node: (
        <SlideShell title="The language, in sixty seconds" wide>
          <Split
            left={
              <>
                <Lead>
                  {
                    'Classes, methods, tests. The last expression is the return value.'
                  }
                </Lead>
                <Bullets
                  items={[
                    'This is a real editor — the full compiler runs in a worker on this page',
                    'Hover for types; click ▶ Run test and it executes here',
                    'No anonymous shapes: every product type has a name',
                  ]}
                />
              </>
            }
            right={
              <BamlEditor filename="greeting.baml" initialCode={BAML_HELLO} />
            }
          />
        </SlideShell>
      ),
      section: 'The language',
      title: 'Sixty seconds of BAML',
    },

    {
      id: 'boundary',
      node: (
        <SlideShell title="Types have to outlive the compiler" wide>
          <Lead>
            {
              'TypeScript checks your types, then erases them. But the model replies at runtime — exactly when there is nothing left to check it against.'
            }
          </Lead>
          <Split
            left={
              <div className="l2-example">
                <p className="l2-example-label font-mono">
                  TypeScript — the type system never sees the model
                </p>
                <BamlCode
                  code={TS_BOUNDARY}
                  diagnostics={[
                    {
                      line: 5,
                      message: 'const data: any',
                      severity: 'warning',
                    },
                  ]}
                  filename="classify.ts"
                  lang="typescript"
                />
              </div>
            }
            right={
              <div className="l2-example">
                <p className="l2-example-label font-mono">
                  BAML — the return type is the schema and the parser
                </p>
                <BamlEditor
                  filename="classify.baml"
                  initialCode={BAML_SENTIMENT}
                />
              </div>
            }
          />
          <Callout tone="note">
            {
              'ctx.output_format writes the Verdict type into the prompt; the runtime parses the reply back into a Verdict — or fails with a typed error.'
            }
          </Callout>
        </SlideShell>
      ),
      section: 'The language',
      title: 'Types at the model boundary',
    },

    {
      id: 'graph',
      node: (
        <div className="l3-playground-slide">
          <h2 className="l2-slide-title">The graph is the program</h2>
          <p className="l2-lead l3-lead-wide">
            {
              'The compiler knows every call in the pipeline, so the visualization is exact and never goes stale. Edit the code; run it.'
            }
          </p>
          <LivePlayground
            initialCode={BAML_IMAGE}
            initialFunction="illustrate"
          />
        </div>
      ),
      section: 'The language',
      title: 'The graph is the program',
    },

    {
      id: 'errors-compare',
      node: (
        <SlideShell title="Where mainstream languages lose the error type" wide>
          <Lead>
            {
              'Code that calls models fails a lot — and error handling is the least typed part of every mainstream language.'
            }
          </Lead>
          <div className="l3-cols3">
            <div className="l2-example">
              <p className="l2-example-label font-mono">TypeScript</p>
              <BamlCode code={TS_CATCH} filename="run.ts" lang="typescript" />
            </div>
            <div className="l2-example">
              <p className="l2-example-label font-mono">Python</p>
              <BamlCode code={PY_CATCH} filename="run.py" lang="python" />
            </div>
            <div className="l2-example">
              <p className="l2-example-label font-mono">Go</p>
              <BamlCode code={GO_CATCH} filename="run.go" lang="go" />
            </div>
          </div>
        </SlideShell>
      ),
      section: 'Errors',
      title: 'Where the error type gets lost',
    },

    {
      id: 'errors-baml',
      node: (
        <SlideShell title="Errors are types; throws is inferred" wide>
          <Split
            left={
              <Bullets
                items={[
                  'You never write throws — the compiler works out what every function can throw, including through callbacks and spawned tasks',
                  'There is nothing to annotate. If you do declare throws, the compiler checks it against reality',
                  'catch matches on the error type; catch_all handles every remaining case',
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
      title: 'throws is inferred',
    },

    {
      id: 'errors-live',
      node: (
        <SlideShell title="The compiler knows the complete error set" wide>
          <Split
            left={
              <>
                <Lead>
                  {
                    'This editor has a live warning: the ParseError arm can never fire, and the compiler can prove it.'
                  }
                </Lead>
                <Bullets
                  items={[
                    'Handling an error that cannot happen is dead code, and the compiler can prove it',
                    'Nothing here is declared. Declarations are optional — write one that hides a real error and the compiler rejects it',
                    'Try it: add throws ParseError on line 9 and watch both fire',
                  ]}
                />
              </>
            }
            right={
              <BamlEditor
                filename="fetch.baml"
                initialCode={BAML_UNREACHABLE}
              />
            }
          />
        </SlideShell>
      ),
      section: 'Errors',
      title: 'The compiler knows the error set',
    },

    {
      id: 'coloring',
      node: (
        <SlideShell title="Reads like TypeScript. Schedules like Go." wide>
          <Split
            left={
              <>
                <div className="l2-example">
                  <p className="l2-example-label font-mono">
                    TypeScript — async repaints the call stack
                  </p>
                  <BamlCode
                    code={TS_COLOR}
                    filename="server.ts"
                    lang="typescript"
                  />
                </div>
                <div className="l2-example">
                  <p className="l2-example-label font-mono">
                    Python — same wall, different paint
                  </p>
                  <BamlCode
                    code={PY_COLOR}
                    filename="server.py"
                    lang="python"
                  />
                </div>
              </>
            }
            right={
              <BamlEditor filename="spawn.baml" initialCode={BAML_SPAWN} />
            }
          />
          <Callout tone="note">
            {
              'To be precise: spawn gives you a Future and you await it — but any plain function can do that, and its callers never know. Nothing spreads up the call stack.'
            }
          </Callout>
        </SlideShell>
      ),
      section: 'Concurrency',
      title: 'No function coloring',
    },

    {
      id: 'fanout',
      node: (
        <SlideShell title="Use every core" wide>
          <Lead>
            {
              'Promise.all can overlap waiting, but compute still runs on one core. BAML spawns schedule across all of them.'
            }
          </Lead>
          <CoreUsage />
        </SlideShell>
      ),
      section: 'Concurrency',
      title: 'Use every core',
    },

    {
      id: 'tests',
      node: (
        <SlideShell title="Tests are language constructs" wide>
          <Split
            left={
              <Bullets
                items={[
                  'test and testset are syntax, not a framework convention',
                  'assert.equal / is_true / contains / not_null / approx_equal',
                  'Testsets can be generated by code — loop over rows, emit one test per case',
                  '▶ Run these here; the runner is part of the toolchain',
                ]}
              />
            }
            right={<BamlEditor filename="tests.baml" initialCode={BAML_TEST} />}
          />
        </SlideShell>
      ),
      section: 'Evals',
      title: 'Tests are language constructs',
    },

    {
      id: 'flaky',
      node: (
        <SlideShell title="Statistical evaluation is just code" wide>
          <Split
            left={
              <>
                <Lead>
                  {
                    'When exact asserts break down, you measure how often it passes. The whole mechanism is a loop and an assert.'
                  }
                </Lead>
                <Bullets
                  items={[
                    'Run the nondeterministic call N times, assert a pass-rate',
                    'Quorums, thresholds, judges — ordinary functions',
                  ]}
                />
              </>
            }
            right={
              <BamlEditor filename="evals.baml" initialCode={BAML_PASSRATE} />
            }
          />
        </SlideShell>
      ),
      section: 'Evals',
      title: 'Statistical evaluation is just code',
    },

    {
      id: 'pack',
      node: (
        <SlideShell title="A function is a deployable unit" wide>
          <Split
            left={
              <>
                <Lead>
                  {
                    'baml pack compiles functions into one self-contained native binary — and derives a CLI from their signatures.'
                  }
                </Lead>
                <Bullets
                  items={[
                    'Nothing to install on the target machine — the binary carries the runtime',
                    'The CLI flags come straight from the function signature',
                    'Cross-compile with --target per platform',
                  ]}
                />
                <Callout tone="note">
                  {'Captured output — this ran in 0.17s on a laptop.'}
                </Callout>
              </>
            }
            right={<TermPlay events={PACK_EVENTS} title="baml pack" />}
          />
        </SlideShell>
      ),
      section: 'Toolchain',
      title: 'baml pack',
    },

    {
      id: 'describe',
      node: (
        <SlideShell title="baml describe — one call, a complete answer" wide>
          <div className="l3-task font-mono">
            <span className="l3-task-label">the agent’s task</span>
            {'what does greet return, and where is it called from?'}
          </div>
          <Split
            left={
              <TermPlay events={GREP_EVENTS} title="agent without describe" />
            }
            right={
              <TermPlay events={DESCRIBE_EVENTS} title="agent with describe" />
            }
          />
          <Callout tone="note">
            {
              'Both sides are real output against the same project. describe also answers for the stdlib and the grammar itself; --budget bounds the output to fit a context window; --json for machines.'
            }
          </Callout>
        </SlideShell>
      ),
      section: 'Toolchain',
      title: 'baml describe vs grep',
    },

    {
      id: 'embed',
      node: (
        <SlideShell title="It embeds in the app you already have" wide>
          <Split
            left={
              <Bullets
                items={[
                  'baml generate writes a typed client — pydantic models, sync and async variants of every function',
                  'BAML class methods become methods on the generated models',
                  'Pass Python functions into BAML; if one raises, you get the same exception object back',
                  'Python and TypeScript today, one shared native layer underneath',
                  'And the other direction: this very deck runs the compiler, VM, and language server in your browser',
                ]}
              />
            }
            right={<BamlCode code={PY_EMBED} filename="app.py" lang="python" />}
          />
        </SlideShell>
      ),
      section: 'Embedding',
      title: 'Call it from Python',
    },

    {
      id: 'metrics',
      node: (
        <SlideShell title="Metrics as a language concept" wide>
          <span className="l3-design-tag font-mono">
            design proposal — not shipped
          </span>
          <Split
            left={
              <>
                <Lead>
                  {
                    'Today, evals and metrics live outside the code: dashboard rules, YAML, separate scripts. Rename a function and the metric breaks silently.'
                  }
                </Lead>
                <Bullets
                  items={[
                    'Metrics live next to the function and are type-checked against it',
                    'Parameter names say what each metric needs — rename anything and it is a compile error, not a dead dashboard',
                    'Versioned in git with the code they measure',
                  ]}
                />
              </>
            }
            right={
              <BamlCode
                code={BAML_METRIC}
                filename="resume.baml (proposed)"
                lang="baml"
              />
            }
          />
        </SlideShell>
      ),
      section: 'Design — not shipped',
      title: 'Metrics as a language concept',
    },

    {
      id: 'metrics-dag',
      node: (
        <SlideShell title="Metrics fire when their data arrives" wide>
          <span className="l3-design-tag font-mono">
            design proposal — not shipped
          </span>
          <Split
            left={
              <>
                <BamlCode
                  code={BAML_METRIC_USE}
                  filename="usage.baml (proposed)"
                  lang="baml"
                />
                <Callout tone="warn">
                  {
                    'This is the part we want torn apart. The runtime already records a trace of every call; this design is the layer on top. Objections welcome — that is why it is in this deck.'
                  }
                </Callout>
              </>
            }
            right={<MetricsDag />}
          />
        </SlideShell>
      ),
      section: 'Design — not shipped',
      title: 'Metrics fire when data arrives',
    },

    {
      id: 'tradeoffs',
      node: (
        <SlideShell title="What you are signing up for">
          <Bullets
            items={[
              'A new language. Models have not seen much BAML in training — the toolchain makes up for it (describe, typed errors, agent skills), but it is a real cost',
              'Two first-class SDK targets today: Python and TypeScript. Go and Ruby are in progress',
              'The metrics design you just saw is not built',
              'Pre-1.0: the language still changes',
            ]}
          />
          <Callout tone="note">
            {
              'Adoption is incremental by construction: one function behind a generated client. Your app does not move; one call site does.'
            }
          </Callout>
        </SlideShell>
      ),
      section: 'Tradeoffs',
      title: 'What you are signing up for',
    },

    {
      id: 'close',
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
      section: 'Start',
      title: 'Try it',
    },
  ];
}
