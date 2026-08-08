'use client';

import Image from 'next/image';
import type { ReactNode } from 'react';
import BamlEditor from '@/app/learn2/_components/baml-editor-lazy';
import { BamlCode } from '../../learn2/_components/BamlCode';
import LivePlayground from '../../learn2/_components/LivePlaygroundLazy';
import { Terminal } from '../../learn2/_components/primitives';
import { CoreUsage } from '../../learn3/_components/CoreUsage';
import { InfectionGraph } from '../../learn3/_components/InfectionGraph';
import { MetricsDag } from '../../learn3/_components/MetricsDag';
import { type TermEvent, TermPlay } from '../../learn3/_components/TermPlay';
import { HoverTerm } from './HoverTerm';
import { PoolSchedule } from './PoolSchedule';
import { RotatingTypes } from './RotatingTypes';
import { SdkPipeline } from './SdkPipeline';
import { StackTower } from './StackTower';

/* ------------------------------------------------------------------ *
 * Snippets. Every BAML_* snippet passes `baml check` (toolchain 0.11.x,
 * the dialect the live editors run); the two intentional diagnostics
 * (BAML_UNREACHABLE → E0063 warning, NS_BAD → "Did you mean
 * `root.a.Widget`?") show the compiler's real messages. Terminal
 * transcripts are captured CLI output. Re-verify with `baml check`
 * if you edit anything.
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

const PY_GATHER = `async def work(i): ...            # must be async

async def run_all(n):             # so must this
    return await asyncio.gather(
        *(work(i) for i in range(n)))

def main():                       # sync world: pay the toll
    return asyncio.run(run_all(5))`;

const BAML_SPAWN = `// A plain function -- nothing marks it async. It launches N tasks
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
}

// -- helpers below -------------------------------------------------

// Per-call helper so each spawn captures its OWN i. (spawn captures by
// reference + let is function-scoped, so spawning inline in the loop races.)
function spawn_work(i: int) -> baml.future.Future<int, null> {
  spawn { work(i) }
}

// Pure-compute "work" -- stands in for any slow task (LLM call, IO, ...).
function work(i: int) -> int {
  i * i
}`;

const BAML_SPAWN_ADV = `// cap shard work at two in flight; extras queue fifo in the group
function main() -> int {
  let pool = baml.spawn.TaskGroup.new(2, name = "shards");

  let a = spawn "shard-0" with baml.spawn.options(group = pool) {
    checksum(0, 50000)
  };
  let b = spawn "shard-1" with baml.spawn.options(group = pool) {
    checksum(50000, 100000)
  };
  let c = spawn "shard-2" with baml.spawn.options(group = pool) {
    read_segment(7)
  };

  // the future's error side carries the body's throws clause;
  // the catch arm is matched by type at the await site
  let salvage = (await c) catch (e) {
    CorruptSegment => e.offset
  };

  (await a) + (await b) + salvage
}

test "shards recover the corrupt segment" {
  assert.equal(main(), 4999953584);
}

// -- helpers below --------------------------------------------------

class CorruptSegment {
  offset: int,
}

// odd segment ids simulate a bad read
function read_segment(id: int) -> int throws CorruptSegment {
  if (id % 2 == 1) { throw CorruptSegment { offset: id * 512 } }
  id * 8
}

// closed-form sum over [lo, hi) stands in for real work
function checksum(lo: int, hi: int) -> int {
  (hi - lo) * (lo + hi - 1) / 2
}`;

const PY_SEMAPHORE = `sem = asyncio.Semaphore(2)          # the cap, hand-rolled

async def limited(i):
    async with sem:                 # every call site must remember
        return await work(i)

async def run_all(n):
    return await asyncio.gather(
        *(limited(i) for i in range(n)))`;

const BAML_TALLY = `// the first parameter is a function type -- the host fills it in
function tally(score: (int) -> int, xs: int[]) -> int {
  let total = 0;
  for (let x in xs) {
    total += score(x);
  }
  total
}

function double(x: int) -> int {
  x * 2
}

test "functions are arguments" {
  assert.equal(tally(double, [1, 2, 3]), 12);
}`;

const PY_CALLBACK = `from baml_sdk import tally

def score(x: int) -> int:        # plain python
    return x * 2

tally(score=score, xs=[1, 2, 3])   # -> 12, baml calling python
# raise inside score() and the SAME exception
# object surfaces back here -- not a copy`;

const TS_CATCH = `try {
  await pipeline(doc);
} catch (e) {
  // e: unknown — the type system has no idea
  // what pipeline() can actually throw
  if (e instanceof NetError) { /* guess */ }
}`;

const BAML_UNREACHABLE = `// Nothing is declared below, yet the compiler knows fetch_page
// can ONLY throw NetError -- so the ParseError arm is provably
// dead code. Try it: add "throws ParseError" to fetch_page and
// the compiler rejects it for hiding the error it really throws.
function show(ok: bool) -> string {
  fetch_page(ok) catch (e) {
    ParseError => "unreachable",
    NetError => "recovered: " + e.detail,
  }
}

// -- the rest is plumbing -----------------------------------------

function fetch_page(ok: bool) -> string {
  if (!ok) {
    throw NetError { detail: "connect timeout" };
  };
  "<html>"
}

class NetError {
  detail: string,
}

class ParseError {
  detail: string,
}`;

const BAML_AGENT = `// A tiny agent: decide -> execute -> observe, in a typed turn loop.
// The model returns a typed Step; a match dispatches to ordinary tool
// functions; results are observed back into the transcript; the loop
// ends when the model chooses to respond.

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

const YAML_EVAL = `# evals live in a separate tool, bound to code by strings
prompts:
  - classify.txt
providers:
  - openai:gpt-5.5
tests:
  - vars: { text: "absolutely loved it!" }
    assert:
      - type: equals
        value: positive
# rename classify, and nothing here notices`;

const BAML_TEST = `testset "basics" {
  test "clearly positive" {
    let v = classify("absolutely loved it!");
    assert.equal(v.label, "positive");
  }

  test "clearly negative" {
    let v = classify("this was terrible.");
    assert.equal(v.label, "negative");
  }
}

// -- the classifier under test --------------------------------------

class Sentiment {
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
}`;

const BAML_PASSRATE = `// Nondeterministic results? Run the function N times and
// assert a pass-rate or a quorum -- it is plain code.
testset "nondeterminism_measured" {
  test "pass-rate over 20 runs is at least 0.8" {
    let rate = pass_count(20) * 1.0 / 20.0;
    assert.is_true(rate >= 0.8);
  }

  test "quorum: a majority of 5 trials agree" {
    assert.is_true(pass_count(5) >= 3);
  }
}

// -- helpers: trial() stands in for a flaky call ---------------------

function pass_count(trials: int) -> int {
  let passed = 0;
  for (let i = 0; i < trials; i += 1) {
    if (trial(i)) { passed += 1; };
  }
  passed
}

function trial(seed: int) -> bool {
  seed % 7 != 0
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

const TS_ALIAS = `// widget.ts
export class Widget { /* ... */ }

// components/index.ts — the barrel renames it
export { Widget as UIWidget } from '../widget';

// app.ts — the import renames it again
import { UIWidget as W } from './components';
new W();`;

const NS_BAD = `// baml_src/ns_b/b.baml — referencing another namespace, unqualified
function use_widget() -> Widget {
  Widget { label: "x" }
}`;

const NS_GOOD = `// one name, one meaning — and the error told us the name
function use_widget() -> root.a.Widget {
  root.a.Widget { label: "x" }
}`;

// Design-stage syntax (metric blocks) — does NOT compile today, which is
// the point of that section. Rendered read-only, never in a live editor.
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

/* ------------------- captured terminal transcripts ------------------- */

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

// The exact source the pack terminal next to it was built from.
const BAML_PACKED = `function greet(name: string) -> Greeting {
  Greeting { message: "hi, " + name }
}

function main() -> string {
  greet("world").message
}

class Greeting {
  message: string,
}`;

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

const LS_EVENTS: TermEvent[] = [
  { cmd: 'ls baml_src/' },
  { text: 'ns_a  ns_b' },
  { cmd: 'ls baml_src/ns_a/', pause: 0.4 },
  { text: 'a.baml' },
  { pause: 0.4, text: '# the layout of baml_src/ is the layout', tone: 'dim' },
  { text: '# of the program. ls is a map of it.', tone: 'dim' },
];

/* ------------------------------------------------------------------ */

function Section({
  id,
  num,
  kicker,
  width = 'prose',
  tall,
  children,
}: {
  id: string;
  num?: string;
  kicker?: string;
  width?: 'prose' | 'wide' | 'full';
  tall?: boolean;
  children: ReactNode;
}) {
  const widthClass =
    width === 'full' ? 'l4-full' : width === 'wide' ? 'l4-wide' : 'l4-prose';
  return (
    <section className={`l4-section${tall ? ' l4-section--tall' : ''}`} id={id}>
      <div className={widthClass}>
        {kicker ? (
          <p className="l4-kicker">
            {num ? <b>{num}</b> : null}
            {kicker}
          </p>
        ) : null}
        {children}
      </div>
    </section>
  );
}

export function Story() {
  return (
    <div className="l4">
      <header className="l4-head">
        <a className="font-mono" href="/">
          BAML <span>· a programming language for AI software</span>
        </a>
        <span className="l4-head-install font-mono">brew install baml</span>
      </header>

      {/* ---- hero ---- */}
      <Section id="hero" width="prose">
        <div className="l4-hero">
          <div className="l4-hero-mark">
            <Image
              alt=""
              className="l4-hero-sheep"
              height={104}
              priority
              src="/baml-sheep.png"
              width={104}
            />
            <h1 className="l4-hero-title">BAML</h1>
          </div>
          <p className="l4-hero-sub">
            {
              'Everything we build on AI runs on languages designed before AI existed. BAML is a language designed after — a model call is just a typed function, and the type is the schema, the parser, the trace, and the test.'
            }
          </p>
          <p className="l4-hero-tag">
            {'Like TypeScript — without the sins of JavaScript.'}
          </p>
          <p className="l4-hero-sub l4-dim">
            {
              'Built to be written, read, and operated by agents as much as by you.'
            }
          </p>
          <Terminal lines={['brew install baml']} />
          <p className="l4-scroll-hint">scroll ↓</p>
        </div>
      </Section>

      {/* ---- 01 the stack ---- */}
      <Section id="stack" kicker="the stack we build on" num="01" width="wide">
        <h2>{'AI is built on languages from before AI'}</h2>
        <p className="l4-lead">
          {
            'Nobody designed this stack — it accreted. A different vendor at every layer, glued together with strings and JSON: types stop at the SDK, traces live in someone else’s product, rename a function and a dashboard dies silently.'
          }
        </p>
        <p className="l4-lead">
          {
            'You can’t fix it from inside one layer — every fix leaks at the next seam. So we did the unreasonable thing and built the whole column: one language, one runtime, one toolchain.'
          }
        </p>
        <StackTower />
      </Section>

      {/* ---- 02 what baml is ---- */}
      <Section id="what" kicker="what baml is" num="02">
        <h2>{'A programming language for AI software'}</h2>
        <p className="l4-lead">{'Hover anything underlined.'}</p>
        <ul className="l4-feature-list">
          <li>
            <HoverTerm tip="an LLM call is a function: the return type is the schema sent to the model and the parser for its reply">
              native AI primitives
            </HoverTerm>
            {' — model calls are ordinary, typed functions'}
          </li>
          <li>
            <HoverTerm tip="the runtime records a trace of every call — the same graph the compiler sees, so it is exact">
              built-in observability
            </HoverTerm>
            {' — the trace is the call graph, not a vendor integration'}
          </li>
          <li>
            <HoverTerm tip="any plain function can spawn tasks and await them; there is no async keyword in the grammar">
              colorless concurrency
            </HoverTerm>
            {' — fan out without repainting your call stack'}
          </li>
          <li>
            <HoverTerm tip="the compiler works out what every function can throw; catch matches on the type">
              typed, inferred errors
            </HoverTerm>
            {' — you never write throws, and catch is checked'}
          </li>
          <li>
            <HoverTerm tip="generated typed clients for Python and TypeScript; the whole toolchain also runs in this browser tab">
              embeddable runtime
            </HoverTerm>
            {' — one function at a time, inside the app you already have'}
          </li>
          <li>
            <HoverTerm tip="baml describe answers in one call what grep needs four; --budget fits the answer to a context window">
              tooling built for agents
            </HoverTerm>
            {' — the toolchain documents itself, from source'}
          </li>
        </ul>
      </Section>

      {/* ---- 03 the loop changed ---- */}
      <Section id="loop" kicker="the loop changed" num="03" width="wide">
        <h2>{'Agents joined the development loop'}</h2>
        <p className="l4-lead">
          {
            'The loop is no longer write, run, test on your machine. Agents write the code, deploy it, and then have to work out what actually ran. Every step needs information mainstream stacks throw away: types at runtime, the error set of a function, the real call graph.'
          }
        </p>
        <p className="l4-dim">
          {
            'Watch the same question answered both ways. The task: what does greet return, and where is it called from?'
          }
        </p>
        <div className="l4-pair">
          <div>
            <p className="l4-pane-label">agent without describe</p>
            <TermPlay events={GREP_EVENTS} title="agent without describe" />
          </div>
          <div>
            <p className="l4-pane-label l4-pane-label--after">
              agent with describe
            </p>
            <TermPlay events={DESCRIBE_EVENTS} title="agent with describe" />
          </div>
        </div>
        <p className="l4-note">
          {
            'Both sides are real output against the same project. describe also answers for the stdlib and the grammar itself; --budget bounds the answer to fit a context window.'
          }
        </p>
      </Section>

      {/* ---- 04 humans in the loop ---- */}
      <Section
        id="viz"
        kicker="humans, still in the loop"
        num="04"
        tall
        width="full"
      >
        <div className="l4-prose" style={{ margin: '0 auto' }}>
          <h2>{'Agents write more of the code than we do now'}</h2>
          <p className="l4-lead">
            {
              'They read it faster than we do, too. Humans are still in the loop — how do we catch up?'
            }
          </p>
        </div>
        <div style={{ marginTop: '1.6rem' }}>
          <LivePlayground
            initialCode={BAML_IMAGE}
            initialFunction="illustrate"
          />
        </div>
        <p className="l4-statement">
          {'The program '}
          <em>is</em>
          {' the visualization.'}
        </p>
        <p
          className="l4-dim"
          style={{ marginTop: '0.8rem', textAlign: 'center' }}
        >
          {
            'The compiler already knows every call in the program, so the picture is exact and it never goes stale. Edit the code above and run it.'
          }
        </p>
      </Section>

      {/* ---- 05 the language ---- */}
      <Section id="lang" kicker="the language" num="05" width="wide">
        <h2>{'Sixty seconds of BAML'}</h2>
        <p className="l4-lead">
          {
            'Classes, methods, tests. The last expression is the return value. This is a real editor — the compiler, VM, and language server are running in a worker on this page. Hover for types; click ▶ Run test.'
          }
        </p>
        <BamlEditor filename="greeting.baml" initialCode={BAML_HELLO} />
      </Section>

      {/* ---- 06a the AI primitive: the LLM function ---- */}
      <Section
        id="ba-structured"
        kicker="native AI primitives"
        num="06"
        width="wide"
      >
        <h2>{'An LLM call is a language primitive'}</h2>
        <p className="l4-lead">
          {
            'In BAML, calling a model is not an SDK call — it is a function definition. The signature is the contract, the prompt is the body, the client is part of the language. The return type does triple duty: the schema shown to the model, the parser for the reply, and the type your code receives.'
          }
        </p>
        <div className="l4-pair">
          <div>
            <p className="l4-pane-label">before — typescript</p>
            <BamlCode
              code={TS_BOUNDARY}
              diagnostics={[
                { line: 5, message: 'const data: any', severity: 'warning' },
              ]}
              filename="classify.ts"
              highlightLines={[7, 8]}
              lang="typescript"
            />
          </div>
          <div>
            <p className="l4-pane-label l4-pane-label--after">after — baml</p>
            <BamlEditor
              filename="classify.baml"
              highlightLines={[8, 14]}
              initialCode={BAML_SENTIMENT}
            />
          </div>
        </div>
        <p className="l4-note">
          {
            'The reply goes through schema-aligned parsing: malformed JSON — trailing commas, missing quotes, prose wrapped around the payload — is repaired and coerced into Verdict, or fails with a typed error. And there is no any in BAML; there is unknown, and the compiler makes you handle it.'
          }
        </p>
      </Section>

      {/* ---- 06b before/after: parallelism ---- */}
      <Section
        id="ba-parallel"
        kicker="before & after · parallelism"
        num="07"
        width="wide"
      >
        <h2>{'Reads like TypeScript. Schedules like Go.'}</h2>
        <p className="l4-lead">
          {'There is no async keyword in the grammar. '}
          <strong>
            {'Call any function concurrently — even CPU-bound ones.'}
          </strong>
          {
            ' Write spawned async tasks anywhere, without rewriting a chain of parent functions to async.'
          }
        </p>
        <div className="l4-pair">
          <div className="l4-stackv">
            <div>
              <p className="l4-pane-label">before — python</p>
              <BamlCode
                code={PY_GATHER}
                filename="run_all.py"
                highlightLines={[1, 3, 8]}
                lang="python"
              />
            </div>
            <div>
              <p className="l4-pane-label">
                the part promise.all cannot fix — cpu-bound work
              </p>
              <CoreUsage />
            </div>
          </div>
          <div>
            <p className="l4-pane-label l4-pane-label--after">after — baml</p>
            <BamlEditor
              filename="spawn.baml"
              highlightLines={[6, 11]}
              initialCode={BAML_SPAWN}
            />
          </div>
        </div>
        <p className="l4-lead" style={{ marginTop: '2.6rem' }}>
          {
            'And organizing the work is built in. Limiting concurrency in Python means hand-rolling a semaphore and threading it through every call site. In BAML a queue is part of the spawn: groups carry a cap, extras queue in order, and the whole group cancels as a unit.'
          }
        </p>
        <div className="l4-pair">
          <div className="l4-stackv">
            <div>
              <p className="l4-pane-label">before — python, a semaphore</p>
              <BamlCode
                code={PY_SEMAPHORE}
                filename="limit.py"
                highlightLines={[1, 4]}
                lang="python"
              />
            </div>
            <div>
              <p className="l4-pane-label">how the group schedules it</p>
              <PoolSchedule />
            </div>
          </div>
          <div>
            <p className="l4-pane-label l4-pane-label--after">
              after — baml · a group with a cap
            </p>
            <BamlEditor
              filename="shards.baml"
              highlightLines={[3, 5, 17]}
              initialCode={BAML_SPAWN_ADV}
            />
            <p className="l4-note">
              {
                'The group caps the shards at two in flight; the corrupt segment throws; the catch arm recovers its offset — the future carries the error type of its body. Hit Run: this executes here, and main returns 4999953584.'
              }
            </p>
          </div>
        </div>
      </Section>

      {/* ---- 06c before/after: errors ---- */}
      <Section
        id="ba-errors"
        kicker="before & after · errors"
        num="08"
        width="wide"
      >
        <h2>{'The compiler knows the complete error set'}</h2>
        <p className="l4-lead">
          {
            'Code that calls models fails a lot, and error handling is the least typed part of every mainstream language. In BAML you never write throws — the compiler works out what every function can throw, and catch matches on the type.'
          }
        </p>
        <div className="l4-pair">
          <div>
            <p className="l4-pane-label">before — typescript</p>
            <BamlCode
              code={TS_CATCH}
              filename="run.ts"
              highlightLines={[3]}
              lang="typescript"
            />
          </div>
          <div>
            <p className="l4-pane-label l4-pane-label--after">
              after — baml · with a live warning
            </p>
            <BamlEditor filename="fetch.baml" initialCode={BAML_UNREACHABLE} />
          </div>
        </div>
        <p className="l4-note">
          {
            'The warning above is real: the compiler proves the ParseError arm can never fire. Add throws ParseError to fetch_page and it rejects the function for hiding the error it actually throws.'
          }
        </p>
      </Section>

      {/* ---- 07 nondeterminism interlude ---- */}
      <Section id="nondet" kicker="the deeper problem" num="09" width="wide">
        <h2>{'One model call changes everything above it'}</h2>
        <p className="l4-lead">
          {
            'If a function calls a model — even three layers down — its output can change from run to run. assert output == expected stops working for everything above the call, and the language gives you no warning.'
          }
        </p>
        <InfectionGraph />
      </Section>

      {/* ---- 08 before/after: evals ---- */}
      <Section
        id="ba-evals"
        kicker="before & after · evals"
        num="10"
        width="wide"
      >
        <h2>{'Evals are just code'}</h2>
        <p className="l4-lead">
          {
            'Most eval stacks are UIs. BAML evals are code — with types, compiler errors, and git. Anything from an LLM judge to a sampled pass-rate is a typed function and an assert.'
          }
        </p>
        <div className="l4-pair">
          <div className="l4-stackv">
            <div>
              <p className="l4-pane-label">before — a typical eval config</p>
              <BamlCode
                code={YAML_EVAL}
                filename="evals.yaml"
                highlightLines={[3, 11]}
                lang="yaml"
              />
            </div>
            <div>
              <p className="l4-pane-label l4-pane-label--after">
                after — pass-rates are just code
              </p>
              <BamlEditor
                filename="evals.baml"
                highlightLines={[5, 6]}
                initialCode={BAML_PASSRATE}
              />
            </div>
          </div>
          <div>
            <p className="l4-pane-label l4-pane-label--after">
              after — baml · run these here
            </p>
            <BamlEditor filename="tests.baml" initialCode={BAML_TEST} />
          </div>
        </div>
      </Section>

      {/* ---- the agent loop, observed ---- */}
      <Section
        id="agent-loop"
        kicker="agents, the workload"
        num="11"
        tall
        width="full"
      >
        <div className="l4-prose" style={{ margin: '0 auto' }}>
          <h2>{'Write the agent loop in the language'}</h2>
          <p className="l4-lead">
            {
              'An agent is a typed turn loop: the model decides one Step, a match dispatches it to a tool, the result is observed back into the transcript. Below is a complete one — and because the loop is ordinary BAML, the graph shows exactly what it does.'
            }
          </p>
        </div>
        <div style={{ marginTop: '1.6rem' }}>
          <LivePlayground initialCode={BAML_AGENT} initialFunction="run_turn" />
        </div>
        <p
          className="l4-dim"
          style={{ marginTop: '0.8rem', textAlign: 'center' }}
        >
          {
            'decide → execute → observe. Tools are plain functions, dispatch is a match on a literal union, and the loop is bounded at eight steps.'
          }
        </p>
      </Section>

      {/* ---- 12 designed to be read by agents ---- */}
      <Section
        id="agents"
        kicker="designed to be read by agents"
        num="12"
        width="wide"
      >
        <h2>{'The language is legible by construction'}</h2>
        <p className="l4-lead">
          {
            'Most of an agent’s work is reading. In TypeScript, the same class can travel under three names before it reaches the call site. BAML is laid out so one name means one thing.'
          }
        </p>
        <div className="l4-pair">
          <div className="l4-stackv">
            <p className="l4-pane-label">typescript — one class, three names</p>
            <BamlCode
              code={TS_ALIAS}
              filename="three files"
              highlightLines={[5, 8]}
              lang="typescript"
              notes={[{ line: 9, text: 'grep "Widget" never finds this' }]}
            />
            <p className="l4-dim">
              {
                'Every rename is invisible to text search. An agent greps Widget, gets widget.ts, and misses the call site entirely — unless it stops to spin up a language server.'
              }
            </p>
          </div>
          <div className="l4-stackv">
            <div>
              <p className="l4-pane-label l4-pane-label--after">
                baml — aliasing does not exist
              </p>
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
            <div>
              <BamlCode
                code={NS_GOOD}
                filename="ns_b/b.baml"
                highlightLines={[2, 3]}
              />
            </div>
            <p className="l4-dim">
              {
                'Cross-namespace names are always written in full, so every reference greps. The compiler’s own errors teach the canonical name.'
              }
            </p>
          </div>
        </div>
        <div style={{ marginTop: '1.4rem' }}>
          <p className="l4-pane-label">and the namespaces are just files</p>
          <TermPlay events={LS_EVENTS} title="the filesystem is the map" />
        </div>
        <p className="l4-note">
          {
            'baml describe is generated from source, so it is up to date by construction — there are no docs to drift.'
          }
        </p>
      </Section>

      {/* ---- 10 pack ---- */}
      <Section id="pack" kicker="programs are shareable" num="13" width="wide">
        <h2>{'Programs should be shareable instantly'}</h2>
        <p className="l4-lead">
          {
            'baml pack turns any function into a self-contained binary — 9.8 MB, 4.7 MB compressed, built in a fifth of a second. The CLI is derived from the function’s signature: each parameter becomes a flag, typed and documented. An agent can mint a tool, hand it to another agent, and --help already explains it.'
          }
        </p>
        <div className="l4-pair">
          <div className="l4-stackv">
            <p className="l4-pane-label">the source — one parameter</p>
            <BamlCode
              code={BAML_PACKED}
              filename="main.baml"
              highlightLines={[1]}
              notes={[{ line: 1, text: 'name: string → --name <flag>' }]}
            />
            <p className="l4-dim">
              {
                'Nothing here mentions a CLI. The argument parser, the subcommands, and the help text all come from the signatures.'
              }
            </p>
          </div>
          <div>
            <p className="l4-pane-label l4-pane-label--after">
              pack it, run it, ask it for help
            </p>
            <TermPlay events={PACK_EVENTS} title="baml pack" />
          </div>
        </div>
      </Section>

      {/* ---- 11 embed ---- */}
      <Section
        id="embed"
        kicker="in the app you already have"
        num="14"
        width="wide"
      >
        <h2>{'Adopt it one function at a time'}</h2>
        <p className="l4-lead">
          {
            'baml generate writes a typed client into your project. Your app imports a package; nothing else about it changes.'
          }
        </p>
        <SdkPipeline />
        <p className="l4-lead">
          {
            'The types come out native. A BAML class is a pydantic model in Python and a typed class in TypeScript — methods included.'
          }
        </p>
        <RotatingTypes />
        <p className="l4-note">
          {
            'Python and TypeScript today; Go and Ruby are in progress. And it runs the other direction too: this page is running the full compiler, VM, and language server in a browser worker.'
          }
        </p>
      </Section>

      {/* ---- two-way ---- */}
      <Section id="twoway" kicker="two-way" num="15" width="wide">
        <h2>{'Functions go in, too'}</h2>
        <p className="l4-lead">
          {
            'The boundary runs both directions. A BAML function can take a function-typed parameter, and the host fills it with a plain Python function — BAML calls back into your code mid-run.'
          }
        </p>
        <div className="l4-pair">
          <div>
            <p className="l4-pane-label">baml — a function-typed parameter</p>
            <BamlEditor
              filename="tally.baml"
              highlightLines={[2]}
              initialCode={BAML_TALLY}
            />
          </div>
          <div>
            <p className="l4-pane-label l4-pane-label--after">
              python — pass a plain function
            </p>
            <BamlCode
              code={PY_CALLBACK}
              filename="app.py"
              highlightLines={[6]}
              lang="python"
            />
          </div>
        </div>
        <p className="l4-note">
          {
            'From the SDK test suite: callables register across the bridge automatically, and an exception raised inside one round-trips as the identical Python object.'
          }
        </p>
      </Section>

      {/* ---- 12 metrics design ---- */}
      <Section id="metrics" kicker="where this goes" num="16" width="wide">
        <span className="l3-design-tag font-mono">
          design proposal — not shipped
        </span>
        <h2>{'The program is also the dashboard'}</h2>
        <p className="l4-lead">
          {
            'Metrics today live in dashboards, bound to code by strings — rename a function and the metric dies silently. We are designing metric blocks: attach one to a function and it carries typed measurements, wired into a dependency graph that computes as data arrives — even hours later, when a human label shows up.'
          }
        </p>
        <MetricsDag />
        <div className="l4-pair" style={{ marginTop: '1.4rem' }}>
          <BamlCode
            code={BAML_METRIC}
            filename="resume.baml (proposed)"
            highlightLines={[10, 15]}
            lang="baml"
          />
          <p className="l4-note" style={{ marginTop: 0 }}>
            {
              'Parameter names are the edges of the graph: quality(judge) depends on judge; f1(precision, recall) waits for both. The function call is the root. This is the part we want torn apart — the runtime already records a trace of every call; this design is the layer on top.'
            }
          </p>
        </div>
      </Section>

      {/* ---- 13 tradeoffs ---- */}
      <Section id="tradeoffs" kicker="the honest part" num="17">
        <h2>{'What you are signing up for'}</h2>
        <ul className="l4-feature-list">
          <li>
            {
              'A new language. Models have not seen much BAML in training — the toolchain makes up for it, but it is a real cost.'
            }
          </li>
          <li>
            {
              'Two first-class SDK targets today: Python and TypeScript. Go and Ruby are in progress.'
            }
          </li>
          <li>{'The metrics design above is not built.'}</li>
          <li>{'Pre-1.0: the language still changes.'}</li>
        </ul>
        <p className="l4-note">
          {
            'Adoption is incremental by construction: one function behind a generated client. Your app does not move; one call site does.'
          }
        </p>
      </Section>

      {/* ---- close ---- */}
      <Section id="close">
        <div className="l4-close">
          <h2>{'Try it'}</h2>
          <Terminal lines={['brew install baml']} />
          <a
            className="l4-close-link"
            href="https://new.boundaryml.com/quickstart"
            rel="noreferrer"
            target="_blank"
          >
            new.boundaryml.com/quickstart →
          </a>
        </div>
      </Section>
    </div>
  );
}
