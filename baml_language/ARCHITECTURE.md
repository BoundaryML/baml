# compiler2: Architecture Reference

> **Audience:** All engineers on the BAML team and coding agents operating in the `baml_language/` workspace.
>
> **Purpose:** This document explains the design, pipeline stages, invariants, and decision framework of the compiler2 system. It is the authoritative reference for understanding where new features should be implemented, what each layer is responsible for, and why specific boundaries exist.

---

## Table of Contents

1. [Pipeline Overview](#pipeline-overview)
2. [The Cardinal Rule: Upstream Over Downstream](#the-cardinal-rule-upstream-over-downstream)
3. [Layer-by-Layer Reference](#layer-by-layer-reference)
   - [Parser (Lexer + CST)](#parser-lexer--cst)
   - [AST (Abstract Syntax Tree)](#ast-abstract-syntax-tree)
   - [HIR (High-level Intermediate Representation)](#hir-high-level-intermediate-representation)
   - [PPIR (Post-Process IR / Stream Type Expansion)](#ppir-post-process-ir--stream-type-expansion)
   - [TIR (Typed Intermediate Representation)](#tir-typed-intermediate-representation)
   - [MIR (Mid-level Intermediate Representation)](#mir-mid-level-intermediate-representation)
   - [Emit (Bytecode Generation)](#emit-bytecode-generation)
4. [Query-Based Architecture (Salsa)](#query-based-architecture-salsa)
5. [Packages and Name Resolution](#packages-and-name-resolution)
6. [Scopes](#scopes)
7. [CST-to-AST Desugaring: Detailed Examples](#cst-to-ast-desugaring-detailed-examples)
   - [Companion Functions](#companion-functions)
   - [Client Desugaring](#client-desugaring)
   - [Lambda Expression Bodies](#lambda-expression-bodies)
8. [Global Let Bindings and Initialization Order](#global-let-bindings-and-initialization-order)
9. [The Type System: Key Concepts](#the-type-system-key-concepts)
   - [Freshness and Widening](#freshness-and-widening)
   - [Unknown, Missing, and Error Types](#unknown-missing-and-error-types)
10. [Loop Desugaring and Diagnostic Preservation](#loop-desugaring-and-diagnostic-preservation)
11. [Span Preservation](#span-preservation)
12. [The Standard Library](#the-standard-library)
13. [Debugging and Snapshot Tests](#debugging-and-snapshot-tests)
14. [Rules for Adding Spans to Data Structures](#rules-for-adding-spans-to-data-structures)
15. [Mutability](#mutability)
16. [Bidirectional Type Checking](#bidirectional-type-checking)
17. [Unions in Type Checking](#unions-in-type-checking)
18. [Recursive Types](#recursive-types)
19. [Salsa Early Cutoff: How Edits Stay Local](#salsa-early-cutoff-how-edits-stay-local)
20. [The Standard Library: Dual Pipeline](#the-standard-library-dual-pipeline)
21. [Testing Infrastructure: Phases and Incrementality](#testing-infrastructure-phases-and-incrementality)
22. [Decision Framework Summary](#decision-framework-summary)

---

## Pipeline Overview

The compiler2 pipeline processes BAML source through a series of representations. There are exactly **three transformations** that produce new data structures, and several **query layers** that answer questions about those structures without transforming them:

```
Source Text
    |
    v
 [Lexer] ──> Tokens
    |
    v
 [Parser] ──> CST (Concrete Syntax Tree)
    |
    |  ← transformation: CST → AST
    v
  AST (Abstract Syntax Tree)
    |
    |  ← query layer (no transformation)
    v
  HIR (names, scopes)
    |
    |  ← expansion: synthesizes stream types, feeds back into HIR
    v
  PPIR (stream type expansion)
    |
    |  ← query layer (no transformation)
    v
  TIR (types)
    |
    |  ← transformation: AST → MIR
    v
  MIR (control flow graph)
    |
    |  ← transformation: MIR → bytecode
    v
  Emit (bytecode for BexVM)
```

**Critical distinction:** The stages above the AST (Parser, CST→AST lowering) are about *producing* the AST. The stages below it (HIR, PPIR, TIR) are about *answering questions* about the AST. They do not produce new syntax trees. The MIR is the second transformation — it converts human-friendly BAML into a machine-friendly control flow graph. The Emit stage is the third transformation — it compiles MIR to bytecode.

This is fundamentally different from the compiler1 architecture, which was a strict linear pipeline where each layer copied and enriched the previous layer's data. In compiler2, each layer is a **query on top of the AST** (at least until MIR), which gives us Salsa-powered incremental compilation for free.

---

## The Cardinal Rule: Upstream Over Downstream

When deciding where to implement a feature, always ask: **what is the earliest layer at which I can do this?**

| Situation | Rule |
|---|---|
| Adding a feature | Put it in the **highest** (earliest) layer possible. Most features belong in the **AST** layer. |
| Changing AST | Relatively forgiving — this is where most work happens. |
| Changing HIR | Discuss with at least one person who works in HIR. |
| Changing TIR | Discuss with at least one person who works in TIR. |
| Changing MIR or Emit | Discuss with at least two people. You are almost certainly making a mistake unless you have a very specific reason. |
| Adding a new layer | Requires explicit approval from the tech lead and a senior contributor. No new layers without significant deliberation. |

**The lower you go, the more scrutiny is required.** Changes to downstream layers cascade into every code path on the team's surface area. Keeping boundaries clean means fewer bugs and fewer accidental coupling problems.

---

## Layer-by-Layer Reference

### Parser (Lexer + CST)

**Crate:** `baml_compiler_lexer`, `baml_compiler_parser`, `baml_compiler_syntax`

**Responsibility:** Grammar only. The parser answers the question *"is this syntactically valid BAML?"* It knows about keywords, punctuation, delimiters, and the structural grammar of the language. It makes no semantic decisions.

**What lives here:**
- Token definitions (keywords, operators, punctuation)
- Grammar rules for all syntactic constructs
- Error-tolerant parsing (the parser produces a tree even for malformed input)
- The distinction between LLM function bodies, regular function bodies, and config blocks

**What does NOT live here:**
- Any understanding of what names mean
- Any understanding of types
- Any semantic validation

The parser produces a **CST** (Concrete Syntax Tree), which is a lossless, error-tolerant representation of the source text. It uses a green/red tree architecture (similar to rust-analyzer's `rowan`).

**Key Salsa query:** `syntax_tree(db, file) -> CST`

---

### AST (Abstract Syntax Tree)

**Crate:** `baml_compiler2_ast`

**Responsibility:** Desugaring. The AST takes the CST and produces a well-formed, semantically-oriented syntax tree. This is where most features live.

**What lives here:**
- **Companion function expansion** — LLM functions are expanded into the base function plus generated companions (`render_prompt`, `build_request`, `parse`).
- **Client desugaring** — `client<llm>` blocks are desugared into a top-level `Let` binding (the `Client` object) plus an optional `$new` companion function (the `PrimitiveClient` constructor).
- **Lambda expression bodies** — A lambda's body is lowered into the enclosing function's arena and referenced by `ExprId`; the lambda gets its own scope, not its own arena.
- **LLM function normalization** — There is no concept of "LLM function" downstream. LLM functions become regular functions with declarative metadata attached.
- **Type expression lowering** — Source-level type syntax is converted to `TypeExpr` nodes.
- **Config item lowering** — Config block syntax (used in clients, generators, etc.) is lowered to AST expressions.

**What does NOT live here:**
- Anything that requires knowing the *name* of something (that's HIR).
- Anything that requires knowing the *type* of something (that's TIR).
- Anything that requires knowing whether something is a class, enum, or alias (that might be PPIR or TIR).

**Key design principle:** One CST node can produce *multiple* AST nodes. For example, a single `client<llm> MyClient { ... }` definition produces two AST items: a `Let` and a `Function`. Conversely, some CST constructs collapse or transform substantially. The AST is the **final syntactic form** of the program.

**The AST is a pure structural lowering.** It uses no Salsa queries. It does not validate names or detect duplicates. It simply converts CST shapes into AST shapes.

---

### HIR (High-level Intermediate Representation)

**Crate:** `baml_compiler2_hir`

**Responsibility:** Names and scopes. The HIR's sole job is to answer the question *"what are the names of things, and where are they declared?"*

**What lives here:**
- **Scope tree construction** — Every block of code gets a scope. Scopes are allocated in DFS pre-order and form a tree.
- **Name resolution** — Given a name at a position, the HIR walks up the scope tree to find where that name was declared.
- **Duplicate name detection** — Names are checked for conflicts within their scope.
- **Shadowing rules** — The HIR decides where shadowing is allowed (e.g., a match arm variable may shadow a function parameter).
- **Lambda capture analysis** — Which variables a lambda captures is determined here. You don't need to know the type to know *what* is captured, only what names are in scope.
- **Package and namespace aggregation** — Cross-file symbol merging happens here.
- **Item tree + type-reference arena** — A span-free `ItemTree` (items keyed by position-independent `LocalItemId`) with a parallel `ItemTreeSourceMap`, plus a flat span-free `TypeRef` arena that replaces inline `ast::TypeExpr`. `ItemTreeBuilder` constructs both together and records the item↔scope and method→owner indices. This is the substrate the PPIR firewall queries front (see [Salsa Early Cutoff](#salsa-early-cutoff-how-edits-stay-local)).

**What does NOT live here:**
- Node transformations. The HIR should NOT construct new AST nodes. If you find yourself doing that, the work belongs in the AST layer.
- Type information of any kind. If you need to know whether something is a class or an enum, that is a downstream concern.

**Key design decision — Lambda captures:** Captures are determined in HIR because you only need name information, not type information, to decide what is captured. The HIR records that a lambda captures variable `a` from the enclosing scope. It does *not* determine whether `a` is a direct capture or a transitive capture — that distinction only matters for the MIR (which builds the control flow graph and needs to understand transitive dependencies). The concept of a "cell" (an indirection pointer for mutable captured variables) also does NOT belong in HIR or TIR. From the TIR's perspective, a captured `int` is still an `int` — the indirection is purely a MIR/VM implementation detail.

**Key Salsa queries:**
- `file_semantic_index(db, file)` — Per-file scope tree with all bindings
- `namespace_items(db, namespace_id)` — Items contributed to a namespace
- `package_items(db, package_id)` — Package-level symbol table (merges all namespaces)

---

### PPIR (Post-Process IR / Stream Type Expansion)

**Crate:** `baml_compiler2_ppir`

**Responsibility:** Stream type generation. This layer exists because streaming types require type-aware code generation that cannot be done in the AST layer but must happen before the TIR.

**Why this must be its own layer:**
- To decide how to expand a streaming type, you need to know whether a type expression refers to a class, an enum, a union, or a type alias. Different kinds produce different stream expansions.
- You cannot answer those questions in the AST layer because the AST does not have name resolution.
- You cannot defer this to the TIR because the TIR needs the stream types to already exist in order to type-check streaming code.
- The PPIR does not perform the full type inference that the TIR does. It performs a narrow, purpose-specific form of type classification sufficient for stream expansion.

**What lives here:**
- Synthesis of `*$stream` variants for classes and type aliases
- Stream expansion logic (`stream_expand`, `expand_partial`)
- SAP (streaming attribute propagation) attributes
- **The canonical item layer** — `ppir::file_item_tree` merges original items with the synthetic stream items, and the per-item **firewall queries** (`item_data`: enumeration + `*_data` + `*_source_map`) that front the item tree live here.

**How it works:** The PPIR generates synthetic AST items (the stream variants) and re-runs the HIR builder over the *merged* list — originals first, then synthetics appended. The flow is HIR → PPIR → (re-run the HIR builder on merged items) → TIR. PPIR's tree is **canonical**, but it is an internal substrate: downstream layers read it **only through the firewall queries** in `item_data` (enumeration + `*_data` + `*_source_map`) — never `file_item_tree` directly, and never the HIR pre-expansion tree. A `ClassLoc`/`FunctionLoc` therefore unambiguously means a canonical item. (Originals keep identical `LocalItemId`s in the pre-expansion and canonical trees because they are allocated first, in the same order — which is what makes a HIR-derived `*Loc` safe to pass to a firewall query. This is enforced: `file_item_tree` is `pub(crate)` in both HIR and PPIR, so no downstream crate can reach the raw tree at all.)

**Key Salsa queries:**
- `ppir_expansion_items(db, file)` — Synthetic stream items per file
- Firewall queries (`item_data`) — the consumer API over the canonical item tree: enumeration (`file_classes` / `file_functions` / …), lookup (`class_data` / `function_data` / …), and spans (`class_source_map` / …). `file_item_tree` itself is the internal substrate these are built on, not for direct downstream use.

---

### TIR (Typed Intermediate Representation)

**Crate:** `baml_compiler2_tir`

**Responsibility:** Types only. The TIR answers the question *"what is the type of this expression?"* and validates type correctness.

**What lives here:**
- **Type inference** — Per-scope expression type maps.
- **Type checking** — Is this assignment valid? Is this function call well-typed?
- **Type narrowing** — After a type guard (e.g., `match`, `instanceof`), the type is narrowed.
- **Exhaustiveness checking** — Are all match arms covered?
- **Generic instantiation** — Resolving type parameters.
- **Type normalization** — Simplifying complex types.
- **Cycle detection** — Detecting recursive types.
- **Package interface generation** — Producing the fully-resolved type interface for each package.

**What does NOT live here:**
- Constructing new statements or expressions. The TIR is purely **informative** — it annotates existing AST nodes with type information.
- Syntax transformations of any kind.

**How type resolution works for local variables:** When you ask "what is the type of `x` on line 10?":

1. The TIR takes the expression ID from the current scope.
2. It asks the HIR: "where was this expression declared?" (This is an HIR question — the HIR knows where every expression is declared.)
3. The HIR returns the declaration site (e.g., line 5).
4. The TIR then asks: "what is the type of the expression at line 5 in that scope?"
5. This recursively resolves until a leaf type is reached.

**Key Salsa queries:**
- `infer_scope_types(db, scope_id)` — Per-scope type inference. This is the main query. It returns types for a single scope, NOT a monolithic per-function result. Note that a lambda body is *not* independently incremental: it lives in the enclosing function's `ExprBody`, so editing it invalidates that function's body query and hence its inference.
- `resolve_name_at(db, file, offset, name)` — On-demand name resolution with type information.

---

### MIR (Mid-level Intermediate Representation)

**Crate:** `baml_compiler2_mir`

**Responsibility:** Control flow graph construction. The MIR is the **first layer that performs a full walk of the AST** and produces a fundamentally different representation. It converts human-friendly BAML code into machine-friendly control flow graphs.

**What lives here:**
- **CFG (Control Flow Graph) construction** — Basic blocks, terminators, branching.
- **Loop unification** — All loop variants (C-style `for`, iterator `for`, `while`) become a single loop construct in MIR. The three source-level forms exist in the AST only for diagnostic quality (see [Loop Desugaring and Diagnostic Preservation](#loop-desugaring-and-diagnostic-preservation)).
- **Transitive capture analysis** — The MIR determines whether a capture is direct or transitive (does the outer lambda also need to capture `a` because the inner lambda captures it?).
- **Cell (indirection) introduction** — Mutable captured variables become cells (indirect references) in MIR.
- **Lambda naming** — Lambdas get debug names based on their definition order and nesting depth (e.g., `anonymous_function_0`, `anonymous_function_0_1` for a lambda inside a lambda).

**Data structures:**
- `MirFunctionBody` — Basic blocks, entry block, local declarations, unwind handlers.
- `BasicBlock` — A sequence of statements plus a terminator.
- `MirFunctionKind::Bytecode(body)` — Functions with BAML code.
- `MirFunctionKind::Builtin(kind)` — Rust-bound builtins (SysOp for I/O, NativeUnresolved for VM intrinsics).

**Readability:** The MIR pretty-printer (`pretty.rs`) has been carefully designed to be readable for debugging. If you add a feature that touches MIR, you are responsible for maintaining the same level of readability. This is critical because MIR is the most bug-prone layer due to the complexity of the CFG transformation.

**Key Salsa queries:**
- `lower_function(db, ...)` — Lower a function to MIR.
- `lower_let_body(db, ...)` — Lower a let binding's initializer to MIR.

---

### Emit (Bytecode Generation)

**Crate:** `baml_compiler2_emit`

**Responsibility:** Compiles MIR to bytecode for the BexVM using stackification.

**You should almost never need to touch this layer.** Changes here should be very small and very well justified. The emit layer is straightforward in concept — it walks the MIR CFG and emits VM instructions — and bugs here are relatively rare compared to the MIR layer.

**What lives here:**
- Bytecode emission from MIR basic blocks
- Global slot allocation
- Package init function compilation (see [Global Let Bindings](#global-let-bindings-and-initialization-order))
- Optimization levels (`OptLevel`)
- Bytecode verification

---

## Query-Based Architecture (Salsa)

The compiler2 uses the [Salsa](https://salsa-rs.github.io/salsa/) incremental computation framework. The key idea is that each layer is defined as a set of **tracked queries** that depend on other queries. When a source file changes, Salsa automatically recomputes only the queries whose inputs changed.

**Database hierarchy (each layer extends the previous):**

```
salsa::Database
  └─ baml_base                 (source roots: path, package, kind, files)
      └─ baml_compiler_parser::Db  (syntax_tree query)
          └─ baml_compiler2_hir::Db  (file_semantic_index, namespace_items, package_items)
              └─ baml_compiler2_ppir::Db  (ppir_expansion_items, canonical queries)
                  └─ baml_compiler2_tir::Db  (infer_scope_types, resolve_name_at)
                      └─ baml_compiler2_mir::Db
                          └─ baml_compiler2_emit::Db
```

The design goal: **before the AST, produce the AST. After the AST, answer questions about the AST.** The only layers that do production (create new data structures) are:
1. Parser → CST
2. CST → AST (including PPIR feeding synthetic items back)
3. AST → MIR
4. MIR → bytecode

Everything else is a query.

---

## Packages and Name Resolution

### Package Resolution Order

Packages are resolved in **topological order** based on their dependency graph. The resolution order is inferred from declared dependencies, not hardcoded.

```
baml (standard library) ← resolved first, no dependencies
    |
    v
testing, insert, etc.   ← depend on baml, resolved next
    |
    v
user                     ← depends on baml (and possibly others), resolved last
```

**Packages must form an acyclic DAG.** Recursive package dependencies are not allowed.

**Why this matters for incremental compilation:** The standard library, testing, and other non-user packages are resolved once and cached. Only the user's package changes during editing, so only it needs to be recomputed in the editor.

### Package Resolution Context

The `PackageResolutionContext` is the **single point of entry** for all name resolution from the TIR. It handles three cases:

| Syntax | Resolution Strategy |
|---|---|
| `root.SomeName` | Look in the current package's root namespace |
| `SomeName` (unqualified) | Look in the current local scope, then walk up scopes |
| `some_package.SomeName` | Look in the external package's interface |

**Important invariant:** If you find code that accesses type system information outside of the package resolution context, that is a bug. Fix it and route through the resolution context to maintain a single point of entry.

### Package Interface

Every package exposes a `PackageInterface` — a fully resolved type interface that lists every name, every type, and full structural information. This is what other packages consume when they depend on you.

---

## Scopes

Scopes are constructed at **HIR time** (not AST time) because you cannot determine scope boundaries without name resolution. Consider: `Foo.Bar.baz` — is `Foo` a namespace, a class, or a variable? You need name resolution to answer that, so scopes and name resolution are co-determined in the HIR.

### Scope Hierarchy

```
Project (root)
  └─ Package
      └─ Namespace (can be nested, can span multiple files)
          └─ File
              └─ Top-level items: Function, Class, Enum, TypeAlias, Item (client/test/etc.), Let
                  └─ Block (curly-brace blocks with let bindings)
                      └─ Lambda
                      └─ MatchArm (pattern bindings visible to arm body and guard)
                      └─ CatchClause → CatchArm
```

### How Name Resolution Works

When resolving a name, the system walks **up** the scope tree:

1. Check the current scope's bindings (let bindings, parameters).
2. Check parent scopes, walking up until the file scope.
3. Check the package's namespace items.
4. Check the `baml` builtin package.

Shadowing rules are scope-kind-dependent. For example, a match arm can shadow a function parameter, but two parameters in the same function cannot shadow each other. The HIR decides where shadowing is allowed.

### Scope IDs

`ScopeId<'db>` is a Salsa tracked struct pairing a `SourceFile` with a `FileScopeId`. It is the key for per-scope queries like `infer_scope_types(db, scope_id)`. Scopes are allocated in DFS pre-order within each file.

---

## CST-to-AST Desugaring: Detailed Examples

### Companion Functions

When the AST layer encounters an LLM function, it expands it into the original function plus up to three **companion functions**:

| Companion | Name Pattern | Parameters | Return Type | Purpose |
|---|---|---|---|---|
| `render_prompt` | `FuncName$render_prompt` | Same as parent | `baml.llm.PromptAst` | Renders the prompt AST |
| `build_request` | `FuncName$build_request` | Same as parent | `baml.http.Request` | Builds the HTTP request |
| `parse` | `FuncName$parse` | `json: string` | Same as parent | Parses the JSON response |

**Implementation** (`baml_compiler2_ast/src/companions.rs`):

Companion expanders are pure functions of type `fn(&FunctionDef) -> Option<FunctionDef>`, stored in a const array `COMPANIONS`. Each expander inspects the function's `declarative_meta` — if it's an LLM function, it produces a companion; otherwise, it returns `None`.

Companion functions are **complete, self-contained AST items**. They flow through HIR → TIR → MIR → emit with zero special-casing. Downstream layers have no idea they were generated.

**Implication for duplicate name detection:** If you have two LLM functions `Foo` and `Foo` (a duplicate), each produces four AST items (itself + three companions). All eight items will trigger duplicate-name errors in the HIR. To prevent cascading duplicate errors, the HIR must be aware that companion-derived errors should not produce additional diagnostics beyond the root duplicate.

### Client Desugaring

A `client<llm>` block desugars into **two AST items**:

1. **A top-level `Let` binding** — Creates a `Client` object (defined in `baml_std`) with:
   - `name: string` — the client's name
   - `client_type: ClientType` — `Primitive`, `Fallback`, or `RoundRobin`
   - `sub_clients: Client[]` — for composite clients, references to sub-clients (as `Expr::Path` references enabling TIR name validation and topological dependency ordering)
   - `retry: RetryPolicy?` — optional retry policy (also an `Expr::Path` reference)
   - `counter: int` — for round-robin clients, the starting index

2. **An optional `$new` companion function** (primitive clients only) — A function `ClientName$new` that constructs a `PrimitiveClient` from the provider and options. This function is called at runtime to create the actual LLM-capable client.

**There is no `Client` type in the AST or compiler type system.** The `Client` and `PrimitiveClient` types are regular structs defined in the BAML standard library (`baml_std/baml/ns_llm/llm_types.baml`). The compiler synthesizes constructor expressions that instantiate these standard library types. This means client type-checking happens for free through the normal TIR — no special type-checking code is needed for clients.

**How `Client` resolves to `PrimitiveClient` at runtime:**
1. The `Client` object has a `get_constructor()` method that returns a Rust function pointer.
2. This function pointer is looked up by the client's name at runtime and returns a closure that constructs a `PrimitiveClient`.
3. The `PrimitiveClient` is the actual object that can render prompts, build requests, and parse responses.
4. The `PrimitiveClient` is constructed every time an LLM function is called (no caching currently — this is a known optimization opportunity).

**What about expressions in client definitions?** Because clients desugar to regular AST expressions, users can use arbitrary expressions in client option values. For example, a variable reference as the model name works automatically. The config block syntax uses colon-delimited key-value pairs which are parsed as a special form in the CST and lowered to expressions in the AST.

### Lambda Expression Bodies

A lambda's body is lowered into the **enclosing function's** `ExprBody`, and the
lambda holds an `ExprId` pointing at it — the same shape as rust-analyzer's
`Expr::Closure { body: ExprId }`. One arena per function (or top-level `let`),
never one per lambda.

Lambdas still get their own `ScopeKind::Lambda` scope, because both of the
reasons they need one are about *scopes*, not arenas:
- per-scope incremental inference,
- capture analysis in HIR, which needs each lambda to be a distinct scope.

Scopes and arenas are therefore orthogonal. Two consequences worth knowing:
- An `ExprId` is unambiguous within a function but **not** within a file, so
  file-level maps key on `ExprMetadataKey` (arena owner + id), which is rustc's
  `HirId { owner, local_id }`.
- Analyses that must not attribute a lambda's behaviour to its definer — effect
  (`throws`) inference, call-graph edges — cannot scan the arena flatly, because
  a lambda's expressions are siblings of the function's own. They walk
  structurally via `ExprBody::reachable_excluding_lambdas`.

---

## Global Let Bindings and Initialization Order

BAML has a special challenge: names can be referenced across files. This means global variables (like clients) have cross-file dependencies that must be resolved in a specific order.

### How it works

1. **Collection:** Every package's top-level `Let` bindings are collected.
2. **Topological sort of packages:** Packages are sorted by their dependency graph (e.g., `baml` before `user`).
3. **Topological sort of lets within each package:** Within each package, `Let` bindings are topologically sorted by their dependency edges (derived from `Expr::Path` references in their initializers). If a cyclic dependency is detected, the compiler emits an error.
4. **Init function compilation:** For each package, a `$init` function is compiled that evaluates the `Let` bindings in topological order, storing each result in a global slot.
5. **Package init order:** The VM receives a `package_init_order` list and calls each package's `$init` function in order during startup.

This is exactly how Go handles global variable initialization: topological sort across the dependency graph, then evaluate in order.

**Important:** Top-level `let` is **not** available in user-facing syntax (the lexer disallows it). It exists only in the AST layer for compiler-generated constructs like client desugaring.

---

## The Type System: Key Concepts

### Freshness and Widening

When you write `let x = 42`, you don't want `x` to have type `literal 42` — you want it to have type `int`. This is handled through **freshness** and **widening**, a concept borrowed from TypeScript:

- A literal on the right-hand side of an assignment is considered **fresh**.
- When a fresh literal is **assigned** to a variable (bound), it is **widened** to its base type: `literal 42` → `int`, `literal "hello"` → `string`.
- If a variable is explicitly typed as a literal type (e.g., `let x: 42 = 42`), the literal is already bound to a regular literal type and does not widen.
- Widening also applies when collecting into containers: an array of fresh literals becomes an array of the widened type.

### `Unknown`, `Error`, and `Infer`

Three `Ty` variants are easy to confuse. Only two of them are failure states:

| Type | Meaning |
|---|---|
| `Unknown` | The **top type** — the user-denotable `unknown` keyword, `T <: unknown` for all `T`. Not a failure: a parameter typed `unknown` is well-typed. |
| `Error` | The error-recovery sentinel: a hard type error was already reported here, so downstream checks suppress rather than cascade. Compiler-only — it has no `RuntimeTy`, and reaching runtime lowering with one is a compiler bug. |
| `Infer` | An inference hole (written `_`) or an inference variable. Compiler-only; every one must be filled or reported before finalize. |

**Debugging heuristic:** in snapshot output, `Unknown` renders as `unknown` and so does a genuine user-written `unknown`, so the two are indistinguishable on sight. If the source has no compile errors, every `unknown` should be a real top type. An unexpected one is worth tracing — `Error` is laundered to the top type at the MIR boundary today (see the `// BUG:` on `erase_compiler_only_ty`), so an `unknown` that should have been a diagnostic can appear here.

---

## Loop Desugaring and Diagnostic Preservation

BAML has three loop forms: C-style `for`, iterator-style `for`, and `while`. In the MIR, all three become a single loop construct — there is no difference at the CFG level.

**Why they remain separate in the AST:** Consider what happens if you desugar a C-style `for (let i = 0; i < arr.length(); i++)` into an iterator-style `for (let item in arr)` at the AST level. You would synthesize an imaginary iterator variable. If the iteration target is non-iterable, the type error would reference this synthesized variable that the user never wrote. The error message would be confusing and unhelpful.

By keeping three distinct AST forms, each loop variant can produce type errors that reference the actual user-written syntax. The MIR then unifies them after diagnostics have been emitted.

**General principle:** Before desugaring any construct, ask yourself: *"What error messages does each form produce? Do those error messages still make sense after desugaring?"* If desugaring would produce confusing diagnostics, keep the forms separate in the AST and unify in the MIR.

---

## Span Preservation

When performing CST→AST desugaring, **you must preserve span information** on every generated node. Every synthesized AST node must carry the source span of the CST construct it was derived from.

If you fail to do this:
- Error messages will point to the wrong location (or no location).
- Users will see confusing diagnostics.
- Coding agents will have difficulty diagnosing issues from snapshot output.

**If you find yourself hacking in incorrect spans** (e.g., using a dummy span or the wrong source location), stop and ask another team member whether the approach is correct. Incorrect spans are a persistent source of subtle bugs.

---

## The Standard Library

**Crate:** `baml_builtins2`
**Source:** `baml_builtins2/baml_std/baml/`

The BAML standard library is written in BAML itself (with some Rust-backed builtins marked with `$rust_type` and `$rust_io_function`). It defines core types, container types, LLM infrastructure, HTTP types, error types, math/string/net utilities, and more.

**Key files:**
- `core.baml` — Core types
- `containers.baml` — Generic `Array<T>`, `Map<K,V>`, etc.
- `ns_llm/llm.baml` — LLM types and client infrastructure
- `ns_llm/llm_types.baml` — `Client`, `PrimitiveClient`, `PrimitiveClientOptions`, `RetryPolicy`, etc.
- `ns_http/http.baml` — `Request`, `Response`
- `ns_errors/errors.baml` — Error types

**Adding to the standard library:** If you want to make new functions or types available to users, the standard library is the primary mechanism. You add BAML source files, and they compile through the normal pipeline. The standard library package (`baml`) is resolved first and is available to all other packages.

**Caution:** Standard library additions pollute the user's namespace. Be deliberate about what you add. Prefer putting things in sub-namespaces (e.g., `baml.llm`, `baml.http`) rather than at the root.

**For agents:** When implementing new language features, prefer adding new types and functions to the standard library rather than introducing new compiler-internal types. The type system should not be impacted unless something is truly unrepresentable with existing types.

---

## Debugging and Snapshot Tests

**Crate:** `baml_tests`

The snapshot test infrastructure is the primary debugging tool for the compiler2 pipeline. Each pipeline stage has its own snapshot format:

| Stage | What the snapshot shows |
|---|---|
| HIR | Scope tree, name bindings, declarations, capture information, lambda definitions |
| TIR | Every expression annotated with its inferred type (similar to IDE inlay hints) |
| MIR | Control flow graph with basic blocks, statements, terminators, local declarations |
| Emit | Bytecode disassembly |

**How to use snapshots for debugging:**

1. Write a BAML test case using the `baml_test!` macro.
2. Run `cargo test` — the snapshot is generated/updated.
3. Read the snapshot output for the relevant layer.
4. For TIR: search for `unknown` — any unexpected `unknown` is a bug.
5. For MIR: read the pretty-printed CFG — it shows basic blocks, terminators, and local types.

**This debugging loop is highly effective for coding agents.** Agents can write test cases, read snapshot output, identify issues, and iterate. The snapshot format was designed specifically to be readable by both humans and LLMs.

**Test macro:**
```rust
baml_test!("baml source code here")

// Or with options:
baml_test! {
    baml: "source",
    entry: "func_name",
    args: { "x" => val },
    opt: OptLevel::Zero,
}
```

---

## Rules for Adding Spans to Data Structures

**The load-bearing invariant: anything a memoized, `PartialEq`-compared query *returns* must be span-free.** Salsa only overwrites a memoized value when the new one is *not* `Eq` to the old, so a span inside a query result is a trap either way:

- If the span is inside `PartialEq`, a whitespace edit makes the value compare unequal and **destroys early cutoff** for everything downstream.
- If the span is *ignored* by `PartialEq` (as `ast::TypeExpr` does with its own `span`), the value compares equal, Salsa keeps the old memo, and the query **serves a stale span forever**.

Concretely:

- **Do not add `TextRange`/span fields to data that flows out of a tracked query.** Type references use the span-free `TypeRef` arena (ids into a per-item store), not `ast::TypeExpr`. Attributes use the span-free `item_tree::Attribute`, not `ast::RawAttribute` (whose `PartialEq` *includes* its span — a subtle transitive leak through any `TypeExpr`).
- **Spans live in a parallel `*_source_map` query**, keyed by the same id (`LocalItemId` / `TypeRefId`), never inline with the data.
- The one place spans may remain inline is the coarse `no_eq` index (`file_semantic_index` and its `ItemTree`): because that query is `no_eq` it is always recomputed, so its spans are always fresh — never stale. Everything *fronting* it (the firewall `*_data` queries) must be span-free.

If you're unsure how to associate span information with a new construct, ask before implementing.

---

## Mutability

BAML supports mutable variables. You can reassign variables (`x = newValue`), use compound assignment operators (`i += 1`, `x -= 1`, etc.), and mutate data structures via methods like `.push()`. The MIR models this through `Assign` and `AssignOp` statements, and mutable variables captured by lambdas are wrapped in cells (indirection pointers) so that inner and outer scopes can mutate the same value.

---

## Bidirectional Type Checking

The TIR implements **bidirectional type checking**, which means it switches between two modes at well-defined boundaries.

### Synthesis (bottom-up)

No expectation from the caller. The type is computed purely from the expression's structure. Used for: literals, variable references, field access, untyped calls. You give the type checker an expression and it tells you what type it is.

### Checking (top-down)

The caller knows what type it wants and passes that expectation down. For most expression forms, checking falls through to synthesis plus a subtype assertion. But for specific forms, the expected type changes the result — this is called **contextual typing**.

### When modes switch

| Site | What happens |
|---|---|
| `let x: Foo = <init>` | Annotation provides expected type → check `init` against `Foo` (top-down) |
| `let x = <init>` (no annotation) | Synthesize the type of `init`, then widen fresh literals (bottom-up) |
| Function call arguments | If param type is fully concrete → check arg against it. If param has unresolved type vars → synthesize |
| `return <expr>` | If declared return type exists → check `expr` against it |
| Array literal where expected = `T[]` | Each element is checked against `T` |
| Map literal where expected = `map<K,V>` | Each key checked against `K`, each value against `V` |
| Object literal where expected = `SomeClass` | Expression gets `SomeClass` type directly; field values use synthesis |

**Concrete example:** `let x: Foo = { field: 42 }`

1. The `let` statement sees an annotation → expected type is `Foo`.
2. The initializer is checked against `Foo` (top-down).
3. The object literal matches in checking mode → typed as `Foo` directly.
4. The integer `42` inside the field is *synthesized* bottom-up → starts as `Literal(42, Fresh)`.

### Narrowing

TypeScript-style control-flow narrowing. The type checker recognizes patterns like `x != null`, `x == null`, `!expr`, and truthiness on nullable types.

For `if (x != null) { ... } else { ... }`:
1. In the then-branch, `x` is narrowed to remove `null`.
2. In the else-branch, `x` is narrowed to `null`.
3. After the if-expression, the original type is restored.

**Guard clause pattern:** After `if (x == null) { return; }`, the then-branch diverges (type `never`). The type checker permanently applies the else narrowing for the rest of the block — so `x` is non-nullable from that point forward.

### TypeScript features present vs absent

**Present:** fresh/regular literal types, `never` as bottom, `unknown` as top, structural typing, union types, `void`, equirecursive recursive types, control-flow narrowing, bidirectional checking.

**Absent:** intersection types, conditional types (`T extends U ? A : B`), mapped types, `infer` keyword, discriminated union contextual decomposition (checking against a union doesn't pick a member to check against — it synthesizes and subtype-checks).

---

## Unions in Type Checking

### Representation

Unions are represented as `Ty::Union(Vec<Ty>)` — a plain vector with no deduplication or sorting at construction.

`Ty::Optional(Box<Ty>)` is a **separate variant** from `Union`. They are not auto-rewritten into each other. The relationship is defined only at the subtype level.

### Subtype rules

Both types are first normalized (all aliases expanded), then structural subtyping runs:

- **`T <: Union(A, B, ...)`** (the "right union" rule): A type is a subtype of a union if it's a subtype of **any** member.
- **`Union(T1, T2) <: U`** (the "left union" rule): A union is a subtype of something if **all** members are subtypes of it.
- **`Optional(T) <: Union(types)`**: Requires `null` to be in the union AND `T` to be a subtype of some member.
- Other rules: `null <: Optional(T)`, `T <: Optional(T)`, `never` is bottom, `unknown` is top, `int <: float`, enum variants are subtypes of their enum, list/map are covariant, functions are contravariant in parameters.

### Unions are never simplified automatically

When combining branch types (e.g., if/else), the type checker does flat deduplication only. No simplification of `Union(T, never)`, no removal of subtypes (e.g., `Union(int, float)` stays as-is). Normalization happens on-demand at subtype-check time and does not write back.

### Match exhaustiveness with unions

When type-checking a `match` expression:

1. The type checker computes the set of **required cases** from the scrutinee type: booleans require `true`/`false`, enums require all variants, optionals require the inner type's cases plus `null`, unions require the union of all members' required cases.
2. Each arm covers some cases. After all arms, the uncovered set is computed.
3. Non-empty uncovered set → `NonExhaustiveMatch` error. Full coverage → the match is marked as exhaustive.
4. Per-arm narrowing: inside each arm body, the scrutinee variable is temporarily set to the narrowed type.

---

## Recursive Types

### The problem

A user writes `type JSON = string | int | bool | null | JSON[] | map<string, JSON>`. The type's body references itself. The compiler must detect the cycle, decide if it's valid, and perform subtype checking without infinite loops.

### How it works

**At HIR time**, type aliases store raw name references. `type JSON = ... | JSON[]` stores a `TypeExpr` with a path reference to `"JSON"`. No attempt to resolve or detect cycles.

**At TIR time**, the path reference becomes an opaque `Ty::TypeAlias` — never automatically expanded. The alias body still references itself via this opaque handle.

### Cycle detection: structural vs non-structural edges

The type checker runs two passes:

**Pass 1 — Which aliases are recursive?** A DFS walks through the alias map, following all type constructors. Any alias found in a cycle is marked recursive.

**Pass 2 — Which cycles are valid?** The dependency graph is analyzed where edges are classified as **structural** (through `List` or `Map`) or **non-structural** (through `Optional`, `Union`, or direct reference). For each strongly connected component, if any intra-SCC edge is structural, the cycle is valid. If no structural edges exist, the cycle is invalid.

The intuition: `List` and `Map` provide a construction base case (an empty container). `Optional` does not — `type A = A?` expands to `A | null`, and `A` still needs to be constructed.

| Definition | Valid? | Why |
|---|---|---|
| `type A = A` | Invalid | Direct self-reference, no structural edge |
| `type A = A?` | Invalid | Optional is not structural |
| `type A = A \| string` | Invalid | Union is not structural |
| `type A = A[]` | Valid | Goes through List (structural) |
| `type JSON = string \| int \| JSON[] \| map<string, JSON>` | Valid | Both back-edges go through List and Map |
| `type A = B[], type B = A` | Valid | `A→B` goes through List (structural) |
| `type A = B?, type B = A` | Invalid | `A→B` goes through Optional (not structural) |

Class cycles use the same approach: a dependency edge is added when a field is **not** behind Optional/List/Map. Any SCC found is unconditionally invalid.

### Mu types and equirecursive subtyping

When subtype checking encounters a recursive alias, the normalizer produces a **mu type**: `Mu { var: "JSON", body: Union([String, Int, ..., List(TyVar("JSON"))]) }`. This is the standard type-theory mu-binder — "the type where `var` in `body` stands for this whole type."

Subtype checking uses **equirecursive co-induction**: before recursing into a pair `(sub, sup)`, the pair is inserted into an assumptions set. If the same pair is encountered again during recursive checking, it returns `true` immediately (the co-inductive assumption). If the overall check succeeds, the assumption is validated. Mu types are unfolded by substituting every `TyVar(var)` with the full Mu type, then continuing the check.

**Why equirecursive (not isorecursive)?** In isorecursive typing, `mu X.T` and its unfolding are different types requiring explicit fold/unfold coercions. Since BAML users write types naturally and expect transparent alias expansion, equirecursive is the practical choice.

---

## Salsa Early Cutoff: How Edits Stay Local

The Salsa query model has one critical optimization beyond basic memoization: **early cutoff**. When a tracked query re-runs but produces the same result as before, Salsa stops propagating invalidation to downstream dependents.

### How it works in practice

The item tree is produced by a single coarse query, `file_semantic_index`, marked `no_eq` — it always reports "changed", so its spans are always fresh but it provides no cutoff itself. In front of it sit fine-grained **firewall queries** (in `baml_compiler2_ppir::item_data`), one family per item kind:

- *Enumeration* — `file_classes(file)` / `file_functions(file)` / … return a `Vec` of interned `*Loc` handles (a `ClassLoc` carries its own file plus a position-independent `LocalItemId`).
- *Lookup* — `class_data(ClassLoc)` / `function_data(FunctionLoc)` / … return **span-free** semantic data. Type references inside them are ids into a per-item `TypeRef` arena — a flat, span-free replacement for `ast::TypeExpr` — never `TextRange`s.
- *Spans* — `class_source_map(ClassLoc)` / `function_source_map(FunctionLoc)` / … are separate queries holding the spans, keyed by the same `*Loc`. The type checker reads the data query; only diagnostics read the source-map query.

Because the `*_data` queries are tracked and `PartialEq`-compared, a whitespace or comment edit that shifts spans but not semantics leaves them **byte-identical** → `PartialEq` returns `true` → anything depending on a `*_data` query cuts off. **This is why the data must be span-free** (see [Rules for Adding Spans](#rules-for-adding-spans-to-data-structures)): a span inside a `PartialEq`-compared result would either destroy the cutoff or, if `PartialEq` ignored it, serve a *stale* span forever.

Items are keyed by **position-independent IDs** — `LocalItemId` is a hash of the item's name plus a collision index, not its position in the file. Adding a blank line before `function Greet(...)` doesn't change the hash of `"Greet"`, so the `FunctionLoc` key stays the same and cached results survive.

### Concrete trace: adding a comment to a file

User adds `// comment` to `file_a.baml`. File B is untouched.

1. `file_a.text` is marked changed.
2. `file_semantic_index(file_a)` re-runs (`no_eq` — always "changed"); its spans are refreshed.
3. Each per-item firewall query for `file_a` (`class_data`, `function_data`, …) re-runs but **early-cuts**: the semantic data is identical, so its `PartialEq` returns `true`. Its `*_source_map` twin re-runs and *does* change (new spans) — correctly, since positions moved.
4. `namespace_items(user_root)` re-runs and early-cuts when the name set is unchanged. (Exception: a file already holding a *duplicate-name* conflict currently carries a `name_span` in the conflict record, so a cosmetic edit there loses cutoff — a known gap being closed.)
5. `file_semantic_index(file_b)` — NOT re-run (its input `file_b.text` is unchanged), so nothing about file B recomputes.

**Status / caveat.** The **item layer** is fully behind the firewall: every consumer (TIR, MIR, emit, LSP, project, CLI, tests) reads items through the `item_data` queries, and the raw doors (`file_item_tree` in both HIR and PPIR) are `pub(crate)`. The remaining red edge is the **scope tree**: `infer_scope_types` (and the LSP scope walkers) still read the coarse `no_eq` `file_semantic_index` directly for scopes/bindings, and therefore re-run on *any* edit to their file today. Realizing end-to-end cutoff (a comment edit not re-running type inference) requires fronting the scope tree with the same kind of fine-grained queries (`scope_owner`/`function_scope` exist; the per-scope data queries do not yet). The incremental tests in `baml_tests` pin what actually holds today, and one of them (`comment_edit_does_not_reexecute_type_inference`) is deliberately `#[ignore]`d precisely because inference still reads the coarse index.

---

## The Standard Library: Dual Pipeline

The standard library (`baml_std`) uses two separate paths: one for the compiler and one for runtime. Understanding both is important because they share source files but consume them differently.

### Compiler path

The `.baml` stub files in `baml_builtins2/baml_std/baml/` are embedded at compile time via `include_str!`. They are injected into the compiler as a Salsa input (`Compiler2ExtraFiles`), separate from the `Project` input that carries user files. The HIR query `compiler2_all_files` unions user files with builtin files. From that point on, builtin functions are type-checked, lowered, and compiled exactly like user-written functions — no special-casing.

### Runtime path

At Rust build time (`build.rs`), the same `.baml` stub files are lexed, parsed, and lowered to AST. Every function with a `$rust_function` or `$rust_io_function` body is collected into a record. From these records, three things are generated:

- **Trait hierarchies** — One trait per class/namespace (e.g., `BamlClassArray` with a method per array builtin). These mirror the namespace structure.
- **A `SysOp` enum** — One variant per I/O builtin, used for async dispatch.
- **I/O traits** — For builtins that do async I/O.

A concrete struct (`PackageBamlImpl`) implements all generated traits. At program load time, the VM walks all functions in the compiled program. For each `NativeUnresolved` function, it calls `get_native_fn(name)` to look up the Rust function pointer. At call time, the VM invokes the function pointer directly.

### Why this matters

When you add a new builtin function to the standard library, you are touching both paths. The `.baml` file defines the signature and body marker. The compiler path type-checks it. The `build.rs` codegen path generates a trait method for it. And you must implement that trait method in Rust. The two paths share the same source of truth (the `.baml` files) but consume it independently.

---

## Testing Infrastructure: Phases and Incrementality

### Snapshot test phases

The test infrastructure generates one snapshot per pipeline phase per test project. Each phase captures a different layer's output:

| Phase | Name | What it snapshots |
|---|---|---|
| `01` | lexer | Token stream |
| `02` | parser | CST + parse errors |
| `03` | hir | Scope tree, item tree, symbol contributions |
| `04` | tir | Typed expressions, resolved names |
| `04_5` | mir | Control flow graphs |
| `05` | diagnostics | All diagnostics aggregated across phases |
| `06` | codegen | Bytecode |
| `10` | formatter | Formatter idempotency (format twice, assert identical) |

Phases 01 and 02 run per-file. Phases 03–06 run per-project (loading all files together). Snapshots are stored alongside the test projects.

### Adding a test case

1. Create a directory with `.baml` files in the test projects area.
2. Run `cargo test` — the build script picks up new directories automatically.
3. Run `cargo insta accept --all` to commit initial snapshots.

### Incremental tests

Separate from snapshot tests, there are targeted incremental tests that verify Salsa's early-cutoff behavior. These wrap the project database with an event log that records `WillExecute` events, then assert exact execution counts. They verify things like:

- A body edit forces re-lex but not cross-file invalidation.
- A rename forces item tree rebuild.
- A comment change re-runs the lexer then stops.
- Editing one file doesn't affect another file's cached queries.
- Repeated identical queries hit zero re-executions.

---

## Decision Framework Summary

When implementing a new feature, walk through these questions in order:

1. **Does it change the grammar?** → Parser (lexer/CST).
2. **Does it introduce a new syntactic form that desugars to existing constructs?** → AST layer.
3. **Does it need to know the name of something?** → It needs HIR, but the *implementation* might still live in the AST with the HIR providing the answer via queries.
4. **Does it need to know the type of something?** → TIR.
5. **Does it need to expand types before type-checking (e.g., stream types)?** → PPIR.
6. **Does it change the control flow representation?** → MIR (with strong justification).
7. **Does it change bytecode emission?** → Emit (very rare).

**When in doubt:** put it in the AST layer. Most features live there. The AST is the workhorse of the compiler.

**When talking to coding agents:** Tell the agent which layer to operate in. This dramatically improves one-shot accuracy. Agents that understand the layer boundaries produce correct code more reliably than agents given free rein to modify any layer.

---

## Quick Reference: Layer Properties

| Layer | Crate | Transforms? | Salsa Queries? | Can construct new nodes? |
|---|---|---|---|---|
| Parser/CST | `baml_compiler_parser` | Yes (text → CST) | `syntax_tree` | Yes |
| AST | `baml_compiler2_ast` | Yes (CST → AST) | No (pure function) | Yes |
| HIR | `baml_compiler2_hir` | No | `file_semantic_index`, `namespace_items`, `package_items` | No |
| PPIR | `baml_compiler2_ppir` | Yes (synthesizes stream types, feeds back to HIR) | `ppir_expansion_items` | Yes (synthetic stream items only) |
| TIR | `baml_compiler2_tir` | No | `infer_scope_types`, `resolve_name_at` | No |
| MIR | `baml_compiler2_mir` | Yes (AST → CFG) | `lower_function`, `lower_let_body` | Yes |
| Emit | `baml_compiler2_emit` | Yes (MIR → bytecode) | `generate_project_bytecode` | Yes (bytecode) |
