# Which Layer Should I Change?

**Pipeline**: Parser -> AST -> HIR -> PPIR -> TIR -> MIR -> Emit

**Golden rule**: Push changes as early as possible. If you can express it as a desugaring in AST, don't add constructs to MIR or Emit.

---

## Quick Reference

| Layer      | Its one question                                   | Change it when...                                                 |
| ---------- | -------------------------------------------------- | ----------------------------------------------------------------- |
| **Parser** | What's the syntax?                                 | Adding new keywords, punctuation, block shapes                    |
| **AST**    | What's the core language after desugaring?         | **Most new features go here.** Rewriting sugar into builtin calls |
| **HIR**    | What names/scopes exist?                           | Adding item kinds, changing name resolution or scope rules        |
| **PPIR**   | What does the program look like with stream types? | Working on streaming                                              |
| **TIR**    | What type does everything have?                    | Adding type constructs, subtype rules, narrowing, diagnostics     |
| **MIR**    | What's the control flow graph?                     | New control flow that can't use existing terminators (rare)       |
| **Emit**   | What's the bytecode?                               | New VM instructions or program structure changes (rarer)          |

---

## Decision Flowchart

For any new feature, work top-to-bottom and stop as early as you can:

1. **New syntax?** -> Parser + AST
2. **Can it desugar into a builtin call?** -> AST only (+ add builtin to `baml_std`) -- **most features stop here**
3. **New named item kind?** -> HIR
4. **New typing rules?** -> TIR
5. **New control flow?** -> MIR (try hard to avoid)
6. **New VM instruction?** -> Emit (try harder to avoid)

---

## The `baml_std` Escape Hatch

When you need new runtime behavior, the cleanest path avoids touching TIR/MIR/Emit entirely:

1. Add a `.baml` stub in `crates/baml_builtins2/baml_std/` with `$rust_function` or `$rust_io_function`
2. Implement the Rust side in `bex_vm/src/package_baml/`
3. Desugar user syntax into a call to your new builtin (AST layer)

The type checker sees a normal function call. MIR sees a normal call. Emit produces a normal call instruction. Only the VM knows it's special.

---

## Design Decisions by Layer

Each row shows something that *was* placed in a layer and something that *wasn't*, with the reasoning.

### Parser

| Decision | Reasoning |
|---|---|
| **Did**: Distinguish LLM bodies from expression bodies via token scan | The two produce different CST shapes -- structural, hard to recover later |
| **Didn't**: Capture analysis, attribute validation, duplicate detection | Semantic concerns -- parser only knows structure |

### AST

| Decision                                                                                     | Reasoning                                                             |
| -------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| **Did**: LLM functions -> `call_llm_function(...)` builtin call + `DeclarativeMeta` sidecar  | No downstream layer has an "LLM function" concept -- it's just a call |
| **Did**: Client blocks -> `Let` + `$new` function; retry policies -> `Let`                   | Declarative blocks become ordinary items                              |
| **Did**: Companion functions (`$render_prompt`, `$build_request`) as ordinary `FunctionDef`s | Generated at desugaring time, invisible to later layers               |
| **Did**: Give lambdas their own `ExprBody` arena                                             | Separates expression IDs before scope analysis                        |
| **Didn't**: Capture analysis                                                                 | No scope information yet -- that's HIR's job                          |

### HIR

| Decision                                                                                  | Reasoning                                                   |
| ----------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| **Did**: Lambda capture analysis (which names, from which scope)                          | Earliest point with scope info; doesn't need types          |
| **Didn't**: Transitive captures (intermediate lambdas forwarding captures they don't use) | Requires walking the nested lambda tree, which only MIR has |
| **Didn't**: `Cell<T>` or any capture *implementation* detail                              | HIR says "captured"; MIR decides "cell-wrapped"             |

### TIR

| Decision | Reasoning |
|---|---|
| **Did**: Bidirectional type checking, narrowing, exhaustiveness, throw sets | All questions about "what type does this have?" |
| **Didn't**: `Cell<T>` type for captured variables | A captured `int` is just `int` -- runtime storage is irrelevant to type safety |
| **Didn't**: Distinguish captured vs non-captured variables | Captured names are seeded into `locals` as ordinary variables |

### MIR

| Decision | Reasoning |
|---|---|
| **Did**: `Cell` wrapping, `Place::Capture`, `MakeClosure`, transitive captures | Runtime representation of closures -- needs locals and places, which only MIR has |
| **Didn't**: Cell read/write *instructions* (`LoadDeref`, `StoreDeref`, `MakeCell`) | MIR marks `is_captured = true`; Emit picks the bytecode encoding |

### Emit

| Decision | Reasoning |
|---|---|
| **Did**: Cell preamble (`MakeCell`/`LoadDeref`/`StoreDeref` instructions) | Bytecode encoding of MIR's `is_captured` flag |
| **Did**: `FunctionMeta::Llm` -- materializing the prompt template sidecar into program output | This is where `DeclarativeMeta` (set in AST) finally becomes output |
| **Didn't**: Any semantic analysis | Emit doesn't decide *what* is captured or *which* functions are LLM -- it encodes decisions made earlier |

---

## Cross-Cutting Example: How Closures Split Across Layers

One feature, each layer takes exactly one concern:

| Layer | What it does for closures | What it leaves to the next layer |
|---|---|---|
| Parser | Parses `LAMBDA_EXPR` uniformly | Everything else |
| AST | Gives lambda its own `ExprBody` arena | Which names are captured |
| HIR | Records captures + marks `captured_names` on defining scopes | Transitive captures, runtime representation |
| TIR | Types captured vars as ordinary variables (no `Cell<T>`) | How to store/access them |
| MIR | `is_captured` flag, `Place::Capture`, transitive forwarding, `MakeClosure` | Bytecode encoding |
| Emit | `MakeCell`/`LoadDeref`/`StoreDeref` instructions | Done |

---

## When to Introduce a New Layer

Almost never. All three must be true:

1. **Distinct question** no existing layer answers
2. **Must sit between two layers** -- it replaces one layer's output for the next
3. **Multiple downstream consumers** benefit from the cached result

### Why PPIR qualifies

Stream expansion can't live in any existing layer:

| "Put it in..." | Why not |
|---|---|
| AST | Needs cross-file resolution (what classes exist, what annotations they have) -- AST sees one file at a time |
| HIR | HIR answers "what names exist?" -- synthesizing new types from `@stream.*` semantics is a different question |
| TIR | TIR should see stream types as *input*, not compute them inline -- otherwise every TIR query needs "is this a stream type I need to expand first?" |

PPIR intercepts HIR queries (`package_items`, `namespace_items`) and returns augmented versions. All downstream layers get stream types for free. That **view transformation** pattern -- replacing a layer's output for all consumers -- is what justifies a new layer.

### Red flags you don't need one

- "I need a transformation on the IR" -> pass within MIR or desugaring in AST
- "I need to collect info across the program" -> Salsa query in the right existing layer
- "Before type checking" -> HIR or PPIR
- "After type checking" -> MIR
