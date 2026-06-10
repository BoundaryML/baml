import type { TermEvent } from '../../learn3/_components/TermPlay';

/* ------------------------------------------------------------------ *
 * Snippets. Every BAML_* snippet passes `baml check` (toolchain 0.11.x,
 * the dialect the live editors run); the intentional diagnostics
 * (BAML_UNREACHABLE → E0063 warning, NS_BAD → "Did you mean
 * `root.a.Widget`?") show the compiler's real messages. Terminal
 * transcripts are captured CLI output. Re-verify with `baml check`
 * if you edit anything. (Shared provenance with learn4.)
 * ------------------------------------------------------------------ */

export const BAML_HELLO = `class Greeting {
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

export const BAML_SENTIMENT = `type Label = "positive" | "negative" | "neutral";

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

export const BAML_SPAWN = `// A plain function -- nothing marks it async. It launches N tasks
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

export const BAML_SPAWN_ADV = `// cap shard work at two in flight; extras queue fifo in the group
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

export const BAML_TALLY = `// the first parameter is a function type -- the host fills it in
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

export const PY_CALLBACK = `from baml_sdk import tally

def score(x: int) -> int:        # plain python
    return x * 2

tally(score=score, xs=[1, 2, 3])   # -> 12, baml calling python
# raise inside score() and the SAME exception
# object surfaces back here -- not a copy`;

export const PY_SDK = `# after \`baml generate\`: import the typed client
from baml_sdk import classify

verdict = classify(text="absolutely loved it!")
verdict.label        # "positive" -- a literal-union field
verdict.confidence   # float -- Verdict is a pydantic model`;

export const BAML_UNREACHABLE = `// Nothing is declared below, yet the compiler knows fetch_page
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

export const BAML_AGENT = `// A tiny agent: decide -> execute -> observe, in a typed turn loop.
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

export const BAML_TEST = `testset "basics" {
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

export const BAML_PASSRATE = `// Nondeterministic results? Run the function N times and
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

export const BAML_IMAGE = `function generate_image(thing: string) -> image {
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

export const NS_BAD = `// baml_src/ns_b/b.baml — referencing another namespace, unqualified
function use_widget() -> Widget {
  Widget { label: "x" }
}`;

export const NS_GOOD = `// one name, one meaning — and the error told us the name
function use_widget() -> root.a.Widget {
  root.a.Widget { label: "x" }
}`;

// The exact source the pack terminal next to it was built from.
export const BAML_PACKED = `function greet(name: string) -> Greeting {
  Greeting { message: "hi, " + name }
}

function main() -> string {
  greet("world").message
}

class Greeting {
  message: string,
}`;

/* ------------------- captured terminal transcripts ------------------- */

export const DESCRIBE_EVENTS: TermEvent[] = [
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

export const PACK_EVENTS: TermEvent[] = [
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

export const LS_EVENTS: TermEvent[] = [
  { cmd: 'ls baml_src/' },
  { text: 'ns_a  ns_b' },
  { cmd: 'ls baml_src/ns_a/', pause: 0.4 },
  { text: 'a.baml' },
  { pause: 0.4, text: '# the layout of baml_src/ is the layout', tone: 'dim' },
  { text: '# of the program. ls is a map of it.', tone: 'dim' },
];
