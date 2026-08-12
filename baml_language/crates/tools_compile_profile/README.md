# tools_compile_profile

Standalone profiling harness for the BAML compiler pipeline.

This is the tool to reach for when someone says **"`baml check` is slow on
my project"**. It runs the same pipeline `baml check` runs
(parse → HIR → PPIR → TIR → MIR → emit), and produces:

- Wall-clock time split between `db build`, `check` (diagnostics), and
  `emit` (bytecode)
- Salsa query execution counts, cache hits, and blocked-on counts
- Per-phase aggregate (which pipeline stage is doing the most query work)
- Top-N most executed queries
- A "suspect" table of queries that fire many times but rarely hit cache

The tool is intentionally black-box: it uses only `salsa::Event`
callbacks, so it needs no changes in the compiler under test. You can run
it against any past commit as long as `baml_project` still exists.

Note: this binary uses **mimalloc** as its global allocator (the compiler
workload is dominated by small short-lived allocations, and system malloc
overhead would otherwise skew wall-clock numbers). Keep that in mind when
comparing against `baml check` wall time measured with a different
allocator.

## When to use it

- To answer **"which pipeline stage is eating my compile time?"** — read
  the "Query events by phase" section.
- To answer **"which specific query is executed a suspiciously high
  number of times?"** — read the "Top queries by executions" section.
- To answer **"is there work I could have cached but didn't?"** — read
  the "Suspect: high exec, low cache hit" section.
- For a **line-level CPU flamegraph** of what's actually hot inside a
  slow query, wrap this binary with `samply` (see below).

## What it is *not*

- Not a per-query self-time measurement. Salsa doesn't emit a
  `DidExecute` event, so it's impossible to bracket a query body from the
  outside. For self-time, use `samply` (which samples the actual thread
  stacks).
- Not a monitor for the LSP path. It runs `check + get_bytecode` from a
  fresh database, which mirrors CLI `baml check`, not the incremental
  editor path.

## Cache mode: cold by default

By default (no `--warm-runs`) every measured run is **cold**: a fresh
`ProjectDatabase` is built per run, so Salsa's memoization cache starts
empty. This mirrors `baml check` from the CLI — one process, one db, no
re-use. `cache hits: 0` in the report is *expected*, not a bug.

There is no other cache to disable. Cargo's build cache is unrelated
(that only affects compiling Rust; the profiler binary is already built
before it runs). Inside the pipeline, the only in-process cache is
Salsa's memoization, and this tool defeats it structurally by building a
new database per run.

To also measure the flip side — "if Salsa's cache *were* effective
across `baml check` invocations, how much would it save?" — pass
`--warm-runs N`. After each cold run, the tool invokes `check` +
`get_bytecode` `N` more times against the *same* database (no input
mutation). Only the first (cold) invocation populates the cache; the warm
invocations then find Salsa's query cache fully warm, so any query *body*
that still runs signals a caching gap. Note that warm runs are **not**
pure cache lookups: the wrapper work around the queries (`db.check()` /
`db.get_bytecode()` materialization, walking, cloning) is not memoized and
runs on every invocation — that uncached wrapper cost is exactly what the
cold-vs-warm table isolates below.

The cold-vs-warm table then makes three failure modes visible:

1. **Warm re-executes queries.** Something is being invalidated even
   though inputs haven't changed → caching gap inside a tracked query.
2. **Warm executes 0 queries but still spends real wall-clock time.**
   That work lives outside any Salsa query. Salsa can't cache it — the
   wrapper (`db.check()` / `db.get_bytecode()`) is doing per-invocation
   materialization / walking / cloning. **This is what we see on the
   test corpus today for `emit`: cold 13.6s, warm 12.1s.**
3. **Warm total ≈ 0.** Salsa is fully caching this workload; nothing
   further to chase in the caching layer.

## Usage

```bash
# Build once
cargo build --release -p tools_compile_profile

# Point at a BAML project root (a directory containing a baml_src/ dir,
# typically alongside baml.toml) or any directory of .baml files
./target/release/tools_compile_profile /path/to/baml/project

# Cold + warm re-runs on the same db — measures Salsa's cache effect
./target/release/tools_compile_profile --warm-runs 2 /path/to/project

# JSON output (diff two runs, feed a spreadsheet, etc.)
./target/release/tools_compile_profile --json /path/to/project > run.json

# Multiple cold-cache runs for variance measurement
./target/release/tools_compile_profile --repeat 5 /path/to/project

# Just measure `check`, skip bytecode (matches what `baml check` does
# when there are diagnostic errors)
./target/release/tools_compile_profile --check-only /path/to/project

# Compact one-liner for logs / dashboards
./target/release/tools_compile_profile --summary-line /path/to/project | tail -1
```

## Combining with `samply` for a real CPU flamegraph

The event-based stats above tell you **how often** each query runs. To
find **which lines inside a query are hot**, sample the process:

```bash
# Build with symbols; the workspace `release` profile already keeps them
cargo build --release -p tools_compile_profile

# Record and open in the web UI (macOS)
samply record ./target/release/tools_compile_profile /path/to/project

# On Linux
samply record ./target/release/tools_compile_profile /path/to/project
```

`samply` opens a browser UI with an inverted call tree, a top function
list, and a flamegraph — read those views alongside the query counts
from this tool. The two together answer:

- "which queries do the most work?" (this tool)
- "which lines inside those queries are hot?" (samply)

## Interpreting the report

Sample output for the built-in test corpus (~78 files, 25k lines) as it
looked **before the July 2026 optimization pass** (see the audit section
below — the same corpus now compiles in ~0.5s):

```text
--- Wall clock (representative run) ---
  db build (inputs)         0.002 s
  check                     8.107 s
  emit (bytecode)          13.245 s
  TOTAL                    21.354 s

--- Query events by phase ---
  phase          executions   cache hits      hit %
  tir                 19016            0      0.00%
  hir                  6697            0      0.00%
  parser                132            0      0.00%
  ppir                  132            0      0.00%
  lexer                 132            0      0.00%

--- Top queries by executions ---
  exec     hits     blocked  hit%      query
  15863    0        0        0.0    % infer_scope_types
  2304     0        0        0.0    % function_in_scope_generic_param_bounds
  2298     0        0        0.0    % function_body
  2172     0        0        0.0    % callable_throws
  ...
```

Reading this:

- **`hits` is 0 everywhere.** That's expected on a cold `baml check` —
  we build a fresh database, so nothing is in the cache. If you want to
  measure the incremental path (LSP), you'd need to run twice against
  the same db (not what this tool models). For CLI `baml check`, cold
  hits are the realistic case.
- **`tir` is 19k / 26k = 72% of all query work.** Most of the compile is
  scope-level type inference.
- **`infer_scope_types` alone fires 15k times.** This is per-scope (a
  scope is a function body / block / lambda / match arm). On this
  workload that's ~200 scopes/file. Whether that's excessive depends on
  the code shape.
- **`emit` takes more wall-clock time than `check`** on this workload.
  That's an outlier worth checking with `samply` because there aren't
  many emit-level Salsa queries — most of emit's cost is in the
  bytecode walker itself.

## The July 2026 cold-compile audit

> **Historical record.** The numbers in this section were measured on the
> `perf/compiler2-cold-compile` branch (PR #4016) **before** two large
> upstream changes landed on canary: #4032 (deleted TIR's `StructuralTy`
> type algebra; alias equivalence now goes through the canonical
> `baml_type::normalize` algebra via `AliasEquivCtx`) and #3924
> (content-addressed on-disk incremental caching). #4032 independently
> fixed the single biggest finding below (item 1, the recursive-alias
> re-analysis in the hot path), and #3924 supersedes item 5 (builtin
> stdlib diagnostics are now cached per toolchain on disk). The remaining
> findings are being re-landed piecewise as small independent PRs, each
> re-measured against current canary. The table is kept as-is as a record
> of the original audit and its methodology.

This section documents the audit this tool was built for, both as a
record of *why* the compiler's hot paths look the way they do now and as
a worked example of the methodology. Several `// See
crates/tools_compile_profile/README.md` comments in the compiler point
here.

**Result: cold `check` + `emit` on the test corpus (78 files, 25,838
lines) went from 16.0s to ~0.50s — a ~32× single-threaded improvement.**
End-to-end CLI `baml check` wall time went from ~17s to ~0.55s on the
same corpus. No parallelism was added; every win was removing redundant
work, memoizing package/project-invariant computation as Salsa queries,
or cutting allocation churn.

### Methodology

1. Run this tool for query execution counts. Any query that executes
   more times than there are "natural" keys for it (files, scopes,
   packages) is doing duplicate work.
2. Wrap the tool with `samply` for a CPU profile. Self-time
   concentrated in `clone`/`drop`/`malloc` means the *representation*
   is the problem, not any single call site.
3. Fix the biggest thing, re-measure, repeat. Every change below was
   individually measured on the same corpus; numbers are cold-cache
   medians of 5 runs.

### What we found and fixed, in order

| # | Change | Cold time after | Notes |
|---|--------|-----------------|-------|
| 0 | Baseline | 16.0s | `check` 8.1s, `emit` 13.2s in the worst measurement |
| 1 | Hoist recursive-alias analysis into `ResolvedAliases` | 4.0s | `find_recursive_aliases` (a whole-alias-graph DFS) ran on **every** `is_subtype_of` / `is_same_normalized_type` call — 18k times. It now runs once per package; callers pass `&ResolvedAliases` (aliases + precomputed recursive set) end-to-end. |
| 2 | `package_resolved_aliases` + `package_impl_locs` as tracked queries | 2.7s | The alias map and impl-block list were rebuilt (full clone of every alias `Ty` + project walk) inside every `infer_scope_types` execution — ~16k times. |
| 3 | `file_ast` tracked query (CST → AST once per file) | 1.9s | Six consumers (HIR + PPIR semantic indexes, `ppir_expansion_items`, two project-wide expansion-map collectors, LSP check) each re-lowered the syntax tree from scratch: ~31% of cold CPU was repeated CST traversal. |
| 4 | `class_type_tags_for_project` tracked query | 1.5s | The project-wide class → type-tag map was rebuilt (walking every file's item tree) once per *function* lowered in MIR, ~13% of cold CPU. |
| 5 | Skip diagnostics collection for builtin stdlib files | 1.5s | The builtin stdlib shipped with the compiler was eagerly type-checked on every `baml check` (a fixed ~70ms floor even for a 1-line project) and can never produce an actionable diagnostic. Builtin scopes user code touches are still inferred lazily. |
| 6 | mimalloc as global allocator (CLI + this tool) | 0.87s | macOS system malloc was ~35% of remaining CPU — the workload is dominated by small, short-lived `Ty`/`Vec`/`SmolStr` allocations. This is mitigation, not cure; see "future work" below. |
| 7 | Subtype-check fast paths (`baml_type::normalize`) | 0.76s | Reflexivity short-circuit, and co-inductive assumption bookkeeping (deep clone + full-tree hash of the pair) restricted to the *expanding* arms (μ-types, type vars, projections) — the only arms through which a cycle can regress. Millions of purely structural steps skip it. |
| 8 | `heads_definitely_differ` filter in `is_same_normalized_type` | 0.66s | MIR impl dispatch probes every candidate impl pattern against the receiver; the common case is a miss between types with different nominal heads, decidable without the two allocation-heavy normalization walks. |
| 9 | PPIR `file_semantic_index` delegates to HIR's when a file has no `$stream` expansions | 0.63s | The post-expansion index is byte-for-byte the pre-expansion one for such files; it was rebuilt anyway. |
| 10 | `callee_generics_for_func` tracked query | 0.58s | Every call site re-derived the callee's declared generic params and re-lowered its bound type exprs (~10% of cold CPU). |
| 11 | Nested-lambda inference projection | 0.56s | Every lambda body was type-inferred **twice**: inline while inferring the enclosing function, then again from scratch by the standalone `ScopeKind::Lambda` query. The inline pass now captures the lambda's complete tables (`NestedLambdaInference`, moved not cloned) and the Lambda query projects them. This also fixed a user-visible bug: diagnostics inside lambdas were reported twice. |
| 12 | Interface-method-name pre-filter in MIR dispatch | 0.53s | `dispatch_target_for_concrete` enumerated every impl block in the package closure for every method call *and field access*. A per-package set of all interface-declared method names rejects the common plain-method case in one hash lookup (`is_same_normalized_type` calls: 952k → 275k). |
| 13 | `ppir::function_body` tracked | 0.50s | MIR fetches the callee's body at every direct-call site; the untracked version cloned the entire `ExprBody` arena out of the item tree each time. |

Two further fixes affected the CLI path rather than the pipeline:

- **Diagnostic rendering built ariadne's line index per diagnostic.**
  `render_diagnostics` constructed a fresh `SourceCache` — a line index
  over *every file in the project* — for each diagnostic. On a corpus
  with 79 warnings that was 34% of `baml check` wall time. The cache is
  now built once per batch. (CLI: 0.94s → 0.55s.)
- **Salsa cycle recovery moved with the query head.** Converting
  `package_resolved_aliases` into a tracked query moved a legitimate
  dependency cycle (aliases whose RHS is an associated-type projection
  resolve through `infer_scope_types`, which consults the alias map)
  onto a query without `cycle_initial`. It now seeds fixpoint iteration
  with the empty alias environment, mirroring `infer_scope_types`.

### What's deliberately NOT here

- **No parallelism.** All wins are single-threaded; parallel check/emit
  remains available as a future multiplier.
- **No on-disk cache.** Verified: the compiler has no bytecode/query
  cache on disk. Every `baml check` is a cold in-process Salsa run.

### Known remaining costs (future work)

- **Allocation churn is mitigated, not fixed.** ~25% of remaining
  self-time is `malloc`/`free`/`memmove`/`clone`/`drop` of `Ty` trees,
  which have deep value semantics (`Vec<Ty>`/`Box<Ty>` children,
  inline `QualifiedTypeName`s). The foundational fix is hash-consing /
  interning (`Ty` as a copyable handle; equality as integer compare),
  staged as: intern `QualifiedTypeName` → intern `Ty` behind smart
  constructors → arena the `NormalTy` normalization temporaries.
- **Files with `$stream` expansions build their semantic index twice**
  (pre-expansion HIR + post-expansion PPIR) — 79 of 132 corpus files;
  worth ~30–50ms.

## Adding a new query to the phase breakdown

Salsa's `Ingredient::debug_name()` returns just the function name (e.g.
`"file_semantic_index"`) with no crate qualification, so we can't group
by crate. When you add a new `#[salsa::tracked]` function to the
compiler, add its name to `phase_for_query` in `src/main.rs` so it lands
in the right phase bucket instead of `"other"`.

Same-named queries from different crates (e.g. HIR and PPIR both expose
`file_semantic_index`) are disambiguated in the report by their
`IngredientIndex` — the report shows them on separate lines with a
`[IngredientIndex(N)]` suffix.
