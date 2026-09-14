import type { TermEvent } from '../../learn3/_components/TermPlay';

/* ------------------------------------------------------------------ *
 * Snippets. Every BAML_* snippet is verified against the release
 * toolchain on this branch (baml 0.11.3-nightly.20260610); most pass
 * `baml check`/`baml test` clean. The intentional diagnostics —
 * BAML_UNKNOWN (E0007, field access on `unknown`), BAML_UNREACHABLE
 * (E0063, dead catch arm), NS_BAD (unresolved type) — show the
 * compiler's real messages. Terminal transcripts are captured
 * CLI output. Benchmark numbers are real measurements from an
 * 18-core Apple Silicon machine — see the comments on BENCH_* below.
 * Re-verify with `baml check` if you edit anything.
 * ------------------------------------------------------------------ */

/* ---------------- 1a · types exist at runtime ---------------- */

export const TS_LIES = `interface User {
  name: string;
  email: string;
}

// 'as' is an unchecked promise the compiler believes
const user = JSON.parse(raw) as User;
user.email.toLowerCase();`;

export const BAML_UNKNOWN = `class User {
  name: string,
  email: string,
}

function load(raw: unknown) -> string {
  if (raw is User) {
    return raw.email.to_lower_case();
  } else {
    // this fails!
    // there is no \`as\`, no \`any\`
    // \`raw\` must be proven to a \`User\`
    raw.email.to_lower_case();

    throw "failed!";
  }
}`;

/* ---------------- 1b · error handling ---------------- */

export const TS_CATCH = `function show(ok: boolean) {
  try {
    return fetch_page(ok);
  } catch (e) {
    // e: unknown -- TS can't tell you what fetch_page throws
    if (e instanceof NetError) return "recovered: " + e.detail;
  }
}`;

export const BAML_UNREACHABLE = `function show(ok: bool) -> string {
  fetch_page(ok) catch (e) {
    NetError => "recovered: " + e.detail,
    ParseError => "unreachable",
  }
}

// -- the rest is plumbing --
function fetch_page(ok: bool) -> string {
  if (!ok) { throw NetError { detail: "timeout" } };
  "<html>"
}

class NetError { detail: string }
class ParseError { detail: string }`;

/* ---------------- 1c · match on types or values ---------------- */

export const TS_INSTANCEOF = `function route(msg: Refund | Question | string) {
  // a grab-bag of typeof + "key in obj" checks -- no real
  // match, so overlapping keys quietly pick the wrong arm
  if (typeof msg === "string") return "text: " + msg;
  if ("id" in msg) return "refund " + msg.id;
  if ("text" in msg) return "answer: " + msg.text;
  // miss a case -> silently returns undefined
}

interface Refund { id: string }
interface Question { text: string }`;

export const BAML_MATCH = `function route(msg: Refund | Question | string) -> string {
  match (msg) {
    Refund => \`refund \${msg.id}\`,
    // with destructuring!
    Question { text } => \`answer: \${text}\`,
    string => \`text: \${msg}\`,
  }
}

// ...or match on VALUES, with guards
function grade(n: int) -> string {
  match (n) {
    100 => "perfect",
    let s if s >= 60 => "pass",
    _ => "fail",
  }
}

class Refund { id: string }
class Question { text: string }`;

/* ---------------- 1d · eval / codemode (coming soon) ----------------
 * Direction-of-travel only; the reflection API below is not yet shipped. */
export const BAML_EVAL = `let raw = reflect.new_package("my_package");
baml.package.set_file("virtual/path/to/file.baml", \`
   function hello() -> string {
     "hello world"
   }
\`)
let pkg = raw.build();

let cb = pkg.get<() -> string>("hello");
print(cb());

// and its typesafe!
let cb = pkg.get<() -> int>("hello") catch (e) {
    reflect.CompilerTypeError => {
        print(\`"hello" is not a function that returns int. \${e}\`)
    }
};`;

/* ---------------- 1e · sandboxing via function mocking (coming soon) ----
 * Direction-of-travel only; baml.mock (BEP-058) is not yet shipped. */
export const BAML_SANDBOX = `// inside a mock scope, baml.http.fetch is whatever you say it is.
let net = baml.mock.new(baml.http.fetch);
net.replace((req: baml.http.Request) -> baml.http.Response {
  // lets ban fetch! so even if the llm uses it, we get an error
  throw baml.NotImplementedError { message: "fetch is disabled in this scope" };
});

let shell = baml.mock.new(baml.sys.shell);
shell.replace((command: string) -> baml.std.ShellOut {
  // lets ban shell! so even if the llm uses it, we get an error
  throw baml.NotImplementedError { message: "shell is disabled in this scope" };
});


baml.mock.scope([net, shell], () -> void {
  run_generated();   // every fetch/shell in here hits the stand-in
});

// out here, fetch and shell are the real thing again -- the scope undoes itself.`;

/* ---------------- 2 · namespaces are directories ---------------- */

export const NS_BAD = `// baml_src/ns_orders/order.baml — referencing another namespace, unqualified
function line_item() -> Product {
  Product { name: "Keyboard" }
}`;

export const NS_GOOD = `// there's a single fully qualified name, every time
function line_item() -> root.catalog.Product {
  root.catalog.Product { name: "Keyboard" }
}`;

export const LS_EVENTS: TermEvent[] = [
  { cmd: 'ls baml_src/' },
  { text: 'ns_catalog  ns_orders' },
  { cmd: 'ls baml_src/ns_catalog/', pause: 0.4 },
  { text: 'product.baml' },
  { pause: 0.4, text: '# the layout of baml_src/ is the layout', tone: 'dim' },
  { text: '# of the program. ls is a map of it.', tone: 'dim' },
];

/* ---------------- 3 · baml describe ---------------- */

export const GREP_EVENTS: TermEvent[] = [
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
  {
    pause: 0.5,
    text: '# 3 tool calls, a whole file in context — and the',
    tone: 'dim',
  },
  { text: '# caller list is still just text matches', tone: 'dim' },
];

// Captured from `baml describe greet` on this branch's release CLI.
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
    text: '✓ baml describe gives you signature, deps, every reference',
    tone: 'ok',
  },
];

/* ---------------- 4 · baml pack ---------------- */

export const BAML_PACKED = `function greet(name: string) -> Greeting {
  Greeting { message: "hi, " + name }
}

function main() -> string {
  greet("world").message
}

class Greeting {
  message: string,
}`;

export const PACK_EVENTS: TermEvent[] = [
  { cmd: 'baml pack -f greet -f main -o greet-bin' },
  { text: '   Packaging greet,main', tone: 'dim' },
  {
    text: '    Finished greet-bin [greet,main, aarch64-apple-darwin] in 0s',
    tone: 'ok',
  },
  { cmd: 'ls -lah greet-bin', pause: 0.4 },
  { text: '7.9M greet-bin' },
  { cmd: './greet-bin greet --name "hacker news"', pause: 0.4 },
  { text: '{"message":"hi, hacker news"}', tone: 'accent' },
  { cmd: './greet-bin --help', pause: 0.4 },
  { text: 'Usage: greet-bin <COMMAND>' },
  { text: '' },
  { text: 'Commands:' },
  { text: '  greet  function greet(name: string) -> Greeting' },
  { text: '  main   function main() -> string' },
];

/* Measured 2026-06-10, Apple Silicon (18 logical cores), same hello world
 * compiled both ways. Bun 1.3.14 `bun build --compile`; BAML release
 * `baml pack`. Startup = median of 20 runs, idle machine. */
export const PACK_BENCH = [
  {
    accent: true,
    gzip: '3.4 MB',
    size: '7.9 MB',
    startup: '5.2 ms',
    tool: 'baml pack',
  },
  {
    accent: false,
    gzip: '22.8 MB',
    size: '60.5 MB',
    startup: '7.6 ms',
    tool: 'bun build --compile',
  },
] as const;

/* ---------------- 4b · baml run <function> ---------------- *
 * Captured from `baml run` on a throwaway `baml init` project whose
 * main.baml is the same greet/main/Greeting program shown above. The
 * `user.` package prefix the CLI currently prints on class values is a
 * known display wart and is omitted here. */
export const RUN_FN_EVENTS: TermEvent[] = [
  { cmd: 'baml run main' },
  { text: '   Compiling 1 file(s)', tone: 'dim' },
  { text: '    Compiled 1 file(s) in 1s', tone: 'dim' },
  { text: '"hi, world"', tone: 'accent' },
  { cmd: 'baml run greet -- --name "hacker news"', pause: 0.5 },
  { text: 'Greeting { message: "hi, hacker news" }', tone: 'accent' },
  { cmd: 'baml run greet -- --help', pause: 0.5 },
  { text: 'function greet(name: string) -> Greeting' },
  { text: 'Options:' },
  { text: '      --name <string>' },
  {
    pause: 0.4,
    text: '# any function is a target; its params become --flags',
    tone: 'dim',
  },
];

/* ---------------- 5 · baml run -e ---------------- */

// Captured output from this branch's release CLI.
export const RUN_E_EVENTS: TermEvent[] = [
  { cmd: `baml run -e '"a,b,c".split(",")'` },
  { text: '   Compiling expression', tone: 'dim' },
  { text: '     Running expression', tone: 'dim' },
  { text: '["a", "b", "c"]', tone: 'accent' },
  {
    cmd: `baml run -e '{ let t = 0; for (let i = 0; i < 5; i += 1) { t += i * i; }; t }'`,
    pause: 0.5,
  },
  { text: '30', tone: 'accent' },
  { text: '    Finished expression in 0s', tone: 'ok' },
  {
    pause: 0.4,
    text: '# no file, no project — paste, run, observe',
    tone: 'dim',
  },
];

/* ---------------- 6 · green threads ---------------- */

export const BAML_SPAWN = `// Nothing marks this function async. spawn runs any
// call in parallel; await joins the result.
function main() -> int {
  let a = spawn { work(1) };
  let b = spawn { work(2) };
  let c = spawn { work(3) };

  (await a) + (await b) + (await c)
}

// stands in for any slow task (LLM call, IO, ...)
function work(i: int) -> int {
  i * i
}

test "spawn and await" {
  assert.equal(main(), 14);
}`;

/* The exact benchmark source (the Bun version is the same code shape
 * with String.prototype.includes). 16 shards × 50 rounds × ~48 MB =
 * 38.4 GB of text scanned. */
export const BENCH_BAML = `// one "log shard": ~48 MB of text, scanned \`rounds\`
// times for a marker -- worst case: it never appears,
// so every scan reads all 48 MB
function scan_shard(id: int, rounds: int) -> int {
  let hay = make_shard();
  let hits = 0;
  for (let i = 0; i < rounds; i += 1) {
    if (hay.includes("ERROR RATE EATER")) {
      hits += 1;
    };
  }
  hits
}

// the parallel version: one spawn per shard
function par(shards: int, rounds: int) -> int {
  let tasks = [];
  for (let s = 0; s < shards; s += 1) {
    tasks.push(spawn_shard(s, rounds));
  }
  let total = 0;
  for (let t in tasks) {
    total += await t;
  }
  total
}

// -- helpers below ---------------------------------

function spawn_shard(id: int, rounds: int) -> baml.future.Future<int, never> {
  spawn { scan_shard(id, rounds) }
}

function make_shard() -> string {
  let line = "ERA EAGER ERRAND EATER ERROR RATED EARNEST ERROR RACER ERRATA REARED ROARER RETREAT TERRACE ".repeat(11);
  line.repeat(49152)
}`;

/* Measured 2026-06-10: 38.4 GB scanned (16 shards × 50 rounds × 48 MB),
 * 18-core Apple Silicon, release CLI, Bun 1.3.14. Stable across runs.
 * The needle's bytes saturate the haystack, so the search does real
 * verification work instead of degenerating to memchr at memory
 * bandwidth (where both runtimes tie). */
export const SPAWN_BENCH = [
  {
    accent: false,
    cpu: '100% (1 core)',
    run: 'Bun, one thread',
    time: '8.2 s',
  },
  {
    accent: false,
    cpu: '100% (1 core)',
    run: 'BAML, one thread',
    time: '6.8 s',
  },
  {
    accent: true,
    cpu: '1008% (10 cores)',
    run: 'BAML, spawn ×16',
    time: '0.87 s',
  },
] as const;

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

/* ---------------- AI workflows ---------------- */

export const BAML_SENTIMENT = `type Label = "positive" | "negative" | "neutral";

class Verdict {
  label: Label,
  confidence: float,
}

function classify(text: string) -> Verdict {
  client: "openai/gpt5.5"
  prompt: \`
    Classify the sentiment of the text. Sarcasm counts
    as the sentiment actually expressed.
    Text: \${text}
    \${ctx.output_format()}
  \`
}`;

// `illustrate` lives at the top so the entry point is the first thing you
// read; the WorkflowPlayground highlights its lines (2-5) by default.
export const BAML_IMAGE = `// the pipeline: generate an image, then have an LLM describe it
function illustrate() -> string {
  let img = generate_image("a purple lamb");
  describe(img)
}

function generate_image(thing: string) -> image {
  client: AiGatewayImagen
  prompt: \`
    Create an image from this prompt: \${thing}
    \${ctx.output_format()}
  \`
}

function describe(img: image) -> string {
  client: "openai/gpt5.5"
  prompt: \`
    Describe this image in one vivid sentence.
    \${img}
    \${ctx.output_format()}
  \`
}

client AiGatewayImagen {
  provider: ai-gateway-images,
  options: {
    model: "google/imagen-4.0-fast-generate-001",
    api_key: env.AI_GATEWAY_API_KEY,
  }
}`;

// Non-LLM, runnable workflow: classify each line, then tally the results.
// `//#` comments add nodes to the playground graph (it renders header-anchored
// nodes + LLM calls + branch arms under a header). `sentiment` is bound to a
// `let` so the call gets its own node and expands inline (showing its own
// `//#` nodes). Entry point `summarize` is lines 2-18; verified check/test pass.
export const BAML_WF_TALLY = `// a non-LLM workflow: classify each line, then tally the results
function summarize(raw: string) -> Tally {
  //# split the input into lines
  let lines = raw.split("\\n");
  let pos = 0;
  let neg = 0;
  for (let line in lines) {
    //# classify each line
    let label = sentiment(line);
    if (label == "positive") {
      pos += 1;
    } else {
      neg += 1;
    };
  }
  //# build the tally
  Tally { positive: pos, negative: neg }
}

//# classify one line of text
function sentiment(text: string) -> string {
  let t = text.to_lower_case();
  //# check sentiment
  if (t.includes("love") || t.includes("great") || t.includes("amazing")) {
    "positive"
  } else {
    "negative"
  }
}

class Tally {
  positive: int,
  negative: int,
}

test "summarize tallies sentiment" {
  let t = summarize("loved it\\nterrible\\ngreat job");
  assert.equal(t.positive, 2);
  assert.equal(t.negative, 1);
}`;

// Non-LLM, runnable workflow: fan three tasks across green threads, then
// combine. `//#` comments add graph nodes. Entry point `analyze` is lines 2-9.
// Verified `baml check`/`baml test` pass.
export const BAML_WF_FANOUT = `// fan three tasks out across green threads, then combine the results
function analyze(n: int) -> int {
  //# fan out across green threads
  let a = spawn { score(n) };
  let b = spawn { score(n + 1) };
  let c = spawn { score(n + 2) };
  //# join and combine the results
  (await a) + (await b) + (await c)
}

//# crunch one shard of work
function score(seed: int) -> int {
  let total = 0;
  for (let i = 0; i < seed * 1000; i += 1) {
    total += i % 7;
  }
  total
}

test "analyze fans out and combines" {
  assert.is_true(analyze(3) > 0);
}`;

export const BAML_CSV_TESTS = `function classify(text: string) -> string {
  let t = text.to_lower_case();
  if (t.includes("love") || t.includes("great")) {
    "positive"
  } else {
    "negative"
  }
}

// testsets are code: loop over data and mint one
// test per row
testset "from a csv" {
  let rows = "text,expect
loved it,positive
absolutely great,positive
worst purchase ever,negative".split("\\n");

  for (let row in rows.slice(1, rows.length())) {
    let cols = row.split(",");
    test ("classify: " + cols[0]) {
      assert.equal(classify(cols[0]), cols[1]);
    }
  }
}`;

// From the toolchain's own test corpus: a testset that fetches its
// cases over HTTP at collection time (S3, a labeling service, anywhere).
export const BAML_HTTP_TESTS = `testset "from object storage" {
  let res = baml.http.fetch("https://datasets.example.com/golden.csv");
  let rows = res.text().split("\\n");

  for (let row in rows.slice(1, rows.length())) {
    let cols = row.split(",");
    test ("golden: " + cols[0]) {
      assert.equal(classify(cols[0]), cols[1]);
    }
  }
}`;

export const BAML_RUNNER = `// the runner attaches with \`with\` -- this body runs
// 5 times per \`baml test\`, and 3 passes are enough
test "tolerates flaky runs" with quorum {
  assert.is_true(check_inventory());
}

// stands in for a nondeterministic check (LLM call, ...)
function check_inventory() -> bool {
  true
}

// -- the runner ------------------------------------

// A custom test runner is just a function: it takes the
// base "run the test once" thunk and returns a new one.
// This one runs the body 5 times and passes on a quorum.
function quorum(base: testing.TestReportThunk) -> testing.TestReportThunk {
  let run: testing.TestReportThunk = () -> testing.TestReport {
    let runs: testing.RunReport[] = [];
    let passed = 0;
    for (let i = 0; i < 5; i += 1) {
      let report = run_once(base);
      for (let r in report.runs) {
        runs.push(r);
      }
      if (report.outcome == "pass") {
        passed += 1;
      };
    }
    testing.TestReport {
      outcome: if (passed >= 3) { "pass" } else { "fail" },
      runs: runs,
    }
  };
  run
}

// one guarded execution of the base thunk
function run_once(base: testing.TestReportThunk) -> testing.TestReport {
  {
    base()
  } catch_all (e) {
    _ => testing.TestReport { outcome: "fail", runs: [] }
  }
}`;

// Design-stage syntax (metric blocks) — does NOT compile today, which is
// the point of that section. Rendered read-only, never in a live editor.
export const BAML_METRIC = `function extract_resume(raw: string) -> Resume { ... }

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

/* A fuller, real-world BAML project for the "navigating a codebase" section:
 * an LLM "Heads Up" guessing game (functions + classes + an agent loop),
 * plus a non-LLM binary search and two testsets. The graph view is the point —
 * it's the visual counterpart to \`baml describe\`. */
export const NAV_CODEBASE = `// Interactive demo: uses baml.io.input — run from the playground, not headless CI.
function GuessGameAgent() -> GuessResponse {
    // comments
    let history: Message[] = [];
    //# set up system
    let famous_person_name = generate_famous_person_name([]);

    let user_input = "Is it Marie Curie?";

    let guess_response = take_guess("", famous_person_name, history);
    //# update history
    history.push(Message { role: "user", content: user_input });
    history.push(Message { role: "assistant", content: guess_response.text });

    let max_guesses = 10;
    while (!guess_response.game_won && max_guesses > 0) {
        if (guess_response.game_won) {
            break;
        } else {
            //# Bad Guess
            user_input = simulate_human_guess(history);
            //# take guess
            guess_response = take_guess(user_input, famous_person_name, history);
            log.info({ "user_input": user_input, "guess_response": guess_response.text });

            history.push(Message { role: "user", content: user_input });
            history.push(Message { role: "assistant", content: guess_response.text });

            max_guesses = max_guesses - 1;
        }
    }

    if (guess_response.game_won) {
        log.info({ "game_won": true });
    } else {
        log.info({ "game_won": false });
    }

    guess_response
}

function generate_famous_person_name(previous_names: string[]) -> string {
    client: "openai-responses/gpt-5.5"
    prompt: \`
        \${role("user")}
        You are a famous person generator for a "Heads Up" guessing game.

        Generate the name of a well-known famous person who:
        - Is recognizable to most people
        - Has distinctive characteristics that can be described with yes/no questions
        - Is appropriate for all audiences
        - Has a clear, unambiguous name

        IMPORTANT: Check the list of what famous people you've already suggested
        and NEVER repeat a person you've already suggested.

        Already suggested names:
        \${previous_names}

        Examples: Albert Einstein, Beyoncé, Leonardo da Vinci, Oprah Winfrey, Michael Jordan

        Return only the person's name, nothing else.
    \`
}

class GuessResponse {
    game_won: bool,
    text: string,
}

class Message {
    role: "user" | "assistant",
    content: string,
}

function take_guess(
    user_guess: string,
    famous_person_name: string,
    history: Message[],
) -> GuessResponse {
    client: "openai-responses/gpt-5.5"
    prompt: \`
        You are a helpful game assistant for a "Heads Up" guessing game.

        CRITICAL: You know the famous person's name but you must NEVER reveal it in any response.

        When a user asks a question about the famous person:
        - Answer truthfully based on the famous person provided
        - Keep responses concise and friendly
        - NEVER mention the person's name, even if it seems natural
        - NEVER reveal gender, nationality, or other characteristics unless specifically asked about them
        - Answer yes/no questions with clear "Yes" or "No" responses
        - Be consistent - same question asked differently should get the same answer
        - Ask for clarification if a question is unclear
        - If multiple questions are asked at once, ask them to ask one at a time

        When they make a guess:
        - If correct: Congratulate them warmly
        - If incorrect: Politely correct them and encourage them to try again

        Encourage players to make a guess when they seem to have enough information.

        \${ctx.output_format()}

        Conversation history:

        \${history}

        Famous person:

        \${famous_person_name}

        Here's the user input:

        \${user_guess}
    \`
}

function simulate_human_guess(history: Message[]) -> string {
    client: "openai-responses/gpt-5.5"
    prompt: \`
        You are playing a "Heads Up" guessing game. Given the conversation history,
        you must take a guess at the famous person's name or ask a question about them.

        Conversation history:

        \${history}
    \`
}

class Memory {
    user_id: string,
    memory: string,

    function update(self, new_memory: string) -> void {
        self.memory = self.memory + "\\n" + new_memory;
        return;
    }
}

function test_image() -> image {
    image
        .from_url("https://upload.wikimedia.org/wikipedia/commons/2/2e/George-Washington.jpg", null)
}

function BinarySearch(array: int[], target: int) -> int {
    let left = 0;
    let right = array.length() - 1;

    while (left <= right) {
        let mid = (left + right) / 2;
        if (array[mid] == target) {
            return mid;
        }
        if (target < array[mid]) {
            right = mid - 1;
        } else {
            left = mid + 1;
        }
    }

    return -1;
}

function print_number(number: int) -> int {
    return 1;
}

testset "game_llm" {
    test "generates famous person" {
        let name = generate_famous_person_name(["George Orwell", "Albert Einstein"]);
        assert.is_true(name.trim().length() > 0);
    }

    test "take_guess responds to a question" {
        let r = take_guess("Was this person a scientist?", "Marie Curie", []);
        assert.is_true(r.text.trim().length() > 0);
    }

    test "take_guess can recognize a correct guess" {
        let r = take_guess("Is it Marie Curie?", "Marie Curie", []);
        assert.is_true(r.game_won);
    }

    test "simulates human guess" {
        let guess = simulate_human_guess(
            [
                Message { role: "assistant", content: "Yes, this person was a scientist." },
                Message { role: "assistant", content: "No, this person was not American." },
            ],
        );
        assert.is_true(guess.trim().length() > 0);
    }
}

testset "binary_search_logic" {
    test "finds_target_in_longer_array" {
        let idx = BinarySearch(
            [
                12, 34, 56, 67, 89, 90, 100, 112, 134, 156, 178, 190, 200, 212,
                234, 256, 278, 290, 300, 312, 334, 356, 378, 390, 400, 412, 434,
                456, 478, 490, 500, 512, 534, 556, 578, 590, 600, 612, 634, 656,
                678, 690, 700, 712, 734, 756, 778, 790, 800, 812, 834, 856, 878,
                890, 900, 912, 934, 956, 978, 990, 1000,
            ],
            34,
        );
        assert.equal(idx, 1);
    }

    for (let i = 0; i < 10; i += 1) {
        test \`finds_present_target\${i}\` {
            let idx = BinarySearch([1, 2, 3, 4, 5], 3);
            assert.equal(idx, 2);
        }
    }

    test "returns_negative_one_when_missing" {
        let idx = BinarySearch([1, 2, 3], 99);
        assert.equal(idx, -1);
    }
}`;
