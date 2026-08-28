# BAML Language Testing Guide

## Where does a new test go?

Answer one question first: **what is the test actually about?**

| The subject of the test | Where it goes |
|---|---|
| BAML *behavior* — run code, assert a value or a catchable throw | `crates/baml_tests/baml_src/ns_<topic>/` as a `test` block |
| A compile **error** (or intentionally broken syntax) | `crates/baml_tests/projects/diagnostic_errors/` or `projects/broken_syntax/` |
| Compiler IR — PPIR/MIR/bytecode/diagnostics of code that compiles | `crates/baml_tests/baml_src/ns_fixtures/ns_<topic>/` (snapshots are generated automatically) |
| Something only Rust can see — VM/heap/GC state, host arg marshalling, wall-clock timing, salsa invalidation, CLI/LSP/FFI surface | a Rust test in the owning crate |

**The default is a BAML `test` block.** If you find yourself writing a Rust test
that compiles a BAML string, runs a function, and asserts the returned value,
stop — that test belongs in `baml_src/`. The whole corpus compiles *once* for
the entire suite, while each Rust test pays its own full compile (stdlib
included). That difference is why the old per-project snapshot tier cost ~23 CPU
minutes and its replacement costs well under one.

Writing one is just:

```baml
// crates/baml_tests/baml_src/ns_<topic>/<topic>.baml
function add(a: int, b: int) -> int { a + b }

test "adds two ints" {
  assert.equal(add(2, 3), 5)
}
```

Run it with `target/debug/baml-cli test --from crates/baml_tests/baml_src`
(add `-i "<name>"` to select one). See `crates/baml_tests/README.md` for the
corpus layout and the "END-TO-END TESTING" section below for the CLI workflow
and the language traps worth knowing.

A Rust test is the right call when BAML genuinely cannot observe the thing —
and when that is so, say why in a comment, the way
`crates/bex_vm/tests/bigint_equality.rs` does (equal literals may share a
constant-pool entry, so BAML source cannot force two distinct allocations).

## Rust tests: import the prefixed compile helpers

A fresh `ProjectDatabase` re-derives the whole stdlib — about nine CPU-seconds,
against a few milliseconds for the snippet under test. `cargo nextest` runs each
test in its own process, so no in-process cache can amortize that; the stdlib
slice is compiled **once at build time** by `baml_tests`' build script
instead.

So in a Rust test, import the compile helpers from `baml_tests::stdlib_prefix`,
not from `baml_project::testing`:

```rust
// slow — re-derives the stdlib on every test
use baml_project::{collect_diagnostics, testing::setup_test_db};

// fast — splices in the build-time stdlib slice
use baml_tests::stdlib_prefix::{check_user_files, setup_test_db};
```

| Need | Use |
|---|---|
| compile a snippet to bytecode | `baml_tests::stdlib_prefix::compile_source{,_with_opt}` |
| several files in one project | `baml_tests::stdlib_prefix::compile_multi_file` / `setup_multi_file_db` |
| a database to collect diagnostics from | `baml_tests::stdlib_prefix::setup_test_db` |
| the diagnostics themselves | `baml_tests::stdlib_prefix::check_user_files` (**not** `collect_diagnostics`) |

`check_user_files` checks only the project's own files plus the package-level
pass, instead of re-checking all ~50 stdlib files whose diagnostics every caller
then filters out anyway. That narrowing is sound here because a test database is
written once and read once, so nothing can go stale — it is *not* safe on the
LSP's long-lived database, which is why `collect_diagnostics` still checks
everything.

Emitted bytecode is **byte-identical** either way, at every optimization level;
`tests/stdlib_prefix_equivalence.rs` compiles a corpus both ways and compares
the serialized programs, so a divergence fails CI. That oracle is why the honest
helpers in `baml_project::testing` still exist — they are its control arm, not
dead code.

One thing deliberately *not* done: mounting the stdlib as a source-less
precompiled package (what runtime `reflect.Package.compile` does) is faster
again, but without stdlib bodies a direct sysop call lowers to `call` instead of
`sys_op`, and checks that walk stdlib bodies or declaration sites (E0153, E0163)
go silent. Speed there would mean testing a different artifact than we ship.

## Test Suites

| Suite | Location | Purpose |
|-------|----------|---------|
| `baml_tests` | `crates/baml_tests/` | Snapshot tests with detailed compiler IR output |
| `baml_ide` | `crates/baml_ide/` | Editor/agent query surface: hover, definition, references, tokens, describe |
| `baml_lsp` / `baml_lsp_server` | `crates/baml_lsp*/` | Protocol layer and transport, including the stdio end-to-end transcript |

## Workflow: Debugging a Failing Test

### 1. Identify the issue in the editor surface

```bash
cargo nextest run -p baml_ide -p baml_lsp -p baml_lsp_server
```

Cursor-position fixtures live inline in each `baml_ide` feature module.

### 2. Create a minimal repro in baml_tests

If the repro **compiles cleanly** and you want its IR, add it to the fixture
corpus:
```bash
mkdir -p crates/baml_tests/baml_src/ns_fixtures/ns_my_repro/
# ...write crates/baml_tests/baml_src/ns_fixtures/ns_my_repro/repro.baml
```

If the repro **must fail to compile**, add a project instead:
```bash
mkdir -p crates/baml_tests/projects/diagnostic_errors/my_repro/   # semantic errors
mkdir -p crates/baml_tests/projects/broken_syntax/my_repro/       # parse errors
```

### 3. Run and generate snapshots

```bash
cargo insta test --test-runner=nextest --accept -p baml_tests
```

### 4. Inspect the output

Fixture snapshots land next to the source in the mirrored snapshot tree,
`crates/baml_tests/snapshots/baml_src/ns_fixtures/ns_my_repro/`:

| Snapshot | Contents |
|----------|----------|
| `ppir.snap` | PPIR (post-expansion item tree) |
| `mir.snap` | MIR |
| `bytecode.snap` | Generated bytecode |
| `diagnostics.snap` | Warnings for that namespace (errors fail the run) |

Failing-to-compile projects snapshot under
`crates/baml_tests/snapshots/{diagnostic_errors,broken_syntax}/my_repro/`.

### 5. Fix the issue

Edit the relevant crate (`baml_compiler_parser`, `baml_compiler_syntax`, `baml_compiler2_hir`, etc.).

### 6. Re-run and update snapshots

```bash
# Update baml_tests snapshots
cargo insta test --test-runner=nextest --accept -p baml_tests

# Update baml_ide snapshots
cargo insta test --test-runner=nextest --accept -p baml_ide
```

### 7. Verify all tests pass

```bash
# Library unit tests — always run these when Rust code changes
cargo test --lib

# Run all tests
cargo nextest run -p baml_tests
cargo nextest run -p baml_ide -p baml_lsp -p baml_lsp_server
```

## Quick Commands

```bash
# Run specific test project
cargo nextest run -p baml_tests -E 'test(/my_project_name/)'

# Run all snapshot tests
cargo nextest run -p baml_tests

# Run just the corpus snapshot pass (PPIR/MIR/bytecode/diagnostics, one compile)
cargo nextest run -p baml_tests --lib -E 'test(/corpus_/)'

# Execute the BAML corpus the way CI does
target/debug/baml-cli test --from crates/baml_tests/baml_src

# Run the editor-surface tests
cargo nextest run -p baml_ide -p baml_lsp -p baml_lsp_server

# Accept all pending snapshots
cargo insta test --test-runner=nextest --accept -p baml_tests

# Review snapshots interactively
cargo insta review
```

## Key Files

- **Lexer**: `crates/baml_compiler_lexer/src/tokens.rs`
- **Parser**: `crates/baml_compiler_parser/src/parser.rs`
- **Syntax kinds**: `crates/baml_compiler_syntax/src/syntax_kind.rs`
- **AST helpers**: `crates/baml_compiler_syntax/src/ast.rs`
- **HIR lowering**: `crates/baml_compiler2_hir/src/body.rs`
- **Type checking**: `crates/baml_compiler2_tir/src/builder.rs`


DO NOT EDIT the diagnostics manually in the corpus fixtures. Use `cargo insta … --accept`

Find the base-case that makes syntax fail and add that to baml_test with a good name and good folder organization.

A good place to start when given a diagnostic failure or parser issue is to create a focused compiler test and inspect its diagnostics and IR snapshots.

BEFORE you run these lsp tests with UPDATE_EXPECT, make sure to just run without it and figure out if the new results are what you expect.

Just because the existing file may say 'no diagnostics expected' doesn't mean it is correct by the way. We haven't finished implementing all diagnostics. You have to see if we added some other comments elsewhere in the file to see what we should sort of expect, or just inspect the behavior manually.

---

# END-TO-END TESTING (running real BAML programs)

The snapshot/LSP suites above test the *compiler internals*. This section is about
testing BAML **end-to-end as a user would**: write a `.baml` program, compile it, run it,
and run its `test` blocks — using the real CLI. Use this when you want to know "does this
actually *work* when someone writes it", not "what CST does the parser produce".

## The binary

Build and use the local dev CLI (do **not** use a `brew`-installed `baml` — you must test
*this* checkout):

```bash
cargo build -p baml_cli           # produces target/debug/baml-cli
BAML="$PWD/target/debug/baml-cli"   # run from baml_language/
```

It prints `warning: using the internal BAML toolchain binary directly is not recommended` on
every invocation — that is expected; ignore/grep it out. The binary name is `baml-cli`
(hyphen), even though the crate is `baml_cli`.

## `baml describe` — the CLI **is** the stdlib documentation. Never guess.

The single most important tool for end-to-end work. The stdlib is large (~50 files,
`crates/baml_builtins2/baml_std/**.baml`); rather than guess method names, ask the binary:

```bash
$BAML describe baml                 # ← THE FULL PICTURE: every namespace, type & function
                                    #   in the stdlib in one listing (csv, env, errors, fs,
                                    #   http, json, math, net, time, toml, yaml, iter, …)
$BAML describe baml.json            # drill into a namespace → its types + function signatures
$BAML describe Array                # drill into a type → full method list + docs
$BAML describe String --budget 200  # output is line-budgeted; raise --budget to see all methods
$BAML describe <YourSymbol>         # also works on symbols in the loaded project
```

`describe` resolves symbols against a project. From inside a project dir it just works; from
elsewhere pass `--from <project-dir>`. Output is capped by `--budget` (default 30) and tells
you "… N more lines (re-run with a higher --budget)" — raise it to see everything. Anything
you can't see, `describe` it; do not guess stdlib names or signatures.

## Setting up a project

```bash
$BAML init <dir> --name <name>      # scaffolds <dir>/baml.toml + <dir>/baml_src/main.baml
                                    # (refuses to clobber an existing baml.toml)
$BAML new <dir>                     # like init but creates a fresh dir (errors if it exists)
```

`baml.toml` minimum is just `[package]\nname = "..."`. Source lives under `baml_src/**.baml`.
An optional `[scripts]` table aliases `baml run` invocations (e.g. `dev = "-f main"`).

## Running code

```bash
# Eval a one-off expression — fastest feedback loop, doubles as a syntax/type check.
# Runs WITHOUT a project; great for probing stdlib behavior in isolation.
$BAML run -e '1 + 2'                                  # → 3
$BAML run -e 'let xs=[3,1,2]; xs.length()'            # → 3
$BAML run -e 'baml.unstable.string(6)'                # → "6"

# Run a named function in the loaded project. The runtime builds a typed clap CLI from the
# function signature and exposes each function as a SUBCOMMAND, so the function name must be
# REPEATED after `--`, then its args as flags:
$BAML run main                                        # simplest for a zero-arg fn (positional target)
$BAML run --function main -- main                     # equivalent explicit form
$BAML run --function greet -- greet --name "Ada"      # scalar args: repeat the fn name, then --flags
$BAML run --function total -- total --json-args '{"xs":[3,1,2]}'   # collection/class/union args: use --json-args
# GOTCHA: bare `$BAML run --function main` (no `-- main`) prints a clap usage screen, not the result.
# GOTCHA: `--json-args` IS a real flag and is REQUIRED for array/map/class/union params (it goes
#         AFTER the repeated function-name subcommand).

# Compile-check the whole project without running:
$BAML check
```

`baml run` always prints a `Loading … / Checking … / Compiling …` preamble; the program's
result is printed after. On compile errors it prints rich diagnostics with `Error code: E####`
and exits non-zero with `Cannot run: compilation errors found`.

## Tests + assertions

```bash
$BAML test --list                    # discover tests without running
$BAML test                           # run all; prints PASS/FAIL per test, exits non-zero on failure
$BAML test -i "<testset>::<case>"    # run a single test by id
```

- A single test needs no wrapper: `test "name" { ... }`. `testset "name" { ... }` only **groups**.
- Assertions live in the `assert` namespace and **throw** (panic) on failure:
  `assert.is_true(cond)`, `assert.equal(actual, expected)`, `assert.not_null(v)`,
  `assert.contains(haystack, needle)`. A test passes iff its body runs without an uncaught throw.
- A failing assert surfaces as `UnhandledThrow { value: Instance { class_name:
  "baml.panics.UserPanic", … } }` with a stack trace pointing into `testing/registry.baml`.
- LLM functions (`client:` + `prompt:`) hit the network — **do not** rely on live LLM calls in
  e2e tests. Test deterministic logic, and for parsing bind the original inputs with
  `Fn@spec(args).parse(raw)` against a canned string (or use
  `baml.json.from_string<T>(...)`).
- Source streaming uses `Fn@stream(args)`; FunctionSpec is not streamable. The
  resulting `Stream<Out$stream, Out>` still uses PPIR's established partial
  types. If `unreflect(expr)` supplies an output type to either projection,
  bind it first (`type Out = unreflect(expr)`); an inline occurrence that
  escapes through `FunctionSpec<Out>` or `Stream<Out$stream, Out>` is E0168.
  Build those dynamic declarations and bindings inside BAML fixtures; host SDK
  tests should consume the resulting opaque values rather than constructing
  reflected types in the host language.

## A known-good reference program

```baml
function sum_list(xs: int[]) -> string {
  let total = 0;
  for (let x in xs) {        // for-loops need `(let …)` and iterate VALUES
    total += x;
  }
  return "sum=" + baml.unstable.string(total);   // no implicit int→string coercion
}

test "sums inline" {
  assert.equal(sum_list([3, 1, 2]), "sum=6")     // inline call form
}
```

## Language cheat-sheet (the traps that cost the most time)

BAML is expression-oriented and TypeScript-ish with snake_case methods. The five things that
bite first — everything else, `baml describe` it:

1. **Class fields are `name: type,`** (trailing comma); construct with `Point { x: 1 }`.
   Methods take an explicit `self`; static factories don't. `baml fmt` normalizes layout.
2. **Last expression in a block is its value** (Rust-style). Early exit is `return x;` (with
   trailing `;`). A no-value function is `-> null` with a trailing `null`.
3. **`for (let x in xs)`** iterates values and requires `let`. `if` / `match` / blocks are
   expressions; `match (v) { 0 => "a", _ => "b" }`.
4. **No implicit string coercion** — `"n=" + 5` will NOT compile; use `baml.unstable.string(5)`.
   Indexing out of bounds **panics** — use `.at(i)` / map `.get(k)` which return `T?`.
   Closures are `(x: T) -> R { ... }`; the `=>` arrow is **match-only**. `.map`/`.filter`
   return arrays directly (no `.collect()`). **Map keys must be `string`.**
5. **`catch` arms are type-only and non-exhaustive**: `f(x) catch (e) { BadInput => fallback }`.
   `throws T` is part of a function's signature; **panics are not catchable**.

## Recommended workflow

`baml describe baml` (full picture) → sketch the program → `baml run -e` / `baml check`
constantly for fast feedback → `baml describe <name>` whenever you need a signature →
`baml test` → `baml fmt baml_src/*.baml` before finishing.

## Finding & reporting language bugs end-to-end

When something behaves wrong, **prove it is the language, not your code**:

1. Reduce to the **smallest** `.baml` (or `baml run -e` one-liner) that still shows it.
2. Confirm the *intended* behavior via `baml describe` / the stdlib source so you're sure it's
   a real defect and not a misuse.
3. Record an exact repro: the minimal source, the exact command, observed vs. expected, and the
   `E####` error code or runtime throw. Classify severity:
   `crash` (VM/compiler panic or internal error) > `wrong-result` > `spurious-compile-error`
   (rejects valid code) > `missing-error` (accepts invalid code) > `bad-diagnostic`.
4. A bug that only reproduces inside one construct (e.g. only inside a `test` block, only under
   a generic) is worth noting — the construct boundary is a strong clue to the root cause.

Example of a real defect found this way: inside a `test` block, a `let`-bound local does not
compare equal to a literal of the same value — `test "x" { let r = "x"; assert.equal(r, "x") }`
**fails**, while the inline form `assert.equal("x", "x")` and `run -e 'let r="x"; r=="x"'` both
**pass**. Tiny source, exact command, observed≠expected, construct-scoped → that's a good report.
