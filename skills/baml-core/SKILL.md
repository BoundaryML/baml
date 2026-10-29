---
name: baml-core
description: "Use when writing, editing, or reviewing BAML code. BAML is a statically-typed, expression-oriented language with first-class LLM functions: TypeScript-like syntax, snake_case methods, Rust-like strictness. Covers the CLI, the language and its traps, LLM functions, tests, and style guidelines. Useful for building ai workflows, agents, evals."
metadata:
  baml-toolchain-version: "{{BAML_TOOLCHAIN_VERSION}}"
---

# baml

BAML is a statically-typed, expression-oriented *language* with `snake_case` methods, `name: type,` fields, enums, interfaces, generics, closures, optional chaining, backtick strings with `${...}` interpolation, `.to_string()` on any value, a real stdlib. And a declarative *DSL* for LLM calls (`function … { client: prompt: }`, `test`) that desugars into it, so a model's structured output is just a typed return value.

It largely behaves like something between TypeScript and Rust: the syntax reads like TypeScript, and the semantics are stricter, closer to Rust (exhaustive `match`, typed errors). Write it in Rust's functional style: expressions instead of statements, `map`/`filter`/`reduce` instead of loops that mutate an accumulator, and `match` instead of `if` chains. Errors are thrown and declared with `throws`, which plays the role of Rust's `Result`; don't return sentinel values or error fields.

**ALWAYS follow the [Style guidelines](#style-guidelines)**, in new code and in edits. The [Examples](#examples) follow them too.

## CLI and workflow

**The CLI is the documentation. Discover via `baml describe`:**

```bash
brew install baml                # CLI binary: `baml`

baml init                        # new project (baml.toml + baml_src/)
baml help <command>              # CLI options and examples
baml describe baml.json          # reference for any module/type/method/signature/keyword
baml describe Array --budget 120 #   (Array, String, Map, assert, match, patterns, spawn, python, ...)
                                 #   ends with "N more lines"? re-run with `--budget <N>`
baml check                       # compile-check the project
baml run -e 'expr'               # eval an expression; fast feedback + syntax check
baml test --list && baml test    # run every test/testset block
baml fmt baml_src/main.baml      # canonicalize the project's formatting
```

`baml describe <name>` prints the **full source body** of stdlib functions. It's the fastest way to verify behavior, and the only path for embedded builtins like `assert` (no on-disk file). Pure functions need no test/client; check them with `baml run -e 'add(2, 3)'`. For anything not shown here (signatures, niche stdlib, advanced features), run `baml describe <name>`; never guess the stdlib.

**Workflow: sketch → `baml run -e` / `baml check` constantly → `baml describe` anything unfamiliar → `baml test` → `baml fmt` when a feature is done.**

- `baml pack` can create a binary.
- **Call BAML from Python / TS.** Declare a `[generator.<name>]` in `baml.toml`, run `baml generate`, then import the typed `baml_sdk`. Install + usage: `baml describe python` / `baml describe typescript` / `baml describe baml_sdk`. The selected toolchain must match the installed language bridge; use `baml toolchain use <channel-or-version>` to select it. Keep AI-related things and workflow logic in BAML as much as possible.

## Language essentials

The silent traps first. These compile and quietly do the wrong thing, or fail where TypeScript wouldn't:

- **Arithmetic is type-driven, not TS-style.** `int / int` is **truncating integer division** (`285 / 100 == 2`, NOT `2.85`) and `%` is the remainder (`285 % 100 == 85`); this compiles fine and just gives a quietly-wrong number, so it's the highest-value gotcha. Mix in a float to get float division (`285 / 100.0 == 2.85`, `285.0 / 100 == 2.85`); any mixed `int`/`float` op promotes to `float` (`5 + 2.0 == 7.0`). There is **no `.to_float()`**; convert an int with `n * 1.0` (or divide by a float). An `int` result does **not** auto-coerce to `float` on assignment (`let x: float = 285 / 100` is a compile error).
- **`+` is numeric-only.** String concat needs both sides already `string` (`"n=" + 5` won't compile). ``score=${n}`` stringifies any value (implicit `.to_string()`); call `.to_string()` for the string alone.
- **No ternary: `if/else` is the expression.** There's no `cond ? a : b`; `if (cond) { a } else { b }` *is* an expression that returns a value, so assign it directly: `let label = if (x > 3) { "big" } else { "small" };`. Each branch is a block whose last expression is its value (no `return`).
- **`T?` does NOT make an argument optional.** Unlike TS `b?: T`, a `b: T?` param is still *required* (you must pass `null`, else `expected N argument(s), got …`); add `= null` to make it omittable.
- **Args: defaults with `=`, keyword calls with `=` (never `:`).** Declare a default in the signature: `function f(a: int, b: int = 10)`; call `f(1)` or `f(1, b = 2)`. A **defaulted param must be passed by name**: `f(1, 2)` is an error (`defaulted parameter 'b' must be passed by name`). Any param (even required) may be passed by name (`f(a = 1, b = 2)`), and you can skip a middle default to set a later one (`f(1, c = 9)`). Keyword syntax is `name = value`; `name: value` won't parse (`:` is for types/fields). Built-ins follow this: `baml.http.fetch(url, timeout = baml.time.Duration.from_seconds(10))`.
- **Narrowing: locals only.** `v is T`, `v != null` / `v == null` and a truthy test narrow `v` only when `v` is a **local** (a `let` or a parameter) that **no closure captures**: in the guarded `if` / `while` branch, after an early exit (`if (!(v is T)) { return … }`), and on the right side of `&&` (left side true) and `||` (left side false), as in `v is T && v.f == 1`, `s == null || s.length() == 0`. NOT narrowed: a captured local, a **field path** (`h.v is T && h.v.f` → `E0007`) and an index path (`xs[0] is T`). Tasks can interleave between statements, so another task or an alias can change a field between the test and the read. For a field write `let v = h.v;` first, or `if let v: T = h.v { … }`, or `match (h.v) { let v: T => … }` (each reads the field one time). An assignment to the local replaces the fact with the assigned type. `baml describe is`.
- **Subscript panics on a missing index/key.** Use `.at(i)`/`.get(k)` (→ `T?`), reach through with `?.`, default with `??` (parenthesize: `(m.get(k) ?? 0) + 1`).
- **Classes are reference types.** `find`/`at(i)`/subscript return a *live alias*, not a copy, so mutating the result mutates that element inside the array (`xs.find(p)?.n += 1` updates `xs`), and a class passed to a function can be mutated by the callee.
- **Enums are plain variants: no methods, no associated data** (`E.A.foo()` won't compile). `enum E { A, B }`, access `E.A`. For variants that carry data, use a union of classes (`type Shape = Circle | Rect;`).

The rest of the language:

- **Blocks are expressions**: the last expression is the value (no `;`); `return x;` for early exit. A side-effect-only function returns `void` (another spelling of `null`, the unit type) and may omit its final expression or `return;`. A value left as the final expression of such a body is an error, not discarded: end it with `;` (`xs.push(x);`, since `push` returns the new length). `for (let x in xs)` iterates VALUES; `while (cond) { … }` loops; `while let PATTERN = expr { … }` loops until the pattern fails (e.g. draining a `T?`-returning `.pop()`).
- **Conditions use truthiness.** `if`, `while`, match guards, `&&`, `||`, and `!` accept any value. Falsy values are `false`, `null`, numeric zero, empty strings, empty arrays/maps, and empty bytes; everything else is truthy. `&&` and `||` still return `bool`, not an operand value; they short-circuit. A truthy optional local narrows in the taken branch and on the right side of `&&` (`s && s.length() > 0`).
- **Locals.** `let` bindings are reassignable (`x = x + 1`), with no `mut` keyword (it's TS `let`, not Rust); inside a body `const` currently means the same as `let`, so use `let` for locals. An empty `{}` or `[]` takes its type from later use (`m.set("a", 1)`); annotate it (`let m: map<string, int> = {};`) when nothing pins it (E0155).
- **Global variables = top-level `const`.** `const MAX_RETRIES = 3;` / `const TAGS: string[] = ["a"];` at file scope (a top-level `let` is an error). Evaluated **once at startup**, in dependency order (forward references are fine; a cycle is E0181). Visible bare in its namespace; elsewhere `root.<ns>.NAME`. The binding is **immutable** (`X = …` is E0179), but its contents are shared by every call: `X.field = v` / `X.push(…)` mutate one engine-wide value. Initializers can't do io (read config with `env.NAME` at request time), `spawn`/`await`, or `return`; an uncaught throw stops startup (warning E0183; `catch` it). Locals/params shadow globals.
- **Classes.** Fields are `name: type,`; construct with `Type { field: val }`. Methods take a bare `self`; a function in a class without `self`, such as `function new(...) -> Self`, is static and is called as `Type.new(...)`. **Fields are mutable** (like TS): `obj.field = v` and `obj.field += n` work, and a `self` method can mutate in place (a side-effect method returns `void`). Struct-update spread is supported: `User { ...u, tier: Tier.Free }`. **Empty classes are legal** (`class Marker {}`), handy as union variants.
- **Interfaces = shared behavior + dynamic dispatch.** `interface I { function m(self) -> T throws never; }`; each method declares its `throws` clause. A method with a body is a default implementation, which an implementor may override. A class opts in via `implements I { … }` inside the class; a value typed `I` (or `I[]`) dispatches to the implementor at runtime. Interfaces can also declare associated types and generic bounds. `baml describe interfaces`.
- **Operators are interfaces, like Rust's traits.** `<`, `>`, and sorting need `baml.ops.Compare` (`cmp` returns a `baml.ops.Ordering`); `+`, `-`, `*`, `/`, and `%` call `baml.ops.Add<Rhs>`, `Subtract`, `Multiply`, `Divide`, and `Remainder`, each with a `type Output`; `==` compares structurally (deep, across classes/arrays/maps) unless the type implements `baml.ops.Equals`. `${}` uses `baml.ToString`, and json uses `baml.ToJson` / `baml.FromJson`. Implement them on your own classes to get the operator. `baml describe baml.ops`.
- **`requires`, not inheritance.** BAML has no inheritance: a class can't extend a class. `interface Pet requires Animal, Named { … }` makes those interfaces prerequisites of `Pet`. `Pet`'s default bodies can call their methods, and a value typed `Pet` exposes them. Every `Pet` implementor also writes its own `implements Animal` and `implements Named` blocks (E0125 otherwise). A diamond of `requires` compiles, and the shared interface is implemented once; a cycle is an error. When two interfaces on one class declare the same method name, an unqualified call is an error (E0121); call `obj.as<I>.m()` instead.
- **Pattern matching.** `match (v) { … }` over values/types; arms are `pattern => expr`: literals, `let x: T` (bind + narrow), class destructure `T { f: let y }`, or-patterns `A | B`, guards `… if cond`, `_`; must be exhaustive. Also `v is T` → bool (narrows) and `if let x: T = v { … } else { … }`. `baml describe patterns`.
- **Closures** `(x) -> { ... }` infer param/return from context (annotate `(x: T) -> R` only when ambiguous; the `->` is required).
- **Stdlib methods are snake_case, called on a value.** Some return new, some mutate in place, a few do both (`sort_by_key` sorts the receiver *and* returns it). To read the docs, run `baml describe <word/type/identifier/keyword/etc>`.
- **Arrays have a JS-like method set**: `map`/`filter`/`filter_map`/`reduce`/`find`/`some`/`every`/`flat_map`/`slice`/`concat`/`join`/`includes`/length(), plus in-place `push`/`pop`/`shift`/`unshift`/`sort_by`/`sort_by_key`. Most take closures that can `throws`. `.map`/`.filter` return arrays directly (no `.collect()`). `baml describe Array` gives more info.
- **Strings.** Backtick strings interpolate `${...}`; the old `#" "#` syntax is removed. Escape with `` \` `` / `\${`; nest with extra backticks.
- **Namespaces =** `ns_*` **directories, no imports.** A folder `ns_<name>/` under `baml_src/` puts its files in namespace `<name>`; files in `baml_src/` itself are the `root` namespace (nesting stacks: `ns_a/ns_b/` → `root.a.b`; non-`ns_` folders don't namespace). Same namespace = same scope: files share definitions with no import. To reach *another* namespace, use the **absolute** path `root.<ns>.<name>` (for example, `root.llm.Response`). Run a target by its namespace-relative path (`baml run agent.main`), but `baml run -e` evaluates in the root scope, so reach in with the absolute form: `baml run -e 'root.agent.main()'`. Keep namespaces as flat as practical, like Go packages.
- **Logging.** BAML has `log.info(..)`, `log.debug(..)`, `log.warn(..)`, and `log.error(..)`.
- **Reflection.** BAML has full reflection: `reflect.Type.of<T>()`, function signatures, `reflect.call_any`, and classes and enums built at runtime. `reflect.Package.current().globals()` maps each global (clients included) to its value; `get_global<T>(name)` / `find_global<T>(name)` read one by name. Explore it with `baml describe reflect`.

## Errors

- **Raise** with `throw baml.errors.InvalidArgument { message: "…" }` or an instance of your own class (error types are ordinary classes; the builtins are `baml.errors.*`: `InvalidArgument`/`ParseError`/`Io`/`Timeout`/…; `baml describe baml.errors`). Annotate a fallible signature with `-> T throws ErrType`. Errors propagate implicitly; callers needn't re-declare.
- **`catch` for some, `catch_all` for all.** `expr catch (e) { baml.errors.ParseError => fallback }` handles a *specific* error and lets the rest propagate; `expr catch_all (e) { _ => fallback }` is *exhaustive*, for a workflow top / entrypoint.
- **Error context.** `catch (e, ctx)` binds a **`baml.errors.Context`** alongside the error. An error thrown while handling another chains onto it, so `ctx.root_cause()` / `ctx.cause` walk back to the original failure and `ctx.to_string()` renders the whole chain (Python `__context__`-style).
- Throw for failures; return a union of classes when every outcome is a normal result, like the states of a job. See the [Style guidelines](#style-guidelines).

## Concurrency and resources

- **Concurrency = green threads.** `spawn { … }` returns a `Future`; `await` collects it. Combine many with `baml.future.all` / `all_settled` / `race` / `any`. `all_settled` returns `Success<T>`, `Failure<E>`, or `Panicked` for each input; input cancellation is a `Panicked` outcome. Configure a spawn with a `with` clause: `spawn with limit, token { … }`. Each value listed is a `baml.spawn.Modifier` that transforms the plan the spawn builds, left to right. `baml.spawn.Limit.new(n)` caps concurrency (excess spawns queue), a `baml.spawn.CancelToken` cancels cooperatively, `baml.spawn.Root.new()` detaches the task from its spawner's cancellation. Use it to parallelize LLM and HTTP calls. `baml describe spawn` / `baml describe baml.future`.
- **Resource safety.** `defer { … }` runs a block at scope exit, LIFO, on *every* path (return / throw / fall-through), like Go. A class method named `function cleanup(self) -> void` is a **finalizer**: it runs at most once per instance whether you call it, `defer` it, or the GC reclaims it.

## LLM functions

- **LLM function = typed return.** The RETURN TYPE *is* the schema the model must produce (`class`, `enum`, literal union, `string[]`, `T?`). Structured output is just a typed value; hand it to ordinary code.
- **Prompts are backtick strings with `${...}` interpolation.** Write `prompt:` ``… ${arg} …``, and **always inject `${ctx.output_format()}`** for a structured return.
- **Clients are values, not config blocks.** `client Fast = openai.ResponsesClient.new(model = "…", api_key = env.OPENAI_API_KEY);`. The old `client<llm> Name { provider: …, options: {…} }` block is **removed**. Anything implementing `ai.Client` works (`openai.ResponsesClient`, `openai.ChatClient`, `anthropic.Client`, …; constructor parameters differ by provider). `api_key` and `base_url` accept `ai.Credential`: a literal string, `null` for the provider default, or a late-bound `env.NAME` reference resolved at request time. Compose reliability by **wrapping**: `ai.clients.Retry.new(inner = c, max_attempts = 3)` and `ai.clients.RoundRobin.new(members = […])` have `.new`, but `ai.clients.Fallback { members: […] }` does **not**; construct it as a class literal. Then use `client: Fast` in the function, or the shorthand `client: "openai/gpt-6-luna"`. `baml describe openai` / `baml describe ai.Credential` / `baml describe ai.clients`.
- **Shape the schema with field attributes.** `@description("…")` adds a `///` hint the model sees in `${ctx.output_format()}`; `@alias("name")` renames the emitted JSON key. Chain: `tags: string[] @alias("labels") @description("…")`. On output types, `///` comments, `@description`, and enum names all render into `ctx.output_format()`, so editing them edits the prompt; put notes in `//` comments.

### Running AI functions

`F(args)` is shorthand for `ai.Agent.new(client = …).run(F@spec(args)).value`. `F@spec(args)` is an unrun, immutable `ai.FunctionSpec<Out>`:
- `.call(client = c)` runs it.
- `.build_request(client = c)` returns the provider HTTP request with no I/O; use it to test prompts and clients.
- `.parse(reply)` parses a saved reply.
- `.prompt()` and `.output_type()` inspect it.

`ai.Agent.new(max_steps = 12, schema_attempts = 2, client = null, tool_errors = ai.tools.ErrorMode.Report, on_event = null)` is the default loop. It runs the model's tool calls in parallel, and re-asks with a correction when a reply won't parse (that retry doesn't count as a step). It returns `ai.RunResult { value, journal, usage }`.

`on_event` (on `Agent.new` or `.call`) receives each `ai.events.Event` as it happens:
- `RunStarted`
- `UserMessage` and `AssistantMessage`
- `ToolRequested`, `ToolCompleted`, and `ToolFailed`
- `ModelCall`, with the model's usage, its timing, and the raw HTTP request and response
- `FinalProduced`

Use it for logs, progress, and cost. A listener that throws can't fail the run (`ai.events.guard`).

To add cross-cutting behavior (metering, caching, replay, budgets), write a class that `implements ai.Runner { type Error = …; function run<Out>(self, spec: ai.FunctionSpec<Out>) -> ai.RunResult<Out> throws Self.Error }`, usually wrapping an inner `ai.Agent`. To fake or wrap a model, write one that `implements ai.Client { requested_model, provider, render, invoke }`. Pass either wherever a client or runner is taken, and keep AI functions free of that plumbing.

### Debugging a recorded call

Runs record their calls, and AI functions record their inputs, output, error and type arguments by default. `baml query` answers SQL over them (`baml query --schema` lists the tables). To turn a failing call into a test:

```bash
# 1. Find the call.
baml query "SELECT span_id, input_args, error_value FROM spans
  WHERE span_name = 'user.Extract' AND error_value IS NOT NULL ORDER BY start_time DESC LIMIT 1"
# 2. Its types as BAML code, classes and enums built at run time included.
#    baml_code also takes an instance or enum value: input_args['row'].
baml query "SELECT baml_code(type_args['T']) FROM spans WHERE span_id = '<span_id>'"
# 3. The model's raw HTTP response, from the request under the call.
baml query "SELECT network_event_values FROM spans
  WHERE span_type = 'network_span' AND parent_span_id = '<span_id>' ORDER BY start_time"
```

`baml_code` returns a block whose value is the recorded type: bind it with `let type`. Replay the model's reply with `F@parse`, so the test never calls the model:

```baml
class Row {
    name: string,
    qty: int,
}

function Extract<T>(raw: string) -> T {
    client: "openai/gpt-6-luna"
    prompt: `Extract. ${ctx.output_format()}\n${raw}`
}

test "Extract parses the recorded reply" {
    // Step 2's output: a block whose value is the recorded type.
    let t = {
        reflect.Type.of<Row>()
    };
    let type T = t;
    // Step 3's message text. `F@parse` replays it, so the test never calls the model.
    let row = Extract@parse<T>(`{"name": "widget", "qty": 3}`);
    assert.equal(row, Row { name: "widget", qty: 3 })
}
```

## Tests

- **Shape:** lone `test "name" { ... }` (no wrapper); `testset` only GROUPS. Asserts (only 6): `assert.equal`/`approx_equal`/`is_true`/`is_type`/`not_null`/`contains`. `assert.equal` compares **structurally** (deep, across classes/arrays/maps), and so does plain `==`, which is the bool form. `assert.equal` is *exact* on floats; use `assert.approx_equal(actual, expected, eps)` for computed ones. Last assert: no trailing `;`.
- **Running:** canonical IDs are `root::TestName` for a top-level test and `root::Testset::TestName` inside a testset. Run one with `baml test -i "root::TestName"` (`-x` to exclude); `baml test --list` prints valid selectors.
- **Test the pure code, not the model.** Unit-test orchestration/post-processing on literal data with `assert.*`. Calling an LLM function in a `test` with a live client makes a real request. To test prompts, requests, and parsing offline, use `F@spec(args)` or a fake client that implements `ai.Client` (see [Running AI functions](#running-ai-functions)).

## Examples

### Example 1: LLM DSL + glue (schema, attributes, client, backtick prompt, post-processing)

```baml
// The return type IS the schema; @description/@alias shape what the model sees.
enum Priority {
    High,
    Low,
}

class LineItem {
    name: string,
    amount: float,
    priority: Priority,
}

class Invoice {
    vendor: string @alias("seller"),
    status: "draft" | "final" @description("invoice state"),
    items: LineItem[],
    note: string?,
}

// Clients are ordinary values implementing `ai.Client` (no `client<llm> { }` block).
// `env.OPENAI_API_KEY` is a late-bound reference resolved when a request runs.
client Fast = openai.ResponsesClient.new(model = "gpt-6-luna", api_key = env.OPENAI_API_KEY);

// Compose reliability by wrapping a client. `Retry`/`RoundRobin` have `.new(...)`;
// `Fallback` has no `.new`, so construct it as a class literal.
client Reliable = ai.clients.Retry.new(inner = Fast, max_attempts = 3);

client Safe = ai.clients.Fallback { members: [Reliable, anthropic.Client.new(model = "claude-sonnet-5-5")] };

function Extract(raw: string) -> Invoice {
    client: Reliable // or shorthand: "openai/gpt-6-luna"
    prompt: `Extract the invoice. ${ctx.output_format()}\n${raw}`
}

// Structured output is just a typed value - hand it to ordinary code.
// Closure params/return infer from context; only the -> is required.
// `min_amount` has a default - omit it, or pass it by name (`min_amount = 10.0`).
function high_total(inv: Invoice, min_amount: float = 0.0) -> float {
    inv.items
        .filter((i) -> {
            i.priority == Priority.High && i.amount >= min_amount
        })
        .reduce(
            (a, i) -> {
                a + i.amount
            },
            0.0,
        )
}

test "post-process a literal Invoice - no model call" {
    let inv = Invoice {
        vendor: "Acme",
        status: "final",
        note: null,
        items: [
            LineItem { name: "srv", amount: 900.0, priority: Priority.High },
            LineItem { name: "mug", amount: 12.0, priority: Priority.Low },
        ],
    };
    assert.equal(high_total(inv), 900.0); // default min_amount = 0.0
    assert.equal(high_total(inv, min_amount = 1000.0), 0.0); // keyword arg
    // `Extract@spec(...)` binds the call without running it; `.parse` reads a saved reply.
    let parsed = Extract@spec("raw").parse(`{"seller":"Acme","status":"draft","items":[],"note":null}`);
    assert.equal(parsed.vendor, "Acme")
}
```

### Example 2: the language (methods, static constructors, interpolation, closures, maps, json, errors)

```baml
// BAML is a real language - no LLM here.
enum Tier {
    Free,
    Pro,
}

class User {
    name: string,
    tier: Tier,
    score: int,

    // static constructor (no self): called as `User.new(...)`
    function new(name: string, score: int) -> Self {
        User { name: name, tier: Tier.Pro, score: score }
    }

    // method (bare self) + ${} interpolation (implicit .to_string() on the int)
    function label(self) -> string {
        `${self.name.to_upper_case()}:${self.score}`
    }

    // fields are MUTABLE like TS: assign / += on self in place; a side-effect method returns void
    function celebrate(self) -> void {
        self.score += 100
    }
}

// inferred closures; sort_by_key; optional chaining + ?? over a possibly-null .at
function top_label(us: User[]) -> string {
    us
        .sort_by_key((u) -> {
            0 - u.score
        })
        .at(0)
        ?.label()
        ?? "none"
}

// map<string,int>: .get ?? default; explicit .to_string()
function tier_counts(us: User[]) -> map<string, int> {
    let counts: map<string, int> = {};
    for (let u in us) {
        counts.set(u.tier.to_string(), (counts.get(u.tier.to_string()) ?? 0) + 1);
    }
    counts
}

// baml.json: decode<T>(text) and to<T>(j) read, stringify(v) and from(v) -> json write.
// Options by name: naming = baml.json.Naming.Aliased (use @alias names), indent = 2 or "\t",
// omit_nulls = true (leave out class fields holding null).
function roundtrip(u: User) -> User {
    baml.json.decode<User>(baml.json.stringify(u))
}

// `catch` with a typed arm handles ONE specific error and lets the rest propagate:
// a bad entry is skipped instead of failing the whole batch.
function parse_scores(texts: string[]) -> int[] {
    texts.filter_map((t) -> {
        baml.Int.parse(t) catch (e) {
            baml.errors.ParseError => null
        }
    })
}

function truthy_label(s: string?) -> string {
    if (s) {
        "set"
    } else {
        "empty"
    }
}

test "lang" {
    let us = [User.new("ada", 90), User.new("bo", 30)];
    log.info(us);
    assert.equal(top_label(us), "ADA:90");
    assert.equal(tier_counts(us).get("Pro") ?? 0, 2);
    let kit = User.new("kit", 5);
    kit.celebrate(); // mutate in place
    kit.tier = Tier.Free; // direct field assignment
    assert.equal(kit.score, 105);
    let upgraded = User { ...kit, tier: Tier.Pro };
    assert.is_type<User>(upgraded);
    assert.equal(upgraded.tier, Tier.Pro);
    assert.equal(285 / 100, 2);
    assert.equal(roundtrip(User.new("zoe", 7)).name, "zoe");
    assert.equal(parse_scores(["42", "x", "7"]), [42, 7]);
    assert.equal(truthy_label("value"), "set");
    assert.equal(truthy_label(""), "empty");
    assert.equal(truthy_label(null), "empty")
}
```

### Example 3: interfaces (shared behavior, default method, dynamic dispatch, `requires`)

```baml
interface Animal {
    function sound(self) -> string throws never;
    // default method
    function describe(self) -> string throws never {
        `${self.sound()}!`
    }
}

// Every Pet is an Animal, so Pet's defaults can call Animal's methods.
interface Pet requires Animal {
    function pet_name(self) -> string throws never;
    function greet(self) -> string throws never {
        `${self.pet_name()}: ${self.describe()}`
    }
}

class Dog {
    name: string,
    implements Animal {
        function sound(self) -> string {
            "woof"
        }
    }
    // needs `implements Animal` too
    implements Pet {
        function pet_name(self) -> string {
            self.name
        }
    }
}

class Cat {
    indoor: bool,
    implements Animal {
        function sound(self) -> string {
            "meow"
        }
        // override
        function describe(self) -> string {
            `quiet ${self.sound()}`
        }
    }
}

// an Animal[] holds any implementor; calls dispatch dynamically
function chorus(animals: Animal[]) -> string {
    animals
        .map((a) -> {
            a.describe()
        })
        .join(" ")
}

test "interfaces" {
    let animals: Animal[] = [Dog { name: "Rex" }, Cat { indoor: true }];
    assert.equal(chorus(animals), "woof! quiet meow");
    let pet: Pet = Dog { name: "Rex" };
    assert.equal(pet.greet(), "Rex: woof!");
    assert.equal(pet.sound(), "woof") // Animal's methods, through Pet
}
```

### Example 4: pattern matching (`match` over values + types, `is`, `if let`)

```baml
// One class per state; a union of them is matched exhaustively.
class Queued {
}

class Retrying {
    attempt: int,
}

class Done {
    result: string,
}

type Job = Queued | Retrying | Done;

const MAX_ATTEMPTS = 3;

function status(job: Job) -> string {
    match (job) {
        Queued => "queued", // class pattern, no binding
        Done { result: "" } => "done, empty", // literal field
        let d: Done => `done: ${d.result}`, // typed binding (matches + narrows)
        Retrying { attempt: let n } if n >= MAX_ATTEMPTS => "giving up", // destructure + guard
        Retrying { attempt: let n } => `retry ${n}`,
    }
}

function classify(n: int) -> string {
    match (n) {
        0 => "zero",
        1 | 2 | 3 => "small", // or-pattern
        let x if x < 0 => "neg", // binding + guard
        _ => "big", // wildcard
    }
}

// `is` -> bool (and narrows)
function finished(jobs: Job[]) -> int {
    jobs
        .filter((j) -> {
            j is Done
        })
        .length()
}

// `if let PATTERN = expr { } else { }` binds and narrows in one step
function result_or_empty(job: Job) -> string {
    if let d: Done = job {
        d.result
    } else {
        ""
    }
}

test "patterns" {
    assert.equal(status(Queued {}), "queued");
    assert.equal(status(Done { result: "" }), "done, empty");
    assert.equal(status(Done { result: "ok" }), "done: ok");
    assert.equal(status(Retrying { attempt: 3 }), "giving up");
    assert.equal(status(Retrying { attempt: 1 }), "retry 1");
    assert.equal(classify(2), "small");
    assert.equal(classify(-5), "neg");
    assert.equal(finished([Queued {}, Done { result: "a" }, Done { result: "b" }]), 2);
    assert.equal(result_or_empty(Done { result: "ok" }), "ok");
    assert.equal(result_or_empty(Queued {}), "")
}
```

### Example 5: resource safety + structured concurrency (defer, cleanup, error context, spawn options, futures, while-let)

```baml
class DbConn {
    log: string[],
    // `cleanup` is a magic method (recognized by name): runs at most once per
    // instance - whether called explicitly, deferred, or reclaimed by the GC.
    function cleanup(self) -> void {
        self.log.push("closed");
    }
}

function use_conn() -> string[] {
    let c = DbConn { log: [] };
    {
        defer { c.cleanup() } // deferred blocks run LIFO at scope exit,
        defer { c.log.push("commit") } // on every path (return / throw / fall-through)
        c.log.push("query")
    };
    c.log // ["query", "commit", "closed"]
}

function fail_a() -> string {
    throw baml.errors.Io { message: "disk full" }
}

function fail_b() -> string {
    throw baml.errors.InvalidArgument { message: "bad retry config" }
}

// `catch (e, ctx)` binds the error AND its baml.errors.Context; throwing while handling
// chains the new error onto the one being handled, so root_cause() walks to the origin.
function root_cause_demo() -> string {
    fail_a() catch (e, ctx) {
        baml.errors.Io => {
            fail_b() catch (e2, ctx2) {
                baml.errors.InvalidArgument => {
                    match (ctx2.root_cause().error) { // ctx.to_string() renders the full chain
                        let io: baml.errors.Io => io.message, // "disk full" - the original cause
                        _ => "unknown",
                    }
                }
            }
        }
    }
}

// spawn returns a Future; baml.future.all/all_settled/race/any combine many.
function concurrent_squares(xs: int[]) -> int {
    // all run concurrently
    let futures = xs.map((x) -> {
        spawn { x * x }
    });
    let squares = await baml.future.all(futures);
    squares.reduce(
        (a, b) -> {
            a + b
        },
        0,
    )
}

// The same shape parallelizes HTTP and LLM calls: results come back in order.
function fetch_all(urls: string[]) -> string[] {
    await baml.future.all(urls.map((u) -> { spawn { baml.http.fetch(u).text() } }))
}

// Each value after `with` is a `baml.spawn.Modifier`: a Limit caps concurrency (excess
// spawns queue), a CancelToken cancels cooperatively, a Root detaches from the spawner.
function rate_limited() -> int {
    let g = baml.spawn.Limit.new(2);
    let a = spawn with g { 1 };
    let b = spawn with g { 2 };
    (await a) + (await b)
}

// while-let drains an optional-returning source; the loop exits when the pattern fails.
function drain(stack: string[]) -> string {
    let out: string[] = [];
    while let item: string = stack.pop() {
        out.push(item);
    }
    out.join("")
}

test "resources + concurrency" {
    assert.equal(use_conn(), ["query", "commit", "closed"]);
    assert.equal(root_cause_demo(), "disk full");
    assert.equal(concurrent_squares([1, 2, 3]), 14);
    assert.equal(rate_limited(), 3);
    assert.equal(drain(["a", "b", "c"]), "cba")
}
```

## Style guidelines

ALWAYS follow these guidelines. Each entry shows a bad form and a good form. **Gap** marks a workaround for something BAML doesn't have yet.

Entries 12, 14, 17, 18, and 22 add structure. Apply them where the structure removes duplication or encodes an invariant the program relies on, not to one-off code.

### 1. Name repeated values

Repeated literals drift apart, and readers can't tell which ones are the same.

Bad:

```baml
function page(xs: string[]) -> string[] {
    xs.slice(0, 25)
}

function is_full(xs: string[]) -> bool {
    xs.length() >= 25
}
```

Good:

```baml
const PAGE_SIZE = 25;

function page(xs: string[]) -> string[] {
    xs.slice(0, PAGE_SIZE)
}

function is_full(xs: string[]) -> bool {
    xs.length() >= PAGE_SIZE
}
```

### 2. Throw instead of returning error fields

Callers can ignore an `error` field. They can't ignore `throws`.

Bad:

```baml
class Result {
    value: string?,
    error: string?,
}

function lookup(domain: string) -> Result {
    if (!domain.includes(".")) {
        return Result { value: null, error: "bad domain" };
    }
    Result { value: domain, error: null }
}
```

Good:

```baml
function lookup(domain: string) -> string throws baml.errors.InvalidArgument {
    if (!domain.includes(".")) {
        throw baml.errors.InvalidArgument { message: "bad domain" }
    }
    domain
}
```

### 3. Prefer a functional style, especially on containers

Transform arrays with `filter`, `map`, and friends instead of looping and mutating
an accumulator. Each step says what it does, and there's no state to track.

Bad:

```baml
class User {
    name: string,
    active: bool,
}

function active_names(users: User[]) -> string[] {
    let out: string[] = [];
    for (let u in users) {
        if (u.active) {
            out.push(u.name);
        }
    }
    out
}
```

Good:

```baml
class User {
    name: string,
    active: bool,
}

function active_names(users: User[]) -> string[] {
    users
        .filter((u) -> {
            u.active
        })
        .map((u) -> {
            u.name
        })
}
```

### 4. Use `match` instead of `if` chains

`match` is exhaustive, so a new variant is a compile error instead of a silent fallthrough.

Bad:

```baml
enum Color {
    Red,
    Green,
}

function hex(c: Color) -> string {
    if (c == Color.Red) {
        "#f00"
    } else {
        "#0f0"
    }
}
```

Good:

```baml
enum Color {
    Red,
    Green,
}

function hex(c: Color) -> string {
    match (c) {
        Color.Red => "#f00",
        Color.Green => "#0f0",
    }
}
```

### 5. Use truthiness

`null`, `false`, `0`, and empty strings and collections are falsy.

Bad:

```baml
function greet(name: string?) -> string {
    if (name != null && name.length() > 0) {
        return `hi ${name}`;
    }
    "hi"
}
```

Good:

```baml
function greet(name: string?) -> string {
    if (name) {
        return `hi ${name}`;
    }
    "hi"
}
```

### 6. Use template strings instead of `+`

Bad:

```baml
function greet(name: string) -> string {
    "hi " + name + "!"
}
```

Good:

```baml
function greet(name: string) -> string {
    `hi ${name}!`
}
```

### 7. Prefer the standard library to handwritten code

The stdlib is tested, and readers already know what it does. A local helper has to be read
to be trusted.

Bad:

```baml
function has_prefix(s: string, prefix: string) -> bool {
    s.length() >= prefix.length() && s.slice(0, prefix.length()) == prefix
}

function is_secure(url: string) -> bool {
    has_prefix(url, "https://")
}
```

Good:

```baml
function is_secure(url: string) -> bool {
    url.starts_with("https://")
}
```

### 8. Say what a regex matches

At minimum, a comment with what it matches and an example. For long regexes, verbose mode
(`(?x)`) lets you split it up and comment each part.

Bad:

```baml
function is_ticket_id(text: string) -> bool {
    baml.regex.new(`^[A-Z]{2,5}-\\d+$`).has_match(text)
}
```

Good:

```baml
function is_ticket_id(text: string) -> bool {
    // A ticket ID like "BAML-1024": 2 to 5 capital letters, a dash, then digits.
    baml.regex.new(`^[A-Z]{2,5}-\\d+$`).has_match(text)
}
```

### 9. Use a class instead of a joined string

Bad:

```baml
function contact_key(name: string, email: string) -> string {
    `${name}|${email}`
}
```

Good:

```baml
class ContactKey {
    name: string,
    email: string,
}
```

### 10. Use a closed type for fixed sets of values

`string` accepts typos. Use an enum or a literal union.

Bad:

```baml
class Ticket {
    status: string,
}

function is_open(t: Ticket) -> bool {
    t.status == "open"
}
```

Good:

```baml
enum Status {
    Open,
    Closed,
}

class Ticket {
    status: Status,
}

function is_open(t: Ticket) -> bool {
    t.status == Status.Open
}
```

### 11. Use literal unions for raw values, enums for concepts

Use a literal union when the strings themselves are the data: wire formats, protocol values,
or a small option on one function. Use an enum for a concept that shows up across the
codebase, or that needs methods or interfaces. Both work with `match`.

Bad:

```baml
enum Method {
    Get,
    Post,
}

function request_line(method: Method, path: string) -> string {
    let verb = match (method) {
        Method.Get => "GET",
        Method.Post => "POST",
    };
    `${verb} ${path}`
}
```

Good:

```baml
function request_line(method: "GET" | "POST", path: string) -> string {
    `${method} ${path}`
}
```

### 12. Throw error classes with data, not strings

Callers can match on a class and read its fields. Put the data that explains the failure in
fields, and build the message in `to_string` instead of storing it.

Bad:

```baml
function load(users: map<string, string>, id: string) -> string {
    users.get(id) ?? throw `no user with id ${id}`
}
```

Good:

```baml
class UserNotFound {
    id: string,

    implements baml.ToString {
        function to_string(self) -> string {
            `no user with id ${self.id}`
        }
    }
}

function load(users: map<string, string>, id: string) -> string throws UserNotFound {
    users.get(id) ?? throw UserNotFound { id: id }
}
```

### 13. Use time methods instead of nanoseconds

Bad:

```baml
function elapsed_ms(start: baml.time.Instant) -> bigint {
    (baml.time.Instant.now().to_timestamp_nanoseconds() - start.to_timestamp_nanoseconds())
        / 1000000n
}
```

Good:

```baml
function elapsed_ms(start: baml.time.Instant) -> bigint {
    start.elapsed().to_milliseconds()
}
```

### 14. Validate in a constructor

Check once, then pass the validated type around.

Bad:

```baml
function record(score: float) -> string {
    if (score < 0.0 || score > 1.0) {
        return "invalid";
    }
    `recorded ${score}`
}
```

Good:

```baml
class Score {
    value: float,

    function new(value: float) -> Self throws baml.errors.InvalidArgument {
        if (value < 0.0 || value > 1.0) {
            throw baml.errors.InvalidArgument { message: "score out of range" }
        }
        Score { value: value }
    }
}

function record(score: Score) -> string {
    `recorded ${score.value}`
}
```

### 15. Throw instead of returning sentinel values

`-1`, `""`, and `null` meaning "failed" look like real values, so callers forget to check.

Bad:

```baml
function parse_port(text: string) -> int {
    int.parse(text) catch (e) {
        _ => -1,
    }
}
```

Good:

```baml
function parse_port(text: string) -> int throws baml.errors.ParseError {
    int.parse(text)
}
```

### 16. Catch where you can recover

Don't let one bad item fail the whole batch.

Bad:

```baml
class NotFound {
    id: string,
}

function lookup(id: string) -> string throws NotFound {
    if (!id) {
        throw NotFound { id: id }
    }
    id
}

function lookup_all(ids: string[]) -> string[] throws NotFound {
    ids.map((id) -> {
        lookup(id)
    })
}
```

Good:

```baml
class NotFound {
    id: string,
}

function lookup(id: string) -> string throws NotFound {
    if (!id) {
        throw NotFound { id: id }
    }
    id
}

function lookup_all(ids: string[]) -> string[] {
    ids.filter_map((id) -> {
        lookup(id) catch (e) {
            NotFound => null
        }
    })
}
```

### 17. Put functions in classes

Most functions belong to a class: the one whose data they work on. Keep free functions for
small utilities shared across classes. Functions that take the same arguments are a sign
of a missing class.

Bad:

```baml
function save(dir: string, name: string, body: string) -> void throws baml.errors.Io {
    baml.fs.write(`${dir}/${name}`, body);
}

function load(dir: string, name: string) -> string throws baml.errors.Io | baml.errors.ParseError {
    baml.fs.read(`${dir}/${name}`)
}
```

Good:

```baml
class Store {
    dir: string,

    function path(self, name: string) -> string {
        `${self.dir}/${name}`
    }

    function save(self, name: string, body: string) -> void throws baml.errors.Io {
        baml.fs.write(self.path(name), body);
    }

    function load(self, name: string) -> string throws baml.errors.Io | baml.errors.ParseError {
        baml.fs.read(self.path(name))
    }
}
```

### 18. Implement builtin interfaces instead of ad-hoc methods

Implement `ToString`, `Compare`, `ToJson`, `FromJson`, and the others instead of inventing
method names. Then `<`, `${}` interpolation, and `sort` work on your type like they do on
builtins. Write the `implements` block inside the class, next to the data it uses.

Bad:

```baml
class Version {
    major: int,
    minor: int,

    function label(self) -> string {
        `${self.major}.${self.minor}`
    }

    function older_than(self, other: Version) -> bool {
        self.major < other.major || (self.major == other.major && self.minor < other.minor)
    }
}
```

Good:

```baml
class Version {
    major: int,
    minor: int,

    implements baml.ToString {
        function to_string(self) -> string {
            `${self.major}.${self.minor}`
        }
    }

    implements baml.ops.Compare {
        function cmp(self, other: Self) -> baml.ops.Ordering {
            if (self.major != other.major) {
                self.major.cmp(other.major)
            } else {
                self.minor.cmp(other.minor)
            }
        }
    }
}
```

### 19. Pin `throws` on your public API

The errors a function throws are part of its contract, like its return type. If `throws` is
inferred, a change deep in the call tree can add a new error that reaches your users with no
change to the signature. Pin it on functions other packages or the generated SDK call, so
that change is a compile error instead. Leave it inferred on internal helpers.

Bad:

```baml
function parse_age(text: string) -> int {
    int.parse(text)
}
```

Good:

```baml
function parse_age(text: string) -> int throws baml.errors.ParseError {
    int.parse(text)
}
```

### 20. Model states as a union, not optional fields

Optional fields that are "only set when..." allow combinations that make no sense, like a
job with both a result and an error. One class per state makes them unrepresentable.

Bad:

```baml
class Job {
    result: string?,
    error: string?,
}

function describe(job: Job) -> string {
    let result = job.result;
    let error = job.error;
    if (error) {
        `failed: ${error}`
    } else if (result) {
        `done: ${result}`
    } else {
        "running"
    }
}
```

Good:

```baml
class Running {
}

class Done {
    result: string,
}

class Failed {
    error: string,
}

type Job = Running | Done | Failed;

function describe(job: Job) -> string {
    match (job) {
        Running => "running",
        let d: Done => `done: ${d.result}`,
        let f: Failed => `failed: ${f.error}`,
    }
}
```

### 21. Use one generic function instead of copies that differ by output type

Bad:

```baml
client Fast = openai.ResponsesClient.new(model = "gpt-5.6-luna");

class Summary {
    text: string,
}

class Outline {
    points: string[],
}

function ExtractSummary(notes: string) -> Summary {
    client: Fast
    prompt: `Read these notes: ${notes}`
}

function ExtractOutline(notes: string) -> Outline {
    client: Fast
    prompt: `Read these notes: ${notes}`
}
```

Good:

```baml
client Fast = openai.ResponsesClient.new(model = "gpt-5.6-luna");

function Extract<Output>(notes: string) -> Output {
    client: Fast
    prompt: `Read these notes: ${notes}`
}
```

### 22. Put per-type behavior in an interface, not a `match`

When several classes each compute something their own way, give each class a method through
a shared interface. Each class's logic stays with the class, and a new class is one new
implementation instead of a new arm in every `match`.

Bad:

```baml
class Square {
    side: float,
}

class Rect {
    width: float,
    height: float,
}

function area(shape: Square | Rect) -> float {
    match (shape) {
        let s: Square => s.side * s.side,
        let r: Rect => r.width * r.height,
    }
}
```

Good:

```baml
interface Shape {
    function area(self) -> float throws never;
}

class Square {
    side: float,

    implements Shape {
        function area(self) -> float {
            self.side * self.side
        }
    }
}

class Rect {
    width: float,
    height: float,

    implements Shape {
        function area(self) -> float {
            self.width * self.height
        }
    }
}
```

### 23. Exit early instead of nesting

`?? throw` and `?? return` handle the missing case on one line, and the rest of the function
stays flat.

Bad:

```baml
class UserNotFound {
    id: string,
}

class BadEmail {
    email: string,
}

function email_domain(
    emails: map<string, string>,
    id: string,
) -> string throws UserNotFound | BadEmail {
    let email = emails.get(id);
    if (email) {
        let domain = email.split("@").at(1);
        if (domain) {
            domain
        } else {
            throw BadEmail { email: email }
        }
    } else {
        throw UserNotFound { id: id }
    }
}
```

Good:

```baml
class UserNotFound {
    id: string,
}

class BadEmail {
    email: string,
}

function email_domain(
    emails: map<string, string>,
    id: string,
) -> string throws UserNotFound | BadEmail {
    let email = emails.get(id) ?? throw UserNotFound { id: id };
    email.split("@").at(1) ?? throw BadEmail { email: email }
}
```

### 24. Catch the errors you can handle, not everything

`_` also catches failures you didn't plan for. Name the errors you can recover from and let
the rest propagate.

Bad:

```baml
const DEFAULT_SETTINGS = "{}";

function load_settings(path: string) -> string {
    baml.fs.read(path) catch (e) {
        _ => DEFAULT_SETTINGS,
    }
}
```

Good:

```baml
const DEFAULT_SETTINGS = "{}";

function load_settings(path: string) -> string throws baml.errors.ParseError {
    baml.fs.read(path) catch (e) {
        baml.errors.Io => DEFAULT_SETTINGS,
    }
}
```

### 25. End with the result; use `return` only to exit early

Bad:

```baml
function label(n: int) -> string {
    if (n < 0) {
        return "negative";
    }
    return `${n}`;
}
```

Good:

```baml
function label(n: int) -> string {
    if (n < 0) {
        return "negative";
    }
    `${n}`
}
```

### 26. Write multi-line strings as indented backtick literals

The compiler strips the common indentation and the leading and trailing blank lines, so the
string reads the way it prints.

Bad:

```baml
function usage() -> string {
    "usage: tool [options]\n  -h  show help\n  -v  verbose"
}
```

Good:

```baml
function usage() -> string {
    `
        usage: tool [options]
          -h  show help
          -v  verbose
    `
}
```

### 27. Throw on invalid input instead of substituting a default

A placeholder like `"untitled"` hides the bug and spreads bad data.

Bad:

```baml
class Tag {
    name: string,

    function new(name: string) -> Self {
        if (!name) {
            return Tag { name: "untitled" };
        }
        Tag { name: name }
    }
}
```

Good:

```baml
class Tag {
    name: string,

    function new(name: string) -> Self throws baml.errors.InvalidArgument {
        if (!name) {
            throw baml.errors.InvalidArgument { message: "tag name is empty" }
        }
        Tag { name: name }
    }
}
```

### Project layout and conventions

1. For a multi-step LLM workflow, you can mirror its steps in the tree: numbered folders (`1_extract/`, `2_classify/`) and a top function that only calls the steps in order.
2. Group related files into directories once a folder gets crowded (past about 8 files); folders without an `ns_` prefix don't create namespaces, so nest freely.
3. AI functions and their output classes go in `<name>_prompt.baml` with no logic; the code around them in `<name>.baml`; tests in `<name>_test.baml`, or at the bottom of the source file, as in Rust.
4. Entry points take `client: ai.Client? = null` and pass it to every call; tests run offline on saved replies (`F@spec(args).parse(reply)`) or a fake client.
5. Mark the main steps of a workflow with `//#` comments; the BAML visualizer shows them. Annotate the flow, not every line:

```baml
function hello() -> string {
    //# Start loading data
    let greeting = "hello";
    //# Return the result
    greeting
}
```
