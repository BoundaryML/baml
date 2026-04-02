# Compiler2 Architecture

This document covers the compiler2 pipeline end-to-end: the stages a `.baml` file passes through from source text to executable bytecode, the Salsa incremental compilation model, the TypeScript-inspired bidirectional type checker, the package system (including `baml_std` as a native package), LLM function desugaring, scopes, recursive types, and testing infrastructure.

**Audience**: Team members who will read and modify the compiler2 code. Assumes familiarity with Rust and basic compiler concepts. Includes concrete file paths and code references throughout.

**How to use**: Read linearly for a full walkthrough, or jump to a specific section via the table of contents. The [Pipeline Overview](#pipeline-overview) gives the 30-second picture; the [Layer Reference](#layer-reference) goes deep on each stage; the remaining sections cover cross-cutting concerns.

---

## Table of Contents

1. [Pipeline Overview](#pipeline-overview)
2. [Layer Reference](#layer-reference)
   - [Parser](#layer-0-parser)
   - [AST](#layer-1-ast)
   - [HIR](#layer-2-hir)
   - [PPIR](#layer-3-ppir)
   - [TIR](#layer-4-tir)
   - [MIR](#layer-5-mir)
   - [Emit](#layer-6-emit)
3. [Salsa: Incremental Compilation](#salsa-incremental-compilation)
4. [The Bidirectional Type Checker](#the-bidirectional-type-checker)
5. [Unions in Type Checking](#unions-in-type-checking)
6. [Recursive Types](#recursive-types)
7. [LLM Function Desugaring](#llm-function-desugaring)
8. [The Scope System](#the-scope-system)
9. [The `baml_std` Builtins Pipeline](#the-baml_std-builtins-pipeline)
10. [Testing Infrastructure](#testing-infrastructure)
11. [Code References](#code-references)

---

## Pipeline Overview

```
Source (.baml file)
    | baml_compiler_parser::syntax_tree  [Salsa tracked]
    v
CST (Concrete Syntax Tree -- lossless, with trivia)
    | baml_compiler2_ast::lower_file()  [no Salsa; owned data]
    |   |-- LLM functions -> synthesize_llm_builtin_call
    |   |-- Companion functions -> expand_companions
    |   \-- Client blocks -> synthesize_client_items
    v
Vec<ast::Item> (span-separated AST: ExprBody + AstSourceMap)
    | SemanticIndexBuilder::build()  [builds scope tree, item tree]
    v
FileSemanticIndex (HIR per file)
    | namespace_items() -> package_items()  [Salsa, with PartialEq early cutoff]
    v
Merged package symbol tables
    | baml_compiler2_ppir  [stream expansion, synthesis of stream_* items]
    v
Augmented HIR
    | infer_scope_types() per scope  [Salsa tracked, per-scope granularity]
    v
ScopeInference (TIR -- fully typed expressions, resolved names)
    | lower_function() / lower_let_body()
    v
MirFunction (CFG of BasicBlocks)
    | cleanup -> analysis -> stack-carry -> emit
    v
bex_vm_types::Program (stack-machine bytecode)
```

Each arrow is a dependency edge. Changes propagate only as far as Salsa's early-cutoff allows.

### Layer Summary

| Layer | Question it answers | Salsa? | Key output |
|---|---|---|---|
| **Parser** | What tokens/syntax? | Yes (1 query) | `SyntaxNode` (CST) |
| **AST** | What items after desugaring? | No | `Vec<Item>` |
| **HIR** | What scopes/names/contributions? | Yes (8 queries) | `FileSemanticIndex`, `PackageItems` |
| **PPIR** | What with stream types expanded? | No (passthrough) | Same as HIR (will diverge) |
| **TIR** | What type is everything? | Yes (6 queries) | `ScopeInference`, `PackageInterface` |
| **MIR** | What is the control flow? | No | `MirFunction` |
| **Emit** | What is the bytecode? | No | `Program` |

The Salsa boundary is at HIR and TIR -- these are the layers with incremental caching. AST, MIR, and Emit are pure transformations that re-run from scratch when their inputs change. The critical early-cutoff points are `namespace_items` (prevents file-edit cascades across files) and `infer_scope_types` (prevents body-edit cascades across scopes).

---

## Layer Reference

### Layer 0: Parser

**Crate**: `baml_compiler_parser`
**Question**: *"What are the tokens and syntactic structure of this file?"*

Produces a lossless CST (Concrete Syntax Tree) including whitespace and comments. Supports incremental reparsing -- on a text edit, only the changed portion of the tree is rebuilt.

| Query | Signature | Description |
|---|---|---|
| `syntax_tree` | `#[salsa::tracked] fn(db, SourceFile) -> Parse` | Returns the full CST parse result for a file |

**Output**: `SyntaxNode` -- a green/red tree (rowan) where every token and trivia node is preserved. Downstream layers never re-lex or re-parse; they consume this tree.

**When to modify**: You're adding new syntax to the language (new keywords, new expression forms, new block types). If you're changing what a construct *means* rather than how it's *written*, you probably want AST or HIR instead.

---

### Layer 1: AST

**Crate**: `baml_compiler2_ast`
**Question**: *"What items did the user declare, and what do their expression bodies look like after desugaring?"*

Converts the CST into clean, owned Rust data structures. Performs all syntactic desugaring: LLM functions become calls to `baml.llm.call_llm_function`, client blocks become let bindings, companion functions are generated. After this layer, there is no distinction between "declarative" and "imperative" function bodies -- everything is an expression tree.

| Function | Signature | Description |
|---|---|---|
| `lower_file` | `fn(root: &SyntaxNode) -> (Vec<Item>, Vec<Diagnostic>)` | The sole entry point; not a Salsa query (pure function) |

**Output**: `Vec<Item>` -- a flat list of `FunctionDef`, `ClassDef`, `EnumDef`, `TypeAliasDef`, `LetDef`, etc. Each function body is an `ExprBody` (arena of `Expr`/`Stmt`/`Pattern` nodes) paired with a separate `AstSourceMap` (arena of `TextRange`s). The span/semantic separation is physical from the start.

**Key types consumed downstream**: `Item`, `ExprBody`, `AstSourceMap`, `Expr` (15 variants), `Stmt` (12 variants), `TypeExpr` (recursive type syntax), `FunctionDef`, `ClassDef`, `DeclarativeMeta::Llm`.

**When to modify**: You're changing how a syntactic construct desugars into the core expression language. Examples: adding a new desugaring (like LLM functions -> builtin calls), adding a new `Expr` or `Stmt` variant, changing what companion functions are generated, or changing how client/retry blocks are lowered. If the new feature doesn't need new syntax but needs new desugaring, this is likely your layer. If you need to change name resolution or type checking, look at HIR or TIR instead.

---

### Layer 2: HIR

**Crate**: `baml_compiler2_hir`
**Question**: *"What scopes exist, what names are visible in each scope, and what items does each file/namespace/package contribute?"*

Builds the semantic index for each file -- the scope tree, item tree, and symbol contributions. Merges contributions across files into namespace-level and package-level symbol tables. This is where Salsa enters the picture: every query here is either `#[salsa::tracked]` or carefully designed for early cutoff.

**Db trait**: `pub trait Db: baml_workspace::Db` -- adds `compiler2_extra_files()` for builtin stubs.

| Query | Key | Returns | Description |
|---|---|---|---|
| `file_semantic_index` | `SourceFile` | `&FileSemanticIndex` | Salsa tracked (`no_eq`); builds scope tree + item tree from AST |
| `namespace_items` | `NamespaceId` | `&NamespaceItems` | Salsa tracked; merges all files in a namespace; `PartialEq` early cutoff |
| `package_items` | `PackageId` | `&PackageItems` | Salsa tracked; aggregates all namespaces in a package |
| `package_dependencies` | `PackageId` | `Vec<PackageId>` | Salsa tracked; returns dependency list (currently hardcoded) |
| `function_signature` | `FunctionLoc` | `Arc<FunctionSignature>` | Salsa tracked; span-free param names + `TypeExpr`s |
| `function_body` | `FunctionLoc` | `Arc<FunctionBody>` | Salsa tracked; span-free `ExprBody` or `Builtin(Vm|Io)` |
| `function_signature_source_map` | `FunctionLoc` | `SignatureSourceMap` | Salsa tracked; spans only -- decoupled from semantic data |
| `function_body_source_map` | `FunctionLoc` | `Option<AstSourceMap>` | Salsa tracked; spans only |
| `let_body` | `LetLoc` | `Arc<LetBody>` | Salsa tracked; span-free let initializer body |
| `scope_bindings_query` | `ScopeId` | `ScopeBindings` | Plain function; returns bindings for a scope |
| `file_item_tree` | `SourceFile` | `Arc<ItemTree>` | Plain function; extracts item tree from semantic index |
| `file_symbol_contributions` | `SourceFile` | `Arc<FileSymbolContributions>` | Plain function; extracts contributions from semantic index |

**Output**: `FileSemanticIndex` containing: `scopes` (DFS pre-order scope tree), `scope_bindings` (per-scope let/param bindings), `item_tree` (position-independent item storage keyed by name-hash `LocalItemId`), `symbol_contributions` (names this file contributes to its namespace). `PackageItems` provides `lookup_type(path)` and `lookup_value(path)` for downstream name resolution.

**When to modify**: You're changing how names are organized, resolved, or contributed across files and packages. Examples: adding a new item kind (so it appears in the `ItemTree`), changing how scopes are built (e.g. adding a new `ScopeKind`), modifying cross-file or cross-package name resolution, adding a new Salsa query for a new per-item data extraction, or changing what constitutes a "symbol contribution" to a namespace. Also modify HIR if you need to change how `Compiler2ExtraFiles` (builtins) are injected. If your change is about *what type* something has rather than *what name* it has, look at TIR.

---

### Layer 3: PPIR

**Crate**: `baml_compiler2_ppir`
**Question**: *"What does the program look like with streaming types expanded?"*

Sits between HIR and TIR. Performs stream expansion: for each class/type-alias with `@stream.*` annotations, synthesizes `stream_*` variants with SAP (Streaming Appearance Protocol) attributes. Currently implemented as a passthrough scaffold -- all queries delegate to HIR.

**Db trait**: `pub trait Db: baml_compiler2_hir::Db`

TIR calls `baml_compiler2_ppir::package_items` (not HIR's directly), so when PPIR adds stream-expansion logic, TIR picks it up automatically.

**When to modify**: You're working on streaming support. This layer is currently a passthrough scaffold, but it's where stream-type expansion (`@stream.*` annotations -> synthetic `stream_*` types) will live. If you're adding or changing streaming behavior, this is your layer.

---

### Layer 4: TIR

**Crate**: `baml_compiler2_tir`
**Question**: *"What is the type of every expression, and does the program type-check?"*

Runs bidirectional type inference per scope. Resolves all name references to fully qualified types. Checks subtyping, validates generics, performs control-flow narrowing, infers throw sets, detects recursive type cycles, and exports fully-resolved package interfaces for cross-package consumption.

**Db trait**: `pub trait Db: baml_compiler2_ppir::Db`

| Query | Key | Returns | Description |
|---|---|---|---|
| `infer_scope_types` | `ScopeId` | `&ScopeInference` | **Core query** -- maps `ExprId -> Ty`, `PatId -> Ty`, resolves method calls, tracks match exhaustiveness |
| `resolve_class_fields` | `ClassLoc` | `Arc<ResolvedClassFields>` | Resolves field `TypeExpr -> Ty` for one class |
| `resolve_type_alias` | `TypeAliasLoc` | `Arc<ResolvedTypeAlias>` | Resolves the body of one type alias |
| `package_interface` | `PackageId` | `&PackageInterface` | Fully-resolved typed API surface of a package (types, functions, throw sets) |
| `package_resolution_context` | `PackageId` | `&PackageResolutionContext` | Bundles own items + dependency interfaces for cross-package lookups |
| `function_throw_sets` | `PackageId` | `&FunctionThrowSets` | Per-function direct and transitive throw sets via call-graph analysis |
| `collect_file_diagnostics` | `SourceFile` | `TypeCheckDiagnostics` | Gathers all TIR diagnostics for one file |

**Output**: `ScopeInference` -- per-scope maps of `ExprId -> Ty` (expression types), `PatId -> Ty` (binding types), `ExprId -> MethodResolution` (resolved call targets), and an exhaustive-match set. `PackageInterface` -- the fully-resolved typed export surface consumed by dependent packages.

**When to modify**: You're changing type-checking rules, adding new type constructs, or modifying how expressions get their types. Examples: adding a new `Ty` variant, changing subtype rules (e.g. making a new type coercible to another), adding new control-flow narrowing patterns, changing how generics are instantiated, adding new diagnostic errors for type mismatches, changing how throw sets are computed, or modifying the `PackageInterface` for cross-package type resolution. If your change is about *what code gets generated* rather than *what types things have*, look at MIR or Emit.

---

### Layer 5: MIR

**Crate**: `baml_compiler2_mir`
**Question**: *"What is the control flow graph for each function, with typed locals and explicit branching?"*

Lowers the AST expression tree (with TIR type annotations) into a control flow graph of basic blocks. Each block contains typed statements and ends with a terminator (goto, branch, switch, call, return, throw, dispatch-future, await). Runs four cleanup passes: dead block elimination, copy propagation, dead local elimination, RPO reordering.

**Db trait**: `pub trait Db: baml_compiler2_tir::Db`

All plain functions (no Salsa tracking):

| Function | Args | Returns | Description |
|---|---|---|---|
| `lower_function` | `db, FunctionLoc` | `MirFunction` | Lowers one function into a CFG with typed locals |
| `lower_let_body` | `db, LetLoc` | `Option<MirFunctionBody>` | Lowers a top-level let initializer into a CFG body |
| `convert_tir2_ty` | `ty, aliases, recursive_aliases` | `Ty` | Converts TIR `Ty` to MIR's `baml_type::Ty`, resolving aliases with recursion guard |
| `def_to_item_ref` | `db, Definition` | `ItemRef` | Converts an HIR `Definition` to a fully-qualified `ItemRef` |

**Output**: `MirFunction` -- either `Bytecode(MirFunctionBody)` or `Builtin(BuiltinKind)`. A `MirFunctionBody` contains `blocks: Vec<BasicBlock>`, `locals: Vec<LocalDecl>` (where `_0` = return place, `_1..=_arity` = parameters), and `entry: BlockId`.

**When to modify**: You're adding new control flow constructs, changing how expressions lower to basic blocks, or adding new MIR optimization passes. Examples: adding a new `Terminator` variant (e.g. a new kind of branch/call), adding a new cleanup pass, changing how pattern matching compiles to switches, or adding a new `Statement` kind. If you're adding a new expression form, you likely need changes in both AST (to parse it) and MIR (to lower it to a CFG). If the change is about how the CFG maps to bytecode instructions, look at Emit instead.

---

### Layer 6: Emit

**Crate**: `baml_compiler2_emit`
**Question**: *"What is the complete executable bytecode program?"*

Compiles MIR into stack-machine bytecode. Runs local classification analysis (parameter/real/virtual/phi-like/dead), register allocation via stack-carry simulation, and emits `bex_vm_types::Program`.

| Function | Args | Returns | Description |
|---|---|---|---|
| `generate_project_bytecode` | `db, options` | `Result<Program, LoweringError>` | Multi-pass driver; assigns global slots, builds class/enum tables, compiles all functions, synthesizes `$init` per package |

**Output**: `bex_vm_types::Program` -- the complete executable program containing: function table (bytecode + metadata), class table (field layouts + type tags), enum table (variant maps), global slot assignments, and test case metadata.

**When to modify**: You're changing how MIR maps to bytecode, adding new VM instructions, or changing the structure of the compiled `Program`. Examples: adding a new bytecode instruction, changing register allocation strategy, modifying how global slots are assigned, changing the class/enum table layout, or adding new metadata to compiled functions (like the `FunctionMeta::Llm` sidecar). If you're changing the VM instruction set itself (what instructions exist and what they do), you're likely changing both Emit and `bex_vm_types`.

---

## Salsa: Incremental Compilation

Salsa is the foundation of the pipeline. Understanding it is necessary to understand everything else.

### The core idea

You declare **inputs** (mutable data the outside world provides) and **tracked queries** (pure functions of the database). Salsa records which queries read which inputs. On the next compilation cycle, it only re-runs queries whose inputs actually changed -- and only propagates invalidation further if the recomputed result is actually *different* from the cached one ("early cutoff").

### Inputs

There are two primary inputs:

- **`SourceFile`** (`baml_base/src/files.rs:14`): Each `.baml` file. Has `text: String`, `path: PathBuf`, `file_id: FileId`. When the user edits a file, the LSP calls `file.set_text(db).to(new_text)`.
- **`Project`** (`baml_workspace/src/lib.rs:50`): The list of all `SourceFile`s. Changed when files are added/removed.

### The `Db` trait chain

Each pipeline crate defines a `Db` trait extending the previous:

```
baml_workspace::Db              <-- provides project() -> Project
  \-- baml_compiler2_hir::Db      <-- adds compiler2_extra_files()
        \-- baml_compiler2_ppir::Db   <-- pure passthrough (no additions)
              \-- baml_compiler2_tir::Db    <-- no additional methods
```

A single concrete `ProjectDatabase` struct implements all four. When a TIR query calls an HIR query, it passes the same `db` reference. Salsa sees one continuous dependency graph -- no manual cache wiring between crate boundaries.

### Tracked queries

A `#[salsa::tracked]` function is Salsa's memoization unit. The first call executes the body and records all `db.` reads as dependencies. Subsequent calls check: are all dependencies up-to-date? If yes, return cached. If not, re-execute and compare with the old result.

Key tracked queries:

| Query | Key | What it does |
|---|---|---|
| `file_semantic_index` | `SourceFile` | Builds scope tree + item tree from AST |
| `namespace_items` | `NamespaceId` | Merges contributions from all files in a namespace |
| `package_items` | `PackageId` | Aggregates all namespaces in a package |
| `function_signature` | `FunctionLoc` | Extracts signature (span-free) |
| `function_body` | `FunctionLoc` | Extracts body (span-free) |
| `infer_scope_types` | `ScopeId` | Runs type inference for one scope |
| `resolve_class_fields` | `ClassLoc` | Resolves field types for one class |

### Early cutoff: how whitespace edits don't cascade

The key optimization. When a tracked query re-runs but produces the same result, Salsa stops propagating invalidation.

**Concrete trace**: User adds `// comment` to `file_a.baml`. File B is untouched.

1. `file_a.text` is marked changed
2. `file_semantic_index(file_a)` re-runs (it uses `no_eq`, so always reports "changed")
3. `namespace_items(user_root)` re-runs -- re-collects contributions from all files. But the result is identical: same names, same definition handles. Its `PartialEq` returns `true`. **Early cutoff fires.** (`namespace.rs:130-146`)
4. `package_items` -- NOT re-run (its dependency `namespace_items` didn't change)
5. `infer_scope_types` for any scope -- NOT re-run
6. `file_semantic_index(file_b)` -- NOT re-run (its input `file_b.text` is unchanged)

Result: a comment addition re-runs the lexer and HIR for that one file, then stops.

### The span/semantic split

Every item has **two** tracked queries: one for semantic data (span-free), one for source maps (spans only).

- `function_signature(db, loc)` -> `Arc<FunctionSignature>` -- names and `TypeExpr`s, no `TextRange`
- `function_signature_source_map(db, loc)` -> spans only

`infer_scope_types` reads `function_signature` but never reads `function_signature_source_map`. A whitespace edit shifts spans but leaves `TypeExpr` trees identical -> `function_signature` early-cuts -> type inference stays cached. (`signature.rs:93-113`)

### Position-independent IDs

`LocalItemId<T>` (`ids.rs:43-76`) is a 32-bit packed value: upper 16 bits = hash of the item's name, lower 16 bits = collision index. Adding a blank line before `function Greet(...)` doesn't change `hash("Greet")`. The `FunctionLoc(file, hash_id)` -- the Salsa query key -- stays the same, so cached per-function results survive.

### Interned types

`#[salsa::interned]` types are deduplicated stable identity keys. `FunctionLoc::new(db, file, id)` with the same arguments always returns the same handle. Nine `*Loc` types exist (`loc.rs:19-77`): `FunctionLoc`, `ClassLoc`, `EnumLoc`, `TypeAliasLoc`, `LetLoc`, `ClientLoc`, `TestLoc`, `GeneratorLoc`, `RetryPolicyLoc`.

---

## The Bidirectional Type Checker

The TIR type checker implements **bidirectional type checking** -- it switches between two modes at well-defined boundaries.

### Two modes

**Synthesis (bottom-up)**: `infer_expr(expr_id, body) -> Ty` (`builder.rs:227`). No expectation from the caller. The type is computed purely from the expression's structure. Used for: literals, variable references, field access, untyped calls.

**Checking (top-down)**: `check_expr(expr_id, body, expected: &Ty) -> Ty` (`builder.rs:391`). The caller knows what type it wants and passes that expectation down. For most expression forms, checking falls through to synthesis + a subtype assertion. But for specific forms, the expected type changes the result (contextual typing).

### When modes switch

| Site | What happens |
|---|---|
| `let x: Foo = <init>` | Annotation -> `check_expr(init, &Foo)` (top-down) |
| `let x = <init>` (no annotation) | `infer_expr(init)` then `widen_fresh().make_evolving()` (bottom-up) |
| Function call arguments | If param type is fully concrete -> `check_expr(arg, &param_ty)`. If param has unresolved TypeVars -> `infer_expr(arg)` |
| `return <expr>` | If declared return type exists -> `check_expr(expr, &return_ty)` |
| Array literal where `expected = List(E)` | Each element -> `check_expr(element, &E)` |
| Map literal where `expected = Map(K,V)` | Each key -> `check_expr(key, &K)`, each value -> `check_expr(val, &V)` |
| Object literal where `expected = Class(C)` | Expression gets `Class(C)` type directly; field values use synthesis |

**Concrete example**: `let x: Foo = { field: 42 }`
1. `Stmt::Let` fires at `builder.rs:695`
2. Annotation present -> `ann_ty = Ty::Class(Foo)` computed
3. `check_expr(init, &Ty::Class(Foo))` called
4. Object literal matches `Expr::Object` in checking mode -> expression typed as `Class(Foo)` directly
5. The integer literal `42` inside the field is *synthesized* bottom-up -> `Ty::Literal(42, Fresh)`

### Fresh literals and widening

Every literal starts as `Ty::Literal(value, Freshness::Fresh)` -- modeled after TypeScript's literal widening.

- `let x = 42` -> infer -> `Literal(42, Fresh)` -> `widen_fresh()` -> `Primitive(Int)` -> `make_evolving()` -> stored as `Int`
- `let x: 42 = 42` -> check against `Literal(42, Regular)` -> freshness stripped -> stays `Literal(42, Regular)`

`widen_fresh()` converts `Literal(_, Fresh)` -> `Primitive`. Only called at unannotated `let` sites. (`ty.rs:289`)

### `join_types` -- combining branch types

When an if-expression has both branches, `join_types(then_ty, else_ty)` produces the result type. If the types differ, it produces `Ty::Union(vec![then_ty, else_ty])` with flat deduplication (unions of unions are flattened, exact duplicates removed). `Never` is absorbed: `join(Never, T) = T`. (`builder.rs:3317-3356`)

### TypeScript features present vs absent

**Present**: fresh/regular literal types, `never` as bottom, `unknown` as top, structural typing, union types, `void`, equirecursive recursive types, control-flow narrowing, bidirectional checking

**Absent**: intersection types, conditional types (`T extends U ? A : B`), mapped types, `infer` keyword, discriminated union contextual decomposition (checking against a union doesn't pick a member to check against -- it just synthesizes and subtype-checks)

---

## Unions in Type Checking

### Representation

`Ty::Union(Vec<Ty>)` -- a plain vector, no deduplication or sorting at construction. (`ty.rs:109`)

`Ty::Optional(Box<Ty>)` is a **separate variant** from `Union`. They are not auto-rewritten into each other. The relationship is defined only at the subtype level.

### Subtype rules

Both types are first normalized to `StructuralTy` (all aliases expanded), then structural subtyping runs. (`normalize.rs:18`)

- **`T <: Union(A, B, ...)`** -- the "right union" rule (`normalize.rs:177-179`): A type is a subtype of a union if it's a subtype of **any** member.
- **`Union(T1, T2) <: U`** -- the "left union" rule (`normalize.rs:182-184`): A union is a subtype of something if **all** members are subtypes of it.
- **`Optional(T) <: Union(types)`** (`normalize.rs:171-174`): Requires `Null in types` AND `T <: some member`.
- Other: `Null <: Optional(T)`, `T <: Optional(T)`, `Never` is bottom, `BuiltinUnknown` is top, `Int <: Float`, `EnumVariant(E,V) <: Enum(E)`, list/map covariant, function contravariant in params.

### Unions are never simplified automatically

`join_types` does flat deduplication only. No simplification of `Union(T, Never)`, no removal of subtypes (e.g., `Union(int, float)` stays as-is). Normalization to `StructuralTy` happens on-demand at subtype-check time and does not write back.

### Match exhaustiveness with unions

`infer_match_expr` (`builder.rs:980`) tracks coverage:
1. `required_match_cases(scrutinee_ty)` computes the required set: `Bool -> {"true","false"}`, `Enum(E) -> {all variants}`, `Optional(T) -> required(T) + {"null"}`, `Union(members) -> union of all members' required cases`
2. Each arm's pattern covers some cases; after all arms, `missing = required - covered`
3. Non-empty `missing` -> `NonExhaustiveMatch` error. Full coverage -> inserted into `exhaustive_matches` set
4. Per-arm narrowing: the scrutinee variable is temporarily set to the narrowed type inside each arm body

---

## Recursive Types

### The problem

A user writes `type JSON = string | int | bool | null | JSON[] | map<string, JSON>`. The type's body references itself. The compiler must detect the cycle, decide if it's valid (has a base case) or invalid (infinite expansion), and perform subtype checking without infinite loops.

### HIR: raw name references

The HIR stores `TypeExpr` with raw name references. `type JSON = ... | JSON[]` stores `TypeExpr::List(TypeExpr::Path(["JSON"]))`. No attempt to resolve or detect cycles. (`item_tree.rs:86-90`)

### TIR: lazy expansion via `Ty::TypeAlias`

`lower_type_expr` converts `TypeExpr::Path(["JSON"])` -> `Ty::TypeAlias(QualifiedTypeName { pkg: "user", name: "JSON" })`. This is **opaque** -- never automatically expanded. The alias body still references itself via `Ty::TypeAlias`. (`lower_type_expr.rs:44`)

### Cycle detection: two passes

**Pass 1 -- Which aliases are recursive?** `find_recursive_aliases` (`normalize.rs:26`) runs DFS over the alias map, walking through all type constructors. Returns `HashSet<QualifiedTypeName>` of all aliases in any cycle.

**Pass 2 -- Which cycles are valid vs invalid?** `find_invalid_alias_cycles` (`normalize.rs:461`) builds a dependency graph where edges are classified as **structural** (through `List` or `Map`) vs **non-structural** (through `Optional`, `Union`, or direct). Runs Tarjan SCC. For each SCC, if any intra-SCC edge is structural -> valid. If no structural edges -> invalid.

The structural/non-structural distinction: `List` and `Map` provide a construction base case (empty container). `Optional` does not -- `type A = A?` expands to `A | null`, and `A` still needs to be constructed.

| Definition | Valid? | Why |
|---|---|---|
| `type A = A` | Invalid | Direct self-reference, no structural edge |
| `type A = A?` | Invalid | Optional is not structural |
| `type A = A \| string` | Invalid | Union is not structural |
| `type A = A[]` | Valid | Goes through List (structural) |
| `type JSON = string \| int \| JSON[] \| map<string, JSON>` | Valid | Both back-edges go through List and Map |
| `type A = B[], type B = A` | Valid | `A->B` goes through List (structural) |
| `type A = B?, type B = A` | Invalid | `A->B` goes through Optional (not structural) |

Class cycles (`find_invalid_class_cycles`, `normalize.rs:740`): uses the same Tarjan approach but only adds a dependency edge when a field is **not** behind Optional/List/Map. Any SCC found is unconditionally invalid.

### Mu types and equirecursive subtyping

When subtype checking encounters a recursive alias, `normalize_impl` (`normalize.rs:304`) produces:

```
StructuralTy::Mu { var: "user.JSON", body: Union([String, Int, ..., List(TyVar("user.JSON"))]) }
```

`Mu { var, body }` is the standard type-theory mu-binder: "the type where `var` in `body` stands for this whole type." `TyVar` is the back-reference inside the body.

**Subtype checking** (`normalize.rs:94`) uses **equirecursive co-induction**: before recursing, the current pair `(sub, sup)` is inserted into an `assumptions: HashSet`. If during recursive checking the same pair is encountered again, it returns `true` immediately (the co-inductive assumption). If the overall check succeeds, the assumption is validated.

`Mu` types are unfolded via `substitute(body, var, self)` at `normalize.rs:149-157` -- replacing every `TyVar(var)` with the full `Mu` type, then continuing the subtype check.

**Why equirecursive (not isorecursive)?** In isorecursive typing, `mu X.T` and its unfolding `T[X := mu X.T]` are different types requiring explicit fold/unfold coercions. Since BAML users write types naturally and expect transparent alias expansion, equirecursive is the practical choice.

---

## LLM Function Desugaring

### What the user writes

```baml
function ExtractUser(text: string) -> User {
    client "MyClient"
    prompt #"Extract a user from: {{ text }}"#
}
```

### What the compiler sees after desugaring

The CST->AST pass (`lower_cst.rs:99-167`) does three things:

**1. Detects the LLM body** (`lower_cst.rs:125`): `func.llm_body()` returns `Some(...)` when the body is `{ client ...; prompt ... }` (declarative) rather than `{ expr }` (imperative).

**2. Synthesizes a builtin call** (`lower_cst.rs:247-354`):
```
Expr::Call {
    callee: Path(["baml", "llm", "call_llm_function"]),
    args: [
        Path(["MyClient"]),              // client (resolves to let binding)
        Literal(String("ExtractUser")),  // function name
        Map { "text" => Path(["text"]) } // argument map
    ]
}
```

The prompt template and client name are stored in a sidecar: `DeclarativeMeta::Llm(LlmBodyDef { client, prompt, span })` on the `FunctionDef` (`ast.rs:467-512`).

**3. Generates companion functions** (`companions.rs:25-91`): Two companions for every LLM function:

- `ExtractUser$render_prompt(text: string) -> baml.llm.PromptAst` -- calls `baml.llm.render_prompt(...)`
- `ExtractUser$build_request(text: string) -> baml.http.Request` -- calls `baml.llm.build_request(...)`

Companions have identical parameter lists to the parent. They carry `declarative_meta: None` -- only the original has the `LlmBodyDef`.

### How they flow through the pipeline

**HIR**: All three functions are registered as ordinary `Item::Function` entries. They get separate entries in `ItemTree::functions`, separate `FunctionLoc`s, separate namespace contributions. (`builder.rs:350-381`)

**TIR**: Type-checked like any expression-body function. The generic `call_llm_function<T>` resolves `T` against the parent's declared return type. Companions type-check against their declared return types (`PromptAst`, `http.Request`).

**MIR**: Standard CFG per function. The client reference becomes `const fn user.MyClient` -- a reference to the client's let-binding global.

**Emit** (`lib.rs:353-363`): After bytecode compilation, only the original function (which has `DeclarativeMeta::Llm`) gets `body_meta = FunctionMeta::Llm { prompt_template, client }`. Companions get `body_meta: None`.

**Runtime**: `extract_llm_function_info` walks all functions with `FunctionMeta::Llm`, builds `LlmFunctionInfo { prompt_template, client_name, return_type }`. When `call_llm_function` executes, it calls `get_jinja_template(function_name)` to retrieve the template, then `client.execute(context)` to drive the retry/routing/HTTP loop.

### Client block desugaring

`client<llm> MyClient { provider openai; options { model gpt-4 } }` desugars into:
1. `Item::Let("MyClient", LetOrigin::Client)` -- initializer is a `Client { name, client_type, sub_clients, retry, counter }` object. (`lower_cst.rs:696-843`)
2. `Item::Function("MyClient$new")` -- zero-param function constructing `PrimitiveClient { provider, options }`. Called at runtime via `Client::get_constructor()`. (`lower_cst.rs:865-1064`)

---

## The Scope System

### Scope hierarchy

Scopes are built at HIR time by `SemanticIndexBuilder`. Each file gets a scope tree in DFS pre-order:

```
Project
  \-- Package ("user")
        \-- Namespace (one per namespace segment)
              \-- File ("test.baml")
                    |-- Function "greet" (has params + body bindings)
                    |     |-- MatchArm (pattern binding)
                    |     |-- CatchClause (clause binding)
                    |     |     \-- CatchArm (arm pattern binding)
                    |     \-- ...
                    |-- Class "User"
                    |     \-- Function "method" (child of Class scope)
                    |-- Enum "Status"
                    |-- TypeAlias "MyStr"
                    \-- Let "TOP_LEVEL_CONST"
```

**`ScopeKind`** (`scope.rs:52-83`): `Project`, `Package`, `Namespace`, `File`, `Class`, `Enum`, `Function`, `TypeAlias`, `Block`, `Lambda`, `Item`, `MatchArm`, `CatchClause`, `CatchArm`, `Let`. Note: `Block` and `Lambda` exist in the enum but are **not currently emitted** by the builder.

**`ScopeId`** (`scope.rs:44`): A `#[salsa::tracked]` struct pairing `SourceFile` + `FileScopeId`. This is the Salsa query key for `infer_scope_types`.

### What TIR does per scope

**`infer_scope_types(db, ScopeId) -> ScopeInference`** (`inference.rs:189`) dispatches on `ScopeKind`:

- **`Function`**: The primary case. Loads signature and body. Merges class generic params if this is a method inside a `Class` scope. Lowers parameter types, adds each as a local. Calls `builder.check_expr(root_expr, body, &return_ty)`. Runs `check_throws_contract`.
- **`Class`**: Empty -- fields are handled by `resolve_class_fields`; methods by their own `Function` scope invocations.
- **`Let`**: Loads let body, calls `builder.infer_expr(root_expr, body)`.
- **`Lambda`**: Placeholder (not yet emitted).
- **All others**: No-op.

**Key insight**: inference runs per scope, not per function. Editing a lambda body would only invalidate that scope's Salsa query, not the enclosing function's.

### The three local maps in `TypeInferenceBuilder`

Within one scope's inference run, the builder (`builder.rs:87`) maintains:

| Map | Purpose | Modified by narrowing? |
|---|---|---|
| `locals` | Flow-sensitive current type of each variable | Yes |
| `declared_types` | The annotated type, written once | No -- assignment checks validate against this |
| `bindings` | The initial binding type (output to ScopeInference) | No |

`add_local(name, ty)` (`builder.rs:212`) writes to both `locals` and `declared_types` (using `entry().or_insert_with()` so narrowing restore cycles don't overwrite the original).

### Narrowing

TypeScript-style control-flow narrowing (`narrowing.rs`). Recognizes: `x != null`, `x == null`, `!expr`, truthiness on nullable types.

For `if (x != null) { ... } else { ... }`:
1. `apply_then_narrowings`: saves originals, sets `locals["x"] = remove_null(ty)` (then-branch)
2. Infer then-branch
3. `restore_and_apply_else`: restores originals, sets `locals["x"] = Null` (else-branch)
4. Infer else-branch
5. `restore_narrowings`: restores originals

**Guard clause pattern** (`builder.rs:877-962`): After `if (x == null) { return; }`, the then-branch diverges (`Ty::Never`). `apply_post_diverge_narrowings` permanently writes `else_type` into `locals` -- so `x` is `int` (not `int?`) for the rest of the block.

### Name resolution across scopes

`resolve_name_at(db, file, at_offset, name)` (`resolve.rs:45`) walks ancestor scopes from innermost to outermost:
1. Check `bindings` in reverse source order, with a **position guard**: `binding_range.start() <= at_offset` (no forward references to let bindings)
2. Check `params` (always visible within their scope)
3. At `File`/`Package` scope: look up in `package_items` (own package then dependencies)
4. Skip `Class` scopes when nested -- field names are not visible via bare lookup in methods

---

## The `baml_std` Builtins Pipeline

The builtin standard library (`baml_std`) uses a two-phase pipeline: the **compiler** sees `.baml` stub files as ordinary source; the **runtime** uses `build.rs` codegen to generate Rust trait hierarchies from those same stubs. This section traces both paths.

### The `.baml` stub files

**Location**: `crates/baml_builtins2/baml_std/baml/`

Stubs are organized by namespace (`ns_*` subdirectories): `containers.baml`, `core.baml`, `string.baml` at the root; `ns_llm/llm.baml`, `ns_http/http.baml`, `ns_fs/fs.baml`, `ns_math/math.baml`, etc.

Each stub uses two special body keywords:
- `$rust_function` -- marks a synchronous VM builtin
- `$rust_io_function` -- marks an async I/O builtin

Behavioral directives are expressed as CST comments on the preceding line:
- `//baml:mut_self` -- receiver is `&mut`
- `//baml:vm` -- passes `vm: &BexVm` to the Rust impl
- `//baml:mut_vm` -- passes `vm: &mut BexVm`

All files are embedded at compile time via `include_str!` in `baml_builtins2/src/lib.rs:54-62`. The complete list is the constant `ALL: &[BuiltinFile]`.

### Compiler path: injection via `Compiler2ExtraFiles`

`Compiler2ExtraFiles` is a Salsa `#[salsa::input]` struct (`baml_workspace/src/lib.rs:70`) holding `files: Vec<SourceFile>`. It is separate from the `Project` input (which carries user files).

During `ProjectDatabase::set_project_root` (`baml_project/src/db.rs:251`):
1. `load_builtin_baml_files()` iterates `baml_builtins2::ALL`, registers each as a `SourceFile`
2. `Compiler2ExtraFiles::new(self, builtin_files)` stores them as a Salsa input

The HIR query `compiler2_all_files` (`baml_compiler2_hir/src/lib.rs:76`) unions user files with `compiler2_extra_files`, filtering out v1 builtins. All downstream queries (`namespace_items`, `package_items`, `file_semantic_index`) work from this combined view. Builtin functions type-check like any other function.

### Runtime path: `build.rs` codegen

Three crates run `extract_native_builtins()` (`baml_builtins2_codegen/src/extract.rs:51`) at build time. This function iterates `baml_builtins2::ALL`, lexes + parses + lowers each file to AST, then collects every function with a `$rust_function` or `$rust_io_function` body into `NativeBuiltin` records.

Each crate generates different output:

| Crate | Build script generates | Output |
|---|---|---|
| `bex_vm` | `generate_native_trait()` | `BamlClass*` / `BamlNamespace*` / `BamlPackageBaml` traits |
| `bex_vm_types` | `generate_sys_op_enum()` | `SysOp` enum (one variant per I/O builtin) |
| `sys_types` | `generate_io_traits()` | `IoClass*` / `IoNamespace*` / `IoPackageBaml` traits |

The generated trait hierarchy mirrors the namespace structure. For example, `BamlClassArray` has one required method per array builtin (`length`, `push`, etc.), plus a `__dispatch_array(method)` method that routes string method names to function pointers. `BamlPackageBaml` is the root supertrait with `get_native_fn(path) -> Option<NativeFunction>`.

### Runtime dispatch

The zero-sized struct `PackageBamlImpl` (`bex_vm/src/package_baml/mod.rs:55`) implements all generated traits via submodules (`array.rs`, `map.rs`, `string.rs`, `math.rs`, etc.).

At program load time, `attach_builtins` (`mod.rs:65`) iterates all functions in the compiled `Program`. For each `FunctionKind::NativeUnresolved`, it calls `PackageBamlImpl::get_native_fn(name)` and stores the resolved function pointer as `FunctionKind::Native(ptr)`.

At call time, the VM transmutes the stored pointer back to `fn(&mut BexVm, &[Value]) -> NativeFunctionResult` and invokes it directly.

### Data flow summary

```
.baml stubs (baml_builtins2/baml_std/)
    |
    |-- [Compiler path]
    |     include_str! -> BuiltinFile -> Compiler2ExtraFiles (Salsa input)
    |     -> compiler2_all_files() = user files + builtin files
    |     -> HIR/TIR/MIR/Emit treat builtins as ordinary functions
    |
    \-- [Runtime path]
          build.rs -> extract_native_builtins() -> lex/parse/lower -> NativeBuiltin records
          -> generate_native_trait() -> BamlClass*/BamlNamespace* traits (OUT_DIR)
          -> PackageBamlImpl impls all traits
          -> attach_builtins() resolves NativeUnresolved -> Native(fn ptr)
          -> VM calls fn ptr directly
```

---

## Testing Infrastructure

### Snapshot tests: `crates/baml_tests/`

The primary testing mechanism is a **build-time snapshot test generator**. `crates/baml_tests/build.rs` scans `projects/` directories and emits `src/generated_tests.rs` containing one Rust `mod` per project with one `#[test]` per compiler phase per file.

#### Phase numbering

| Phase | Name | Scope | What it snapshots |
|---|---|---|---|
| `01` | `lexer` | per-file | Token stream |
| `02` | `parser` | per-file | CST + parse errors |
| `03` | `hir` | per-project | HIR (scope tree, item tree, symbol contributions) |
| `04` | `tir` | per-project | TIR (typed expressions, resolved names) |
| `04_5` | `mir` | per-project | MIR (control flow graphs) |
| `05` | `diagnostics` | per-project | All diagnostics aggregated across phases |
| `06` | `codegen` | per-project | Bytecode (`compiler2_emit`) |
| `10` | `formatter` | per-file | Formatter idempotency (format twice, assert identical) |

Phases 01 and 02 generate one test per `.baml` file; phases 03-06 generate one test per project (loading all files). Snapshots are stored at `crates/baml_tests/snapshots/<project_name>/`.

#### Adding a new test case

1. Create `crates/baml_tests/projects/<new_name>/` with one or more `.baml` files
2. Run `cargo test --package baml_tests <new_name>`
3. Run `cargo insta accept --all` to commit initial snapshots
4. The build script picks up new directories automatically

#### Project categories

Projects are organized by what they test: `parser_*` (parser-focused, also get incremental/node-reuse tests), `basic_types`, `type_aliases`, `generics` (type system), `control_flow`, `catch_throw` (control flow), `error_cases` (diagnostics), `stream_types` (streaming), etc.

### Hand-written unit tests: `compiler2_tir/`

The `crates/baml_tests/src/compiler2_tir/` directory contains targeted tests for specific type-checker behaviors:

| Module | Tests |
|---|---|
| `inference.rs` | Core inference: literal types, widening, field access, mismatches |
| `phase3a.rs` | 12 diagnostic categories: union normalization, `UnknownType`, `ArgumentCountMismatch`, etc. |
| `phase3a_recursion.rs` | Cycle validation: direct self-ref, mutual recursion, structural vs non-structural edges |
| `phase5.rs` | Builtin stdlib package loading verification |
| `phase6.rs` | Generic method resolution for builtin types (`Array<T>`, `Map<K,V>`) |
| `phase7.rs` | Type narrowing: null-check, truthiness, early-return divergence |
| `phase8_exceptions.rs` | `catch`/`throw`/`throws` contract enforcement |
| `stream_expansion.rs` | Stream type expansion |

All tests use `support::make_db()` to create a `ProjectDatabase` with builtins loaded, and `support::render_tir(db, file)` to produce a human-readable snapshot of the typed IR.

### Incremental tests

`crates/baml_tests/src/incremental/mod.rs` provides `IncrementalTestDb`, which wraps `ProjectDatabase` with a Salsa event log. It records `WillExecute` events to assert exact execution counts.

`incremental/scenarios.rs` has 8 tests verifying Salsa early-cutoff behavior:
- Body edit forces re-lex but not cross-file invalidation
- Rename forces item tree rebuild
- Comment change re-runs lexer then stops
- Editing one file doesn't affect another file's cached queries
- Repeated identical queries hit zero re-executions
- Whitespace-only changes still re-lex but early-cut at `namespace_items`

### Running tests

```sh
# Run all snapshot tests (skip parser_stress for speed)
cargo test --package baml_tests -- --skip parser_stress

# Run a specific project's tests
cargo test --package baml_tests <project_name>

# Review/accept snapshot changes
cargo insta review
cargo insta accept --all

# Run LSP tests with auto-update
UPDATE_EXPECT=1 cargo test --package lsp_actions_tests
```

See `TEST_INSTRUCTIONS.md` for the full debugging workflow.

---

## Code References

### Salsa Architecture
- `baml_base/src/files.rs:14` -- `SourceFile` input
- `baml_workspace/src/lib.rs:39` -- root `Db` trait, `Project` input
- `baml_compiler2_hir/src/lib.rs:51` -- HIR `Db` trait, `file_semantic_index` (line 96, `no_eq`)
- `baml_compiler2_hir/src/ids.rs:43` -- `LocalItemId<T>` name-hash packing
- `baml_compiler2_hir/src/loc.rs:19` -- nine `#[salsa::interned]` `*Loc` types
- `baml_compiler2_hir/src/namespace.rs:130` -- `namespace_items` `PartialEq` early cutoff
- `baml_compiler2_hir/src/signature.rs:93` -- span/semantic split
- `baml_compiler2_tir/src/inference.rs:189` -- `infer_scope_types` per-scope query

### Type System
- `baml_compiler2_tir/src/ty.rs:92` -- `Ty` enum (21 variants)
- `baml_compiler2_tir/src/builder.rs:227` -- `infer_expr` (synthesis mode)
- `baml_compiler2_tir/src/builder.rs:391` -- `check_expr` (checking mode)
- `baml_compiler2_tir/src/builder.rs:3317` -- `join_types` (union construction)
- `baml_compiler2_tir/src/normalize.rs:18` -- `is_subtype_of` (structural subtyping)
- `baml_compiler2_tir/src/normalize.rs:94` -- `StructuralTy::is_subtype_of` (co-inductive)
- `baml_compiler2_tir/src/generics.rs` -- generic instantiation and inference

### Recursive Types
- `baml_compiler2_tir/src/normalize.rs:26` -- `find_recursive_aliases` (DFS cycle detection)
- `baml_compiler2_tir/src/normalize.rs:461` -- `find_invalid_alias_cycles` (Tarjan + structural edge check)
- `baml_compiler2_tir/src/normalize.rs:304` -- `normalize_impl` (Mu type construction)
- `baml_compiler2_tir/src/normalize.rs:149` -- Mu unfolding in subtype check
- `baml_compiler2_tir/src/normalize.rs:740` -- `find_invalid_class_cycles`

### LLM Desugaring
- `baml_compiler2_ast/src/lower_cst.rs:125` -- LLM body detection
- `baml_compiler2_ast/src/lower_cst.rs:247` -- `synthesize_llm_builtin_call`
- `baml_compiler2_ast/src/companions.rs:25` -- companion function generation
- `baml_compiler2_ast/src/ast.rs:467` -- `DeclarativeMeta::Llm`, `LlmBodyDef`
- `baml_compiler2_emit/src/lib.rs:353` -- `FunctionMeta::Llm` population at emit time
- `baml_builtins2/baml_std/baml/llm.baml:52` -- `call_llm_function<T>` builtin

### Scopes
- `baml_compiler2_hir/src/scope.rs:52` -- `ScopeKind` enum
- `baml_compiler2_hir/src/builder.rs:350` -- `lower_function` scope creation
- `baml_compiler2_tir/src/builder.rs:87` -- `TypeInferenceBuilder` (locals, declared_types, bindings)
- `baml_compiler2_tir/src/narrowing.rs:60` -- `extract_narrowings`
- `baml_compiler2_tir/src/resolve.rs:45` -- `resolve_name_at` scope-chain walk

### `baml_std` / Builtins
- `baml_builtins2/src/lib.rs:54` -- `builtin!` macro, `ALL: &[BuiltinFile]`
- `baml_builtins2_codegen/src/extract.rs:51` -- `extract_native_builtins()`
- `baml_builtins2_codegen/src/codegen.rs:580` -- `generate_native_trait()`
- `baml_workspace/src/lib.rs:70` -- `Compiler2ExtraFiles` Salsa input
- `baml_project/src/db.rs:251` -- `set_project_root` loads builtins
- `baml_compiler2_hir/src/lib.rs:76` -- `compiler2_all_files` unions user + builtin files
- `bex_vm/src/package_baml/mod.rs:55` -- `PackageBamlImpl`
- `bex_vm/src/package_baml/mod.rs:65` -- `attach_builtins`

### Testing
- `baml_tests/build.rs:24` -- snapshot test code generation
- `baml_tests/projects/` -- test fixture directories
- `baml_tests/snapshots/` -- snapshot files per project per phase
- `baml_tests/src/compiler2_tir/mod.rs` -- `support::make_db()`, `support::render_tir()`
- `baml_tests/src/incremental/mod.rs` -- `IncrementalTestDb`
- `baml_tests/src/incremental/scenarios.rs` -- Salsa memoization scenario tests
