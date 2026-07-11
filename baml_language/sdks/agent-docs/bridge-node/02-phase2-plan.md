# Phase 2 Plan: Codegen Scaffolding (sdkgen_nodejs)

## Overview

Phase 2 turns `sdkgen_nodejs::to_source_code` from a panicking stub into a
real emitter that produces a structurally correct `baml_sdk/` TypeScript
tree for any `SymbolPool`. The output is **scaffolding only** — every BAML
top (class, enum, type alias, function, static method, instance method)
renders as a `// kind Name` placeholder comment plus a runtime stub
(`export const Name: any = …;`) so that downstream `tsc --noEmit`
typechecks resolve imports cleanly.

The output follows the **directory-per-namespace** layout mandated by
`00a-example-ts-codegen-type-shapes.md` (§"File Layout"): every BAML
namespace — leaf or container — emits a *directory* with its own
`index.ts` + `index.d.ts`. A leaf namespace `user.lorem` emits
`baml_sdk/lorem/index.ts` + `baml_sdk/lorem/index.d.ts` (a directory with
`index.ts`, **not** a flat `lorem.ts`). The SDK root and pure *container*
directories — parents that exist solely to nest child namespaces (e.g.
`symbol_collisions/`, `vendor/`, `stream_types/`) — also get an
`index.ts` / `index.d.ts` whose body re-exports child namespaces *only*
(`export * as child from "./child"`), never flattening child symbols (no
`export * from "./child"`; a parent never hoists a child's symbols). This
mirrors the Python fixture's `__init__.py` + `__init__.pyi` split exactly:
`baml_sdk/primitives/__init__.py` → `baml_sdk/primitives/index.ts`,
`baml_sdk/symbol_collisions/lorem/__init__.py` →
`baml_sdk/symbol_collisions/lorem/index.ts`.

Phase 3 will replace placeholders with real types via `translate_ty`;
Phase 4 will replace stubs with real bindings (Pydantic equivalent, factory
calls); Phase 5 will wire codec; Phase 6 ships the release.

The deliverables are entirely in two places:
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/` — emitter
  source (Rust), structured as a near-1:1 port of
  `codegen_python/src/{lib,routing,emit/*,leaf,utils}.rs` but without
  `translate_ty.rs` (Phase 3). The file layout is a **direct mirror** of
  Python: where Python emits `<ns>/__init__.py(i)` for every namespace
  directory, Node emits `<ns>/index.ts` + `<ns>/index.d.ts`. Only the
  filename within the directory differs (`__init__.py(i)` → `index.ts(.d.ts)`);
  the directory structure is identical.
- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/` —
  the runtime `BamlTypeMap` analog (TypeScript), imported by every
  generated `_typemap.ts`.

## Goal (delivery criteria)

A run of `cargo nextest run -E 'package(sdk_test_nodejs_typescript)'`
produces a `generated/baml_sdk/` tree for each fixture
(`type_shapes`, `docstrings_etc`, `llm_functions`, and the additional
`function_calls` / `host_callables` fixtures present under
`sdk_tests/crates/nodejs_typescript/`) where:

1. `sdkgen_nodejs::to_source_code(pool, files, PreserveCase)` returns
   `HashMap<PathBuf, String>` (matching the Python `to_source_code`
   return type — the current `sdkgen_nodejs` stub already returns
   `HashMap`, see `lib.rs:22`) containing exactly the file set described
   in §4: one `<ns>/index.ts` + one `<ns>/index.d.ts` per namespace
   directory (leaf and container alike — root + pure nesting dirs +
   leaf namespaces), plus the two root-only files `_inlinedbaml.ts` and
   `_typemap.ts`. Codegen never emits `package.json` or `tsconfig.json`
   — the harness (`harness_setup/src/nodejs_typescript.rs`) writes those.
2. Routing matches the rules in
   `/Users/sam/thoughts/sam-projects/bridge-node/00a-spec-codegen-mappings.md`
   §"Package/namespace codegen rule" and §"Companion type rules": user → root,
   `baml` → `baml/`, vendor → `vendor/<pkg>/`, `$stream` classes prepended
   with `stream_types/`, function companions alongside the parent.
3. Each top renders as the **placeholder shape** described in §3:
   ```
   // class Resume
   export const Resume: any = BAML_PLACEHOLDER;
   ```
   plus type-level
   ```
   // class Resume
   export const Resume: any;
   ```
   in the `.d.ts` sibling. (The root module emits root-namespace symbols
   directly; namespaced symbols live in their `<ns>/index.ts` file. There
   is **no aggregated `b` client object** — the package module itself is
   the client; see A4. Phase 4 replaces each placeholder with the real
   `defineFunction(...)` / `defineInstanceFunction(...)` / `export class`
   shape from `00a-example-ts-codegen-type-shapes.md`.)
4. The runtime `BamlTypeMap` module is registered: the root `index.ts`
   imports `initializeRuntime`, `setTypeMap`, and `BamlTypeMap` from
   `@boundaryml/baml-core-node` (the canonical runtime package name — see the
   ⚠ note under "What exists today"), builds the lazy entry maps from
   `_typemap.ts`, and calls `setTypeMap(_TYPE_MAP)`. The typemap is fully
   functional
   (the lazy entries dict is populated for every emitted class/enum/alias
   so a Phase 5 decode walk would resolve correctly), even though the
   underlying placeholder values are `undefined`.
5. `cargo nextest run -E 'package(sdk_test_nodejs_typescript) and test(tsc)'`
   passes on all three fixtures. The `jest` tests partially pass — namespace-import
   tests pass (`expect(mod).toBeDefined()`), individual-symbol tests pass
   (`expect(Foo).toBeDefined()` with the stub `Foo: any = undefined;` is
   truthy under jest's `toBeDefined()` which only asserts not-`undefined` …
   actually `toBeDefined()` checks for `!== undefined`, so the stub
   `Foo: any = undefined;` fails this. See the assumption in §3.2 — we
   set the placeholder to a non-undefined sentinel for that reason). Enum
   keys-of tests fail (placeholder is a sentinel object, not an enum).
   That red/green split is the expected TDD progression.
6. Routing unit tests in `sdkgen_nodejs/src/routing.rs` mirror the
   Python ones and all pass.

## Current State Analysis

### What exists today

> **⚠ Crate rename (Phase 2.0).** The new Node SDK generator crate is named **`sdkgen_nodejs`**, but the stub currently lives in the repo as **`codegen_nodejs`** (`baml_language/sdks/nodejs/codegen_nodejs/`, `Cargo.toml` `name = "codegen_nodejs"`, registered in `baml_language/Cargo.toml` members + path-dep at `:6` / `:58`). Step zero of Phase 2 is to rename the crate: move the directory to `sdks/nodejs/sdkgen_nodejs/`, set `Cargo.toml` `name = "sdkgen_nodejs"`, and update the two workspace references (and any dependent, e.g. `baml_cli`). All paths below already use the post-rename `sdkgen_nodejs` name. (The existing per-language crates `codegen_python` / `codegen_go` keep their names; only the new Node crate adopts the `sdkgen_*` convention.)

- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/Cargo.toml`
  declares `baml_codegen_types` as the only workspace dep (verified). No
  `askama`, `indexmap`, or other dep is present yet — sub-phase 2.6 adds
  whatever the `_inlinedbaml.ts` renderer needs.
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs`
  is the **only** file in `src/` today — no `routing.rs`, `leaf.rs`, or
  `emit/` exist yet (verified by `ls src/`: `lib.rs` and `Cargo.toml`
  are the entire crate). It has the entry-point signature
  `pub fn to_source_code(pool, user_baml_files, naming_convention) -> HashMap<PathBuf, String>`
  (`lib.rs:18-26`, returns `HashMap`, **not** `IndexMap`) with a body of
  `unimplemented!("sdkgen_nodejs::to_source_code is a stub …")`
  (`lib.rs:23-25`) — confirmed still a pure stub; the prior pass added
  nothing beyond it. It re-exports `NamingConvention` /
  `OutputType` and defines `pub type UserBamlFile = (PathBuf, String)`,
  matching the Python `lib.rs`.
- `/Users/sam/baml3/baml_language/sdk_tests/harness_setup/src/nodejs_typescript.rs`
  (the build harness moved here — the old `sdk_tests/build/src/nodejs.rs`
  path no longer exists) wraps the codegen call in
  `panic::catch_unwind` (`nodejs_typescript.rs:173-205`) precisely so
  the build script survives the stub. The `Ok(output)` arm already
  contains the `for (rel, content) in output { … fs::write … }` loop;
  the `Err(_)` arm just records a diagnostic and leaves `baml_sdk/`
  empty. Once Phase 2 lands, the `Err(_)` arm becomes dead.
  **Action item for sub-phase 2.6 below**: drop the `catch_unwind`
  wrapper so a future codegen panic is loud, not silently warned about.
- `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs_typescript/{type_shapes,llm_functions,docstrings_etc}/customizable/main.test.ts`
  (plus a `generated/main.test.ts` sibling in each fixture, and
  additional `function_calls` / `host_callables` fixtures) already encode
  the expected import shape: `import * as mod from "./baml_sdk/<seg>"`,
  `import { Foo } from "./baml_sdk"` (root-namespace), and per-test
  `expect(Foo).toBeDefined()` style assertions. The nextest package name
  is `sdk_test_nodejs_typescript` (`crates/nodejs_typescript/Cargo.toml`).
- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts`
  exposes `BamlRuntime`, `AbortController`, `BamlHandle`,
  `HostSpanManager`, `getVersion`, `flushEvents`, `encodeCallArgs`,
  `decodeCallResult`, `CtxManager`, the `BamlError` family, and the
  `Collector`/`FunctionLog` wrappers. It does **not** currently export
  `defineFunction`, `defineInstanceFunction`, `BamlTypeMap`,
  `setTypeMap`, `getTypeMap`, or `BAML_PLACEHOLDER`, and there is **no
  `typescript_src/typemap.ts`** yet (verified — only `ctx_manager.ts`,
  `errors.ts`, `index.ts`, `native.d.ts`, `proto.ts`, `proto/` exist).
  Sub-phase 2.5 adds these exports (in `bridge_nodejs`, not in
  `sdkgen_nodejs`). Note: the generated leaf bodies in this plan emit
  the *Phase-2 placeholder* form (`export const Foo: any = …`); the real
  `defineFunction(...)` callable shape from
  `00a-example-ts-codegen-type-shapes.md` is Phase 4 work.

  > ⚠ **Spec note — runtime package name.** The canonical runtime npm
  > package (locked 2026-05-29 across all four `00*` spec docs) is
  > **`@boundaryml/baml-core-node`**. Generated code imports `defineFunction`,
  > `defineInstanceFunction`, `initializeRuntime`, `BamlTypeMap`,
  > `setTypeMap`, and the stdlib re-exports (`Image`/`Audio`/`Video`/
  > `Pdf`/`Stream`) from it. This plan uses `@boundaryml/baml-core-node`
  > throughout. **Discrepancy to reconcile:** the actual
  > `bridge_nodejs/package.json` `"name"` field today still reads
  > **`@boundaryml/baml-node`** (verified). Either the package is renamed
  > to `@boundaryml/baml-core-node` before Phase 2 ships, or the import
  > constant is a one-line override in the banner/import templates
  > (`A10`, `A4`, `render_root_index`, `_typemap.ts`). Flag this when the
  > package name is finalized; do NOT silently emit `-node` — the spec is
  > `-core`.

### Python prior art file map (1:1 references)

| Python file | New Node file | Notes |
| --- | --- | --- |
| `codegen_python/src/lib.rs` | `sdkgen_nodejs/src/lib.rs` | top-level `to_source_code`, banner, `_inlinedbaml`/`_typemap`/root index renderers. Python emits `__init__.py(i)` per directory via `init_py_path`/`init_pyi_path` (`lib.rs:231-247`) and `render_root_init`/`render_package_init` (`lib.rs:275-354`). **Node mirrors this directly**: every namespace directory (leaf and container) emits `<segs…>/index.ts` + `<segs…>/index.d.ts`, i.e. `init_py_path`'s `__init__.py` → `index.ts`, `init_pyi_path`'s `__init__.pyi` → `index.d.ts`. Directory structure is identical; only the in-directory filename changes. See "File layout produced by Phase 2". |
| `codegen_python/src/routing.rs` | `sdkgen_nodejs/src/routing.rs` | port the routing logic verbatim — both languages share the same `LeafPath` namespace rules (`routing.rs:57-110`). `LeafPath` is layout-agnostic; the directory→filename mapping (`<ns>/index.ts` vs `<ns>/__init__.py`) lives in `lib.rs`, not `routing.rs`. The `init_py`/`is_root` test helpers (`routing.rs:35-49`) port to `init_ts` (returning `<segs…>/index.ts`) + `is_root`. |
| `codegen_python/src/emit/mod.rs` | `sdkgen_nodejs/src/emit/mod.rs` | `build_emitted`, `EmittedSymbol`, `expand_function`/`expand_methods`/`expand_callable`/`bare_callable_name`, sync/async fan-out (`emit/mod.rs:59-363`). Verbatim. |
| `codegen_python/src/emit/{class,enum_,function,method,type_alias}.rs` | `sdkgen_nodejs/src/emit/{class,enum_,function,method,type_alias}.rs` | per-kind struct fields. Python's `PyClass` etc. carry `py_name`/`source`/`properties`/`static_methods`/`instance_methods`/`generic_params`/`docstring` (`emit/mod.rs:99-111`); the Node structs keep `name`/`source` and drop `arg_tys`/`return_ty`/`generic_params`/`docstring` for Phase 2 — Phase 3 reintroduces them. Rename `py_name` → `name`. |
| `codegen_python/src/emit/typemap_file.rs` | `sdkgen_nodejs/src/emit/typemap_file.rs` | `_typemap.ts` renderer. Python (`typemap_file.rs:28-92`) emits three sorted dict literals keyed by source FQN → `(module_path, py_name)`, where `module_path` is the **dotted** form `baml_sdk.lorem` (`module_path_for_leaf`, `typemap_file.rs:78-92`), then `BamlTypeMap.from_lazy_entries(classes=…, enums=…, type_aliases=…)`. **Node diverges in two ways**: (1) `module_path` is the **filesystem-relative** path `baml_sdk/lorem` (JS imports are path-based, not dotted) which CommonJS resolves to the directory's `baml_sdk/lorem/index.ts`; (2) the literals are three `Record<string,[string,string]>` and the installer is `BamlTypeMap.fromLazyEntries({ classes, enums, typeAliases })`. Functions are skipped in both (`typemap_file.rs:49`). Sort: classes/enums/aliases each `.sort()`ed by `(source FQN, module_path, name)` (`typemap_file.rs:53-55`). |
| `codegen_python/src/leaf.rs` | `sdkgen_nodejs/src/leaf.rs` | scaffolding only in Phase 2 — `LeafBody`, `group_and_sort`, plus a Phase-2 `render_leaf_body{,_dts}` that emits only placeholders. The import-resolution machinery (`collect_root_imports`, `RootImports`, `RelImport`) is **not ported** in Phase 2 — added in Phase 4 alongside `translate_ty`. |
| (none — Phase 3) | (none — Phase 3) | `translate_ty.rs` — not in Phase 2 |
| `src/baml_core/typemap.py` | `bridge_nodejs/typescript_src/typemap.ts` | runtime `BamlTypeMap` class, `set_type_map`/`get_type_map` → `setTypeMap`/`getTypeMap`. (Verified path: `sdks/python/src/baml_core/typemap.py`.) |

## What We're NOT Doing (Phase 3+)

These are intentionally deferred and the Phase 2 plan must not creep into them:

- **No `translate_ty`** — every per-symbol Phase 2 render is just a comment
  with the bare name. Phase 3 introduces `translate_ty.rs` and the leaf
  renderer will start consuming `Ty` values for class fields, function
  args, return types, etc.
- **No real class / enum / alias / function bodies.** A BAML
  `class Resume { name string }` produces ONLY:
  ```
  // class Resume
  export const Resume: BamlPlaceholder = BAML_PLACEHOLDER;
  ```
  (or equivalent — see §3.2 for the exact shape including the sentinel
  rationale). Phase 4 emits the real interface/class.
- **No docstring handling** — `PyClass.docstring` etc. fields are kept on
  the structs (so Phase 4 can fill them in) but not consumed by Phase 2's
  renderer.
- **No proto encoder / decoder changes.** Phase 5 wires codegen-emitted
  classes through the proto decoder via the typemap.
- **No bridge_nodejs runtime fixes for `BamlPlaceholder`/`callFunction` arg
  shape** beyond exposing `BamlTypeMap` and `setTypeMap`. The Phase 1 plan
  owns runtime parity; the Phase 2 plan only adds the typemap class and
  related exports.
- **No release / workflow wiring** — Phase 6.
- **No CLAUDE.md, no README, no project-level docs.** Internal `//!` and
  `///` rustdoc on new code is welcome and expected; no separate docs.

## Implementation Approach

The emitter is a direct, line-by-line port of `codegen_python` with a small
number of Node-specific design decisions documented as assumptions below.

### Assumptions (sensible defaults locked in)

These are decisions the plan makes once and references throughout — picked
because they (a) match the Python prior art most closely and (b) keep the
Phase 2 surface area small. Any future deviation requires a revision to
this plan, not a new fork inside the code.

- **A1. One `<ns>/index.ts` + `<ns>/index.d.ts` per namespace
  directory — leaf and container alike.** This is the spec-mandated
  layout from `00a-example-ts-codegen-type-shapes.md` §"File Layout",
  a direct mirror of the Python fixture: Python's
  `baml_sdk/primitives/__init__.py(i)` becomes
  `baml_sdk/primitives/index.ts` + `index.d.ts`, and
  `baml_sdk/symbol_collisions/lorem/__init__.py(i)` becomes
  `baml_sdk/symbol_collisions/lorem/index.ts` + `index.d.ts`. There is
  **no flat-file layout**: a leaf namespace is a directory with an
  `index.ts`, never a `lorem.ts`. The SDK root
  (`baml_sdk/index.ts` + `index.d.ts`) and pure container dirs that exist
  *only* to nest child namespaces (`symbol_collisions/index.ts`,
  `vendor/index.ts`, `stream_types/index.ts`, etc.) get the same
  `index.ts` + `index.d.ts` pair, whose body re-exports child namespaces
  only. A single namespace that is both a routed leaf AND has children
  (e.g. `stream_types/` carrying both root-namespace `Foo$stream` and
  child namespaces) emits one `<dir>/index.ts` that carries its own
  symbols *and* the child re-exports together — exactly matching Python's
  merged `__init__.py` for the same directory. Because every namespace is
  already a directory, there is no special "flat file vs. index" branch:
  every routed `LeafPath` and every interior directory maps to
  `<segments…>/index.ts`.
  Emitting `.d.ts` matches the Python `.pyi` parallel called out in
  `00b-overview.md` phase 2 ("we'll need to generate both `.ts` files and
  `.d.ts` files") and gives Phase 4 a clean hook for emitting types
  separately from runtime bindings. TypeScript's `noEmit: true` (already
  set in the fixture `tsconfig.json`) means we hand-roll `.d.ts` siblings
  without `tsc` ever running. Per-file content:
  - `.ts` carries runtime placeholders (`export const Foo = …`).
  - `.d.ts` carries type-position placeholders (`export const Foo: any;`
    plus, in Phase 4, the real `export interface Foo {...}`).
  In Phase 2 the `.d.ts` is structurally a duplicate of the `.ts`
  (placeholders plus comments). Future Phase 4 may demote unused
  `.d.ts` files entirely; until then we keep both for parity.

- **A2. Module style: CommonJS (`module: "commonjs"`).** The fixture
  `tsconfig.json` already uses `"module": "commonjs"` and the
  `package.json` template sets `"type": "commonjs"` — generated code
  must compile under that toolchain. Concretely: leaf `<ns>/index.ts`
  files use `export const`, `export class`, `export enum`, `export type`;
  container `index.ts` files use re-export `export * as <child> from
  "./<child>";` lines. No `module.exports = …`, no top-level `await`.

- **A3. Container `index.ts` re-exports child namespaces — never
  flattens.** A container (barrel) file contains **no generated BAML
  symbols**; it exists solely to expose nested namespace paths. It emits
  exactly:
  ```ts
  export * as <child> from "./<child>";
  ```
  one line per child — **never** `export * from "./<child>"`, which would
  hoist (flatten) the child's symbols into the parent. The spec
  (`00a-example` §"File Layout") is explicit: `symbol_collisions`
  exposes `lorem` as a child namespace but must **not** export
  `Ipsum` directly; `Ipsum` stays reachable only as
  `symbol_collisions.lorem.Ipsum`. Every child reference `<child>`
  resolves to a directory (`./lorem` → `lorem/index.ts`,
  `./vendor` → `vendor/index.ts`) under CommonJS directory resolution, so
  the re-export line is identical for leaves and containers alike.
  ECMAScript `export * as ns from "./x"` is supported in TS ≥ 3.8; the
  fixture toolchain is TS 5.4. This makes `import * as b from
  "baml_sdk"` expose `b.lorem.Resume`, `b.symbol_collisions.lorem.Ipsum`,
  etc., with no symbol hoisted out of its namespace.

- **A4. Root `index.ts` runtime init — the package module IS the
  client; there is no aggregated `b` object.** Mirrors `render_root_init`
  from Python (`lib.rs:325-339`), which wires the runtime +
  `set_type_map` and re-exports child packages, and does **not** build a
  client value. Per `00a-example` §"Root Package", the root emits:
  ```ts
  import { initializeRuntime, setTypeMap, BAML_PLACEHOLDER } from "@boundaryml/baml-core-node";
  import * as _inlinedbaml from "./_inlinedbaml";
  import { _TYPE_MAP } from "./_typemap";

  initializeRuntime("baml_src", _inlinedbaml.FILES);
  setTypeMap(_TYPE_MAP);

  export * as <child> from "./<child>";
  // (one line per top-level child namespace / container)
  ```
  (`initializeRuntime` is a **named export** of `@boundaryml/baml-core-node`,
  per the `00a-example` root snippet — not a static method on
  `BamlRuntime`. See A4a for the Phase-1 dependency.)
  The root leaf's own root-namespace symbols (`user.Foo`, `user.make_foo`,
  …) follow as plain module exports right after — **not** members of any
  `b` object. Users do `import * as b from "baml_sdk"` and call
  `b.make_foo(...)` (root) or `b.primitives.return_int_async()`
  (namespaced). Per `00a-example` §"Root Package": "The root module does
  not define an `export const b` client object … The package module
  itself is the client." Both `00a-spec-codegen-mappings.md` §"File
  layout rule" and §"Rules" now agree (re-edited 2026-05-29): "There is
  no aggregated `b` client object: the package module itself is the
  client." Phase 2 follows the no-`b` model throughout; if any emitter
  draft produces an `export const b = {...}` aggregate that collects
  functions as methods, that is a bug — root functions are plain module
  exports and namespaced functions are reached only through their
  namespace directory.

  **A4a. `initializeRuntime` API surface.** Phase 1 owns this API. The
  `00a-example` root snippet imports `initializeRuntime` as a **named
  export** of `@boundaryml/baml-core-node` and calls
  `initializeRuntime("baml_src", _inlinedbaml.FILES)`. The current
  `bridge_nodejs/typescript_src/index.ts` exports `BamlRuntime` directly
  from `./native` but does **not** yet export a free `initializeRuntime`
  (verified). **If Phase 1 has not landed this named export by the time
  Phase 2 ships, sub-phase 2.5 below adds a minimal pass-through.** This
  plan assumes the named export exists with the same shape as Python's
  `BamlRuntime.initialize_runtime("baml_src", FILES)`.

- **A5. `_inlinedbaml.ts` shape.** Mirrors `_inlinedbaml.py`:
  ```ts
  // (banner)
  export const FILES: Record<string, string> = {
    "main.baml": "class Foo {}\n",
    "lorem/types.baml": "class Resume {…}\n",
  };
  ```
  Sorted by relative path, same string-escape rules as Python.

- **A6. `_typemap.ts` shape.** Mirrors `_typemap.py`
  (`emit/typemap_file.rs`) but with JS path-based module paths and a
  `Record<…>` / `fromLazyEntries(...)` surface in place of Python's dict /
  `from_lazy_entries(...)`:
  ```ts
  import { BamlTypeMap } from "@boundaryml/baml-core-node";

  const _CLASS_ENTRIES: Record<string, [string, string]> = {
    "user.lorem.Resume": ["baml_sdk/lorem", "Resume"],
    // …
  };
  const _ENUM_ENTRIES: Record<string, [string, string]> = { … };
  const _ALIAS_ENTRIES: Record<string, [string, string]> = { … };

  export const _TYPE_MAP = BamlTypeMap.fromLazyEntries({
    classes: _CLASS_ENTRIES,
    enums: _ENUM_ENTRIES,
    typeAliases: _ALIAS_ENTRIES,
  });
  ```
  `module_path` is the **filesystem-relative** path under the SDK root
  with `baml_sdk` as the leading directory segment — `"baml_sdk/lorem"`
  for the leaf namespace `lorem`, **resolving to that directory's
  `baml_sdk/lorem/index.ts`** via CommonJS directory resolution.
  Root-leaf symbols use `"baml_sdk"` (the root `index.ts`). This is
  forward-slash, path-based, not the dotted `baml_sdk.lorem` Python form
  (`module_path_for_leaf` in `typemap_file.rs:78-92` returns dotted; the
  Node port returns slash-joined). The runtime `BamlTypeMap.getClass`
  uses `require(modulePath)[attr]` semantics (see §A8) — and
  `require("…/lorem")` resolves `lorem/index.ts`. Phase 2's typemap is
  fully functional in
  the sense that every emitted class/enum/alias gets an entry; the values
  it resolves to are `BAML_PLACEHOLDER` sentinels until Phase 4, but
  that's by design.

  Sort order: same as Python's typemap walk — `classes`/`enums`/`aliases`
  each `.sort()`ed by `(source FQN, module_path, name)` ascending
  (`typemap_file.rs:53-55`).

- **A7. No `py.typed` analog.** Python emits an empty `py.typed` PEP 561
  marker at the SDK root (`lib.rs:215`). TypeScript needs no such marker —
  type checkers discover types via `package.json`'s `"types"` field, and
  the fixture's `package.json` / `tsconfig.json` are written by the
  harness (`sdk_tests/harness_setup/src/nodejs_typescript.rs`, near the
  `fs::write(generated.join("package.json"), …)` call), not by codegen.
  Codegen never emits a `package.json`, `tsconfig.json`, or any marker
  file — only `*.ts`, `*.d.ts`, `_inlinedbaml.ts`, and `_typemap.ts`.

- **A8. Runtime typemap API surface (sub-phase 2.5).** The runtime
  `BamlTypeMap` lives in
  `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/typemap.ts`
  and is re-exported from `index.ts`. Same shape as Python:
  ```ts
  export type LazyEntry = [string, string]; // [modulePath, attrName]

  export class BamlTypeMap {
    static fromLazyEntries(args: {
      classes: Record<string, LazyEntry>;
      enums: Record<string, LazyEntry>;
      typeAliases: Record<string, LazyEntry>;
    }): BamlTypeMap { … }

    getClass(fqn: string): any { … }
    getEnum(fqn: string): any { … }
    getTypeAlias(fqn: string): any { … }
    pyTypeToBamlType(cls: any): string { … } // (renamed to jsTypeToBamlType in §2.5)
    warm(): void { … }
  }

  let _TYPE_MAP = new BamlTypeMap();
  export function setTypeMap(m: BamlTypeMap): void { _TYPE_MAP = m; }
  export function getTypeMap(): BamlTypeMap { return _TYPE_MAP; }
  ```
  `getClass` uses `require(modulePath)[attrName]` and memoizes. The
  stdlib reverse-overrides equivalent (the five media + stream classes
  in `_STDLIB_REVERSE_OVERRIDES`) is wired up in 2.5 but seeded to the
  Node-side BamlImage/BamlAudio/BamlVideo/BamlPdf/BamlStream class
  identities (which Phase 1 owns).

  > **Forward note (from `10a-todo-items.md` Phase D).** The encode-side
  > reverse machinery — `jsTypeToBamlType`, `_STDLIB_REVERSE_OVERRIDES`,
  > the reverse-seeding loop inside `fromLazyEntries`, and the `reverse`
  > field — is expected to become **dead** once the JS encoder stops
  > tagging class instances with their FQN (the encoder now emits every
  > non-builtin object as `mapValue` and lets
  > `coerce_arg_to_declared_type` recover the type). A later cleanup pass
  > deletes all four; the decode-side `getClass`/`getEnum`/`_resolve`
  > stays. Phase 2 may emit the reverse table for parity, but should NOT
  > build new dependencies on it. Treat the `jsTypeToBamlType`/reverse
  > code as provisional.

- **A9. Placeholder shape (the key Phase 2 decision).** For BAML
  `class Resume { name string; age int }`, the Phase 2 emitter produces:
  ```ts
  // .ts
  // class Resume
  export const Resume: any = BAML_PLACEHOLDER;

  // .d.ts
  // class Resume
  export const Resume: any;
  ```
  where `BAML_PLACEHOLDER` is a single module-scoped sentinel imported
  from `@boundaryml/baml-core-node` (added in sub-phase 2.5):
  ```ts
  // in bridge_nodejs/typescript_src/index.ts
  export const BAML_PLACEHOLDER: any = Object.freeze({ __bamlPlaceholder: true });
  ```
  Rationale: jest's `expect(Foo).toBeDefined()` checks for `Foo !== undefined`.
  Using a frozen sentinel object instead of `undefined` lets the Phase 2
  jest tests in `customizable/<f>/main.test.ts` go green for the
  "is defined" cases while leaving enum-shape tests red until Phase 4.
  The TypeScript type `any` makes static usages like
  `Resume.someField`, `new Resume()`, or `Sentiment.POSITIVE` typecheck —
  important so `tsc --noEmit` doesn't gate Phase 2 on having real
  signatures.

- **A10. Banner / preamble.** Mirrors `PYTHON_BANNER` — a `// fmt: off`
  / `// eslint-disable` / `// @ts-nocheck`-free preamble. Concretely:
  ```ts
  /* eslint-disable */
  // @ts-nocheck
  // prettier-ignore
  // ----------------------------------------------------------------------------
  //
  //  Welcome to Baml! To use this generated code, please run the following:
  //
  //  $ pnpm add @boundaryml/baml-core-node
  //
  // ----------------------------------------------------------------------------
  // This file was generated by BAML: please do not edit it. Instead, edit the
  // BAML files and re-generate this code using: baml-cli generate
  // baml-cli is available with the baml package.
  //
  ```
  Note `@ts-nocheck` is NOT used (it would gut Phase 4's type-checking
  work). Phase 2 generated code is intended to typecheck cleanly under
  `tsc --noEmit` precisely because every placeholder is `: any`.

- **A11. Reserved-identifier sanitization.** Python's `routing.rs`
  sanitizes the `assert` package segment. JavaScript has many more
  reserved words (`class`, `default`, `function`, `import`, `package`,
  `let`, `const`, `private`, `public`, …) AND the path-segment names
  must be valid file names. Phase 2 ports the **single-case
  `sanitize_python_module_segment`** approach: only handle segments
  that would break the emitted code today (currently none — no test
  fixture exercises a vendor package or namespace named after a JS
  keyword). The function exists and is wired through the routing
  module so that Phase 4 / Phase 6 can extend it cheaply. The Python
  unit test for `assert_` is ported but expects `"assert"` to pass
  through (no sanitization needed in JS — `import * as assert from
  "./assert"` is legal because `assert` is contextual). The
  `assert_namespace_segment_is_sanitized` Python test is **dropped**
  in Phase 2; a Node-side `keyword_does_not_collide` test placeholder
  is added so the matrix shape stays comparable.

- **A12. Naming convention.** Like the Python emitter, Phase 2 only
  supports `NamingConvention::PreserveCase` and asserts on any other
  value. Phase 4 can wire `NamingConvention::Language` once the JS
  equivalent of Python's underscore-vs-camelCase rule is settled.

- **A13. Stdlib re-export routing for the five runtime-owned types.**
  `baml.media.Image`, `baml.media.Audio`, `baml.media.Video`,
  `baml.media.Pdf`, and `baml.llm.Stream` are **unique among generated
  symbols**: codegen must NOT emit a generated `class`/placeholder body
  for them. They re-export the runtime-owned class from
  `@boundaryml/baml-core-node` because callers need the runtime's
  constructors, static helpers, and handle identity (a generated
  structural class would not round-trip). This mirrors the Python fixture,
  which re-binds them (`from baml_core.baml_py import BamlImage as Image`,
  `from baml_core import BamlStream as Stream`). The leaf renderer
  (§2.3) must detect these five FQNs and route them to a re-export line
  (`export { BamlImage as Image } from "@boundaryml/baml-core-node";` shape per
  `00a-example` §"Stdlib Re-Exports") instead of the placeholder body.
  Every OTHER top (user/vendor classes, enums, type aliases, free
  functions, method bindings) is codegen-emitted as a placeholder in
  Phase 2. The exact media-shape wiring (snake_case static aliases,
  `_fromHandle` decode plumbing) is a known open item tracked in
  `10a-todo-items.md` §B3; Phase 2 only needs the *routing decision*
  (re-export vs. emit) correct, with the bodies filled in by Phase 4.

## File layout produced by Phase 2

For BAML input:
```
// fully qualified BAML symbols:
class user.lorem.Resume { name string; age int }
function user.lorem.extract_resume() -> Resume
function user.lorem.extract_resume$stream() -> Resume    // companion
class user.lorem.Resume$stream { … }                     // companion
class baml.http.Response { … }                           // stdlib
class aws.s3.Bucket { … }                                // vendor
class user.Foo                                           // root namespace
enum user.ipsum.Sentiment { POSITIVE, NEGATIVE }
```

the emitter produces (relative to `baml_sdk/`). **Every namespace — leaf
or container — is a directory with its own `index.ts` + `index.d.ts`
(mirroring Python's `__init__.py` / `__init__.pyi`):**

```
index.ts                  # root CONTAINER: runtime init + child re-exports + root-namespace placeholders (Foo)
index.d.ts
_inlinedbaml.ts           # FILES dict
_typemap.ts               # BamlTypeMap.fromLazyEntries(...)
lorem/index.ts            # LEAF: Resume placeholder + extract_resume / extract_resume_async / extract_resume_stream / extract_resume_stream_async
lorem/index.d.ts
ipsum/index.ts            # LEAF: Sentiment placeholder
ipsum/index.d.ts
baml/index.ts             # CONTAINER: re-exports http (no symbols of its own)
baml/index.d.ts
baml/http/index.ts        # LEAF: Response placeholder
baml/http/index.d.ts
vendor/index.ts           # CONTAINER: re-exports aws
vendor/index.d.ts
vendor/aws/index.ts       # CONTAINER: re-exports s3
vendor/aws/index.d.ts
vendor/aws/s3/index.ts    # LEAF: Bucket placeholder
vendor/aws/s3/index.d.ts
stream_types/index.ts        # CONTAINER: re-exports lorem
stream_types/index.d.ts
stream_types/lorem/index.ts  # LEAF: Resume$stream placeholder (bare name Resume on emit)
stream_types/lorem/index.d.ts
```

The mapping rule: a `LeafPath { segments: [a, b] }` that routes at least
one symbol emits `a/b/index.ts` + `a/b/index.d.ts`. Every *proper-prefix*
directory of a leaf (`a/`, and the root) that is NOT itself a routed leaf
emits a container `a/index.ts` + `a/index.d.ts` that re-exports its
immediate children. The root (`segments: []`) is the container
`index.ts`. A directory that is BOTH a routed leaf AND has children emits
a single `index.ts` carrying its own symbols *and* the child re-exports
(matching Python's merged `__init__.py`).

### Example: `baml_sdk/lorem/index.ts` (leaf)

```ts
/* eslint-disable */
// prettier-ignore
// ---------------------------------------------------------------------------
// Welcome to Baml! …
// ---------------------------------------------------------------------------

import { BAML_PLACEHOLDER } from "@boundaryml/baml-core-node";

// class Resume
export const Resume: any = BAML_PLACEHOLDER;

// function extract_resume
export const extract_resume: any = BAML_PLACEHOLDER;
export const extract_resume_async: any = BAML_PLACEHOLDER;

// function extract_resume$stream
export const extract_resume_stream: any = BAML_PLACEHOLDER;
export const extract_resume_stream_async: any = BAML_PLACEHOLDER;
```

### Example: `baml_sdk/lorem/index.d.ts` (leaf)

```ts
/* eslint-disable */
// prettier-ignore
// ---------------------------------------------------------------------------
// Welcome to Baml! …
// ---------------------------------------------------------------------------

// class Resume
export const Resume: any;

// function extract_resume
export const extract_resume: any;
export const extract_resume_async: any;

// function extract_resume$stream
export const extract_resume_stream: any;
export const extract_resume_stream_async: any;
```

> ⚠ **Spec note — placeholder vs. final shape.** The `export const Foo:
> any = BAML_PLACEHOLDER;` form above is the **Phase-2 scaffolding
> placeholder only**. Phase 4 replaces each placeholder with the real
> shape from `00a-example-ts-codegen-type-shapes.md` and the `00a-spec`
> appendix:
> - classes → `export class Foo { … constructor(init: {...}) }`, with
>   cross-namespace field types via a single root import
>   `import type * as <rootns> from ".."` and references like
>   `<rootns>.fizz.foo.Bar` (NOT per-leaf flattened imports);
> - free functions → `export const fn = defineFunction("user.<fqn>",
>   "sync"|"async", [argNames]) as (...args) => T` plus the `_async`
>   sibling returning `Promise<T>`;
> - instance methods → class-FIELD initializers `m =
>   defineInstanceFunction("user.<fqn>", "sync", ["self"]).bind(this) as
>   () => T` (not module-level const, not `static readonly`);
> - the five stdlib types (`Image`/`Audio`/`Video`/`Pdf`/`llm.Stream`) →
>   `export { … } from "@boundaryml/baml-core-node"` re-exports, never a
>   generated body.
> All bindings import their helpers (`defineFunction`,
> `defineInstanceFunction`) from `@boundaryml/baml-core-node`. Phase 2 emits
> none of this; it only fixes the file/namespace skeleton these will
> drop into.

### Example: `baml_sdk/vendor/index.ts` (container barrel — no symbols)

```ts
/* eslint-disable */
// prettier-ignore
// ---------------------------------------------------------------------------
// Welcome to Baml! …
// ---------------------------------------------------------------------------

// Re-exports child namespaces ONLY — never `export * from "./aws"`,
// which would flatten aws's symbols into `vendor`.
export * as aws from "./aws";
```

### Example: `baml_sdk/index.ts` (root)

```ts
/* eslint-disable */
// prettier-ignore
// ---------------------------------------------------------------------------
// Welcome to Baml! …
// ---------------------------------------------------------------------------

import { initializeRuntime, setTypeMap, BAML_PLACEHOLDER } from "@boundaryml/baml-core-node";
import * as _inlinedbaml from "./_inlinedbaml";
import { _TYPE_MAP } from "./_typemap";

initializeRuntime("baml_src", _inlinedbaml.FILES);
setTypeMap(_TYPE_MAP);

export * as baml from "./baml";
export * as ipsum from "./ipsum";
export * as lorem from "./lorem";
export * as stream_types from "./stream_types";
export * as vendor from "./vendor";

// class Foo
export const Foo: any = BAML_PLACEHOLDER;
```

### Example: `baml_sdk/_typemap.ts`

```ts
/* eslint-disable */
// prettier-ignore
// ---------------------------------------------------------------------------

import { BamlTypeMap } from "@boundaryml/baml-core-node";

const _CLASS_ENTRIES: Record<string, [string, string]> = {
  "aws.s3.Bucket":         ["baml_sdk/vendor/aws/s3", "Bucket"],
  "baml.http.Response":    ["baml_sdk/baml/http",     "Response"],
  "user.Foo":              ["baml_sdk",               "Foo"],
  "user.lorem.Resume":     ["baml_sdk/lorem",         "Resume"],
  "user.lorem.Resume$stream": ["baml_sdk/stream_types/lorem", "Resume"],
};

const _ENUM_ENTRIES: Record<string, [string, string]> = {
  "user.ipsum.Sentiment":  ["baml_sdk/ipsum",         "Sentiment"],
};

const _ALIAS_ENTRIES: Record<string, [string, string]> = {};

export const _TYPE_MAP = BamlTypeMap.fromLazyEntries({
  classes: _CLASS_ENTRIES,
  enums: _ENUM_ENTRIES,
  typeAliases: _ALIAS_ENTRIES,
});
```

## Phase 2.1 — Routing port

### Changes Required

Port `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/routing.rs`
(`routing.rs:57-110`) to
`/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/routing.rs`.
The routing *namespace* logic is **identical** between languages — both
produce the same `LeafPath { segments }` (`user`→root, `baml`→`["baml"]`,
vendor→`["vendor", pkg]`, `$stream` classes prepended with
`"stream_types"`). The **only** divergence is downstream: how a
`LeafPath` directory is turned into an in-directory filename
(`<segs>/index.ts` vs `<segs>/__init__.py`), which lives in `lib.rs`
(§2.6), not in `routing.rs`. So `routing.rs` ports essentially verbatim.

- Copy `LeafPath`, `route`, `route_class_ref`, `route_inner`,
  `sanitize_python_module_segment` (renamed `sanitize_module_segment` for
  general framing; the body remains "only handle 'assert'"). The
  `LeafPath::init_py` / `is_root` test helpers should become
  `init_ts` (returning `<segs…>/index.ts`, or `index.ts` for the root) +
  `is_root` — exactly mirroring Python's `init_py` directory layout, with
  only the in-directory filename changed (`__init__.py` → `index.ts`).
- Keep all unit tests, except: `assert_namespace_segment_is_sanitized`
  (`routing.rs:305-312`) drops because `assert` is not a JS reserved
  word. Replace it with a `keyword_does_not_collide` test that uses a
  non-keyword segment to pin the no-op identity behavior. (Per A11.)
  Any test asserting `init_py()` paths reshapes to assert `segments`
  directly (the file-naming is verified in `lib.rs` tests, §2.6).
- Update doc comments to say "TypeScript" not "Python".
- Add module to `sdkgen_nodejs/src/lib.rs` (`mod routing;`).

### Success Criteria

**Automated:**
- `cd /Users/sam/baml3/baml_language && cargo test -p sdkgen_nodejs routing::tests` — all routing tests green.
- `cd /Users/sam/baml3/baml_language && cargo build -p sdkgen_nodejs` — compiles.

**Manual:**
- Spot-check that the file layout described under `## File layout
  produced by Phase 2` above would be the routed paths returned by
  `route(...)` for each fixture symbol.

## Phase 2.2 — Per-kind emitter scaffolding

### Changes Required

Create the emitter mod tree under
`/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/emit/`:

- `mod.rs` with `EmittedSymbol`, `SortKey`, `build_emitted`. Port the
  Python `expand_function`, `expand_methods`, `expand_callable`,
  `bare_callable_name`, `origin_key` functions **verbatim** — the
  fan-out logic (sync + async per call, companion suffix rules) is
  language-agnostic. Drop `arg_tys` / `arg_defaults` / `return_ty` /
  `generic_params` / `docstring` fields from the new Node structs in
  Phase 2 — they'll come back in Phase 3/4.
- `class.rs`: `NodeClass`, `NodeClassProperty`. Phase 2 fields:
  `name`, `source` (kept for typemap registration), `is_stream`
  (computed). Properties are kept but unused by Phase 2's renderer.
- `enum_.rs`: `NodeEnum`, `NodeEnumVariant`. Phase 2 fields: `name`,
  `source`, variants kept for use in Phase 4.
- `type_alias.rs`: `NodeTypeAlias`. Phase 2 fields: `name`, `source`.
  `resolves_to` and `recursive` are dropped in Phase 2 — added back
  alongside `translate_ty` in Phase 3.
- `function.rs`: `NodeFunction`, `SyncAsync`. Phase 2 fields:
  `name` (the bare identifier including `_async` / `_stream` suffix
  per `bare_callable_name`), `baml_fqn`, `mode`.
- `method.rs`: `NodeMethodBinding`, `MethodKind`. Phase 2 fields:
  `name`, `baml_fqn`, `mode`, `kind`. (`param_names` and `arg_tys`
  drop in Phase 2.)
- `typemap_file.rs`: see sub-phase 2.4.

In `build_emitted`, the per-symbol walk produces `EmittedSymbol::Class/Enum/TypeAlias/Function`
entries — one entry per symbol, **except** functions which fan out 2x via
`expand_callable` (sync + async). Companions ride through `expand_function`
naturally because they arrive in the pool as their own `Symbol::Function`
entries with `$<suffix>` names.

### Success Criteria

**Automated:**
- `cargo test -p sdkgen_nodejs emit::tests` — at this point the
  emitter module compiles but has no behavior tests of its own (those
  live in `lib.rs` tests sub-phase 2.6).
- `cargo build -p sdkgen_nodejs` clean.

**Manual:**
- The `NodeFunction` struct has just enough state to render the placeholder line.

## Phase 2.3 — Leaf renderer (placeholder output)

### Changes Required

Create `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/leaf.rs`:

- Define `LeafBody { leaf: LeafPath, symbols: Vec<(EmittedSymbol, SortKey)> }`.
- Port `group_and_sort` from `codegen_python/src/leaf.rs:527-579` — same
  primary `(file, span)` sort, same `symbol_kind_ord` tertiary tie-break
  (`TypeAlias` last so any forward references resolve). Drop the
  recursive-alias hoist (Phase 3 territory).
- Implement `render_leaf_body(body: &LeafBody) -> String` that emits the
  per-symbol placeholder lines. Each symbol renders:

  | EmittedSymbol | Lines emitted (`.ts`) |
  | --- | --- |
  | `Class(NodeClass { name, .. })` | `// class <name>\nexport const <name>: any = BAML_PLACEHOLDER;\n` |
  | `Enum(NodeEnum { name, .. })`   | `// enum <name>\nexport const <name>: any = BAML_PLACEHOLDER;\n` |
  | `TypeAlias(NodeTypeAlias { name, .. })` | `// type <name>\nexport const <name>: any = BAML_PLACEHOLDER;\n` |
  | `Function(NodeFunction { name, baml_fqn, mode, .. })` | `// function <baml_fqn> (<mode>)\nexport const <name>: any = BAML_PLACEHOLDER;\n` |

  Methods don't render as top-level lines — they live inside their
  parent class's body. In Phase 2 we elide them entirely because the
  parent class is also a placeholder; Phase 4 will surface methods
  inside the real class declaration.
- **Stdlib re-export special-case (A13).** Before rendering a `Class`
  placeholder, check whether its source FQN is one of the five
  runtime-owned types (`baml.media.{Image,Audio,Video,Pdf}`,
  `baml.llm.Stream`). If so, emit a re-export line
  (`export { BamlImage as Image } from "@boundaryml/baml-core-node";` per `00a-example`
  §"Stdlib Re-Exports") **instead of** a `BAML_PLACEHOLDER` body — these
  must resolve to the runtime class identity, not a sentinel. All other
  classes get the normal placeholder. (Body details — snake_case static
  aliases, `_fromHandle` — are Phase 4 / `10a-todo-items.md` §B3; Phase 2
  only needs the re-export vs. placeholder branch.)
- Implement `render_leaf_body_dts(body: &LeafBody) -> String` that emits
  the same lines but with `: any;` instead of `: any = BAML_PLACEHOLDER;`.
- Add a one-line `import { BAML_PLACEHOLDER } from "@boundaryml/baml-core-node";`
  at the top of `.ts` *leaf* files that contain ANY placeholder. `.d.ts`
  files don't need the import (type-only). Container `index.ts` files
  carry no placeholders (re-exports only) and so never import
  `BAML_PLACEHOLDER`.

### Example output

For a leaf with one class and one (free) function (BAML
`class Resume; function extract_resume() -> Resume`):

```
import { BAML_PLACEHOLDER } from "@boundaryml/baml-core-node";

// class Resume
export const Resume: any = BAML_PLACEHOLDER;

// function user.lorem.extract_resume (sync)
export const extract_resume: any = BAML_PLACEHOLDER;
// function user.lorem.extract_resume (async)
export const extract_resume_async: any = BAML_PLACEHOLDER;
```

### Success Criteria

**Automated:**
- A unit test in `leaf.rs` (mirror Python's `class_body_renders` /
  `enum_body_renders` shape) that constructs a one-symbol `LeafBody`
  and asserts the rendered string is exactly the placeholder shape.
- `cargo test -p sdkgen_nodejs leaf::` passes.

**Manual:**
- Eyeball the rendered string for one class + one function.

## Phase 2.4 — Typemap file emission

### Changes Required

Create `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/emit/typemap_file.rs`:

- Port `render_typemap_module` from
  `codegen_python/src/emit/typemap_file.rs:28-76`. The function takes
  `&BTreeMap<LeafPath, LeafBody>` + `sdk_root: &str` (`"baml_sdk"`).
- `module_path_for_leaf(leaf, sdk_root)` returns a forward-**slash** path
  string — `baml_sdk/lorem` for the `lorem` leaf,
  `baml_sdk/vendor/aws/s3` for `aws.s3` — joining `sdk_root` with the
  leaf segments by `/`. This is the **divergence from Python**, whose
  `module_path_for_leaf` (`typemap_file.rs:78-92`) joins with `.` for a
  dotted import path. The slash form resolves to the directory's
  `baml_sdk/lorem/index.ts` via CommonJS directory `require`. Root leaf
  returns `"baml_sdk"`.
- The body shape is per A6 above. Each entry line is:
  ```
  "<source FQN>": ["<module_path>", "<name>"],
  ```
  rendered via a small `ts_string` helper (port `py_string`,
  `lib.rs:400-418`, using JS string-escape rules — same `\\`, `\"`,
  `\n`, `\r`, `\t`, `\x` hex; JS strings are byte-compatible with
  Python's form for the ASCII range). The header is
  `import { BamlTypeMap } from "@boundaryml/baml-core-node";` (Node) in place
  of Python's `from baml_core import BamlTypeMap`; the installer call is
  `BamlTypeMap.fromLazyEntries({ classes, enums, typeAliases })` in
  place of `BamlTypeMap.from_lazy_entries(classes=…, …)`.
- Functions are NOT in the typemap (mirrors Python — the `match` skips
  `EmittedSymbol::Function`, `typemap_file.rs:49`).

### Success Criteria

**Automated:**
- A `cargo test` covering: (a) empty pool yields three empty record
  literals plus the `fromLazyEntries` call; (b) class / enum / alias
  routed to nested leaf emits the right `module_path` (e.g.
  `baml_sdk/vendor/aws/s3` for `aws.s3.Bucket`); (c) stream class
  routes to `baml_sdk/stream_types/lorem`; (d) sort order is
  deterministic across runs.

**Manual:**
- Hand-trace the `lorem.Resume` fixture through to the emitted
  `_typemap.ts` text and confirm it matches the §A6 example.

## Phase 2.5 — Runtime `BamlTypeMap` and `BAML_PLACEHOLDER` exports (bridge_nodejs)

### Changes Required

This is the only Phase 2 work that touches `bridge_nodejs`. It adds
exports the generated code depends on. No native (NAPI / Rust) changes —
everything is in `typescript_src/`.

1. Create `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/typemap.ts`:
   ```ts
   import { BamlError } from "./errors";

   export type LazyEntry = [string, string]; // [modulePath, attrName]

   // Hard-coded stdlib reverse-overrides. Mirrors
   // _STDLIB_REVERSE_OVERRIDES in baml_core/typemap.py.
   //
   // Keys are [modulePath, exportName] of the *native* class
   // identities that the codegen-emitted re-exports point at. Phase 4
   // will fill in the actual native identities; Phase 2 leaves the
   // table empty and notes the design intent in a comment.
   const _STDLIB_REVERSE_OVERRIDES: Map<string, string> = new Map([
     // [["@boundaryml/baml-core-node", "BamlImage"], "baml.media.Image"], — wired up in Phase 4
     // …
   ]);

   export class BamlTypeMap {
     private classLazy = new Map<string, LazyEntry>();
     private enumLazy = new Map<string, LazyEntry>();
     private aliasLazy = new Map<string, LazyEntry>();
     private classCache = new Map<string, any>();
     private enumCache = new Map<string, any>();
     private aliasCache = new Map<string, any>();
     private reverse: Map<string, string> = new Map(_STDLIB_REVERSE_OVERRIDES);

     static fromLazyEntries(args: {
       classes: Record<string, LazyEntry>;
       enums: Record<string, LazyEntry>;
       typeAliases: Record<string, LazyEntry>;
     }): BamlTypeMap {
       const m = new BamlTypeMap();
       for (const [fqn, le] of Object.entries(args.classes))      m.classLazy.set(fqn, le);
       for (const [fqn, le] of Object.entries(args.enums))        m.enumLazy.set(fqn, le);
       for (const [fqn, le] of Object.entries(args.typeAliases))  m.aliasLazy.set(fqn, le);
       for (const [fqn, [mp, attr]] of Object.entries(args.classes)) {
         const k = `${mp}::${attr}`;
         if (!m.reverse.has(k)) m.reverse.set(k, fqn);
       }
       for (const [fqn, [mp, attr]] of Object.entries(args.enums)) {
         const k = `${mp}::${attr}`;
         if (!m.reverse.has(k)) m.reverse.set(k, fqn);
       }
       return m;
     }

     getClass(fqn: string): any {
       const cached = this.classCache.get(fqn);
       if (cached !== undefined) return cached;
       const entry = this.classLazy.get(fqn);
       if (entry === undefined) throw new BamlError(`Unknown class FQN ${fqn}`);
       const [modulePath, attr] = entry;
       const mod = require(modulePath);
       const cls = mod[attr];
       if (cls === undefined) {
         throw new BamlError(`Could not resolve ${fqn} → ${modulePath}.${attr}`);
       }
       this.classCache.set(fqn, cls);
       return cls;
     }

     getEnum(fqn: string): any { /* same shape */ }
     getTypeAlias(fqn: string): any { /* same shape */ }

     jsTypeToBamlType(cls: any): string {
       // Walk the prototype chain. Returns "" if no match. Phase 5
       // refines this for class identity.
       let cur = cls;
       while (cur != null) {
         const name = cur?.name;
         const mod = cur?.__bamlModulePath;
         if (mod && name) {
           const fqn = this.reverse.get(`${mod}::${name}`);
           if (fqn !== undefined) return fqn;
         }
         cur = Object.getPrototypeOf(cur);
       }
       return "";
     }

     warm(): void {
       for (const k of this.classLazy.keys()) this.getClass(k);
       for (const k of this.enumLazy.keys()) this.getEnum(k);
       for (const k of this.aliasLazy.keys()) this.getTypeAlias(k);
     }
   }

   let _TYPE_MAP = new BamlTypeMap();
   export function setTypeMap(m: BamlTypeMap): void { _TYPE_MAP = m; }
   export function getTypeMap(): BamlTypeMap { return _TYPE_MAP; }
   ```
2. Edit `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts`
   (which today exports `BamlRuntime`, `AbortController`, `BamlHandle`,
   `getVersion`, `flushEvents`, `encodeCallArgs`, `decodeCallResult`,
   `CtxManager`, and the `BamlError` family, but none of the codegen
   names):
   - Add `export const BAML_PLACEHOLDER: any = Object.freeze({ __bamlPlaceholder: true });`
   - Add `export { BamlTypeMap, setTypeMap, getTypeMap } from "./typemap";`
   - Note: the Phase-4 callable surface (`defineFunction`,
     `defineInstanceFunction`) is referenced by the *real* generated
     leaf bodies in `00a-example-ts-codegen-type-shapes.md`, but Phase 2
     leaves emit only `BAML_PLACEHOLDER` and so do NOT import
     `defineFunction`. Adding/exporting `defineFunction` is Phase 4 work;
     do not add it here unless a Phase-2 leaf actually references it (it
     does not).
3. **Guarded `initializeRuntime` named export.** The `00a-example` root
   snippet calls a free `initializeRuntime("baml_src", _inlinedbaml.FILES)`
   imported from `@boundaryml/baml-core-node`. If Phase 1 has not yet exported
   such a free function (today `index.ts` re-exports `BamlRuntime` from
   `./native` but no free `initializeRuntime`), sub-phase 2.5 adds a thin
   pass-through:
   ```ts
   // In typescript_src/index.ts, near the BamlRuntime re-export
   export function initializeRuntime(srcDir: string, files: Record<string, string>): void {
     // calls the underlying native initializer; exact symbol depends on
     // Phase 1's NAPI design. Document the dependency clearly in a TODO.
   }
   ```
   so the codegen root template's `initializeRuntime(...)` resolves.
   **Assumption: Phase 1 lands the named export by the time Phase 2
   ships, so the generated root uses `initializeRuntime` directly.** If
   not, the wrapper above is the one-line bridge in the
   `render_root_init` template.

### Success Criteria

**Automated:**
- Add a jest test
  `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/tests/test_typemap.test.ts`
  that:
  - Constructs an empty `BamlTypeMap` and asserts `getClass("foo")`
    throws `BamlError`.
  - Constructs a populated one with one entry pointing at this test
    file and asserts the import roundtrips.
  - Confirms `BAML_PLACEHOLDER` is truthy and is `Object.frozen`.
- Run `cd /Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs && pnpm test`
  (or `cargo test -p bridge_nodejs` if the existing test harness
  goes via Rust).

**Manual:**
- Confirm `index.ts` exports the new names: `BAML_PLACEHOLDER`,
  `BamlTypeMap`, `setTypeMap`, `getTypeMap`.

## Phase 2.6 — `to_source_code` entrypoint and file-tree wiring

### Changes Required

Replace the body of
`/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs` with the real
emitter. Port the Python `lib.rs` top-level structure:

1. `to_source_code(pool, user_baml_files, naming_convention) -> HashMap<PathBuf, String>`
   (matching Python's signature, `lib.rs:93-97`, and the current Node
   stub `lib.rs:18-26` — **`HashMap`, not `IndexMap`**):
   - Assert `naming_convention == PreserveCase` (A12), as Python
     (`lib.rs:100-104`).
   - Walk `pool` to collect routed leaves (`route(key, sym)`), `lib.rs:109-112`.
   - Force-insert root leaf and `["baml"]` leaf so they're always emitted
     (mirrors `lib.rs:119-124`).
   - Walk every leaf's ancestor chain to discover all directories and
     populate `all_dirs` + the `children` map (`lib.rs:133-149`).
   - Call `build_emitted(pool)` → triples → `group_and_sort` → `bodies`
     (`lib.rs:154-155`).
   - **Emit one `<dir>/index.ts` + `<dir>/index.d.ts` per directory.**
     This is a near-direct port of Python's directory-per-leaf loop
     (`lib.rs:158-189`); the only change is `__init__.py(i)` → `index.ts(.d.ts)`
     as the in-directory filename. For each `dir` in `all_dirs`:
     - If `dir` is a **routed leaf** (it appears in `leaves`), the
       `index.ts` body = `render_leaf_body(body)` (the symbol
       placeholders). If that leaf *also* has children
       (`children[dir]` non-empty, e.g. a `stream_types/` that both
       routes symbols and nests child namespaces), prepend the child
       re-exports to the same `index.ts` — the merged case, matching
       Python's merged `__init__.py`.
     - If `dir` is a **container** (root, or a proper-prefix directory
       that is not itself a routed leaf), the `index.ts` body =
       `render_package_index(children)` (root uses
       `render_root_index(children)`), with NO leaf body — containers
       carry only child re-exports, never symbols.
     - The root (`segments: []`) always emits `index.ts` +
       `index.d.ts` directly under `baml_sdk/`.
   - Root-only files (at `baml_sdk/`):
     - `_inlinedbaml.ts` via `render_inlinedbaml(user_baml_files)`
       (`lib.rs:196-199`).
     - `_typemap.ts` via `render_typemap_module(&bodies, "baml_sdk")`
       (`lib.rs:208-211`).
     - **No `py.typed` analog** (A7) — Python emits one at `lib.rs:215`;
       Node emits nothing.
   - Prepend `NODE_BANNER` to every `.ts` / `.d.ts` file (mirrors the
     Python `for (path, content)` banner loop, `lib.rs:218-226`).
2. `render_package_index(children)` — container barrel, child re-exports
   ONLY (A3):
   ```rs
   fn render_package_index(children: &BTreeSet<String>) -> String {
       let mut out = String::new();
       for c in children {
           // `export * as` — namespace-preserving. NEVER `export * from`
           // (which would flatten the child's symbols into this parent).
           writeln!(out, "export * as {c} from \"./{c}\";").unwrap();
       }
       out
   }
   ```
   No `index.d.ts` divergence — same shape for both (TS infers types
   from the re-export naturally; the `.d.ts` is byte-identical here
   since `export * as` is purely a re-export of bindings/types). Every
   child `c` is a directory (`./c` → `c/index.ts`); CommonJS directory
   resolution handles it uniformly.
3. `render_root_index(children)` — mirrors Python `render_root_init`
   (`lib.rs:325-339`), minus the PEP 562 lazy-children machinery (TS
   `export * as` is eager and sufficient). Note: the root does NOT build
   an `export const b` client object (A4):
   ```rs
   const ROOT_PREAMBLE: &str = "\
   import { initializeRuntime, setTypeMap, BAML_PLACEHOLDER } from \"@boundaryml/baml-core-node\";\n\
   import * as _inlinedbaml from \"./_inlinedbaml\";\n\
   import { _TYPE_MAP } from \"./_typemap\";\n\n\
   initializeRuntime(\"baml_src\", _inlinedbaml.FILES);\n\
   setTypeMap(_TYPE_MAP);\n\n";
   ```
   followed by the same children re-exports, then the root leaf's own
   root-namespace placeholders (`render_leaf_body` of the root body).
4. `render_inlinedbaml(files)`: port the Python askama template
   (`lib.rs:356-395`) or an equivalent `fmt::Write` loop, shape per A5.
   If askama is used, add it to `sdkgen_nodejs/Cargo.toml` (it is NOT a
   current dep — the crate only declares `baml_codegen_types`).
5. `ts_string(s)`: port `py_string` (`lib.rs:400-418`). JS string
   escaping rules are essentially identical (`\\`, `\"`, `\n`, `\r`,
   `\t`, `\xNN`).
6. **Drop the `catch_unwind` wrapper** in
   `/Users/sam/baml3/baml_language/sdk_tests/harness_setup/src/nodejs_typescript.rs`
   (`nodejs_typescript.rs:173-205` — the old `sdk_tests/build/src/nodejs.rs`
   path in earlier drafts no longer exists). Today the `Ok(output)` arm
   already runs `for (rel, content) in output { … fs::write … }`; the
   `Err(_)` arm records a `"codegen"` diagnostic and leaves `baml_sdk/`
   empty. Replace the whole `panic::catch_unwind(...)` + `match` with a
   direct call + the existing `Ok`-arm loop, no panic capture. Update the
   surrounding doc comment (`nodejs_typescript.rs:6-8`, 59-94) to remove
   the "stub survival" rationale.
7. Add `// generated by sdkgen_nodejs — do not edit` headers as needed
   on per-file emission (the `NODE_BANNER` already covers this; no extra
   per-file header needed).

### Tests to port to `lib.rs::tests`

Mirror the Python `lib.rs` test names; the assertions reshape to TS:

- `empty_pool_emits_structural_files` — assert `index.ts`, `index.d.ts`,
  `_inlinedbaml.ts`, `_typemap.ts`, `baml/index.ts` exist. Root contains
  `import { initializeRuntime, setTypeMap, BAML_PLACEHOLDER } from "@boundaryml/baml-core-node"`,
  `initializeRuntime("baml_src", _inlinedbaml.FILES);`, `setTypeMap(_TYPE_MAP);`,
  `export * as baml from "./baml";`. `baml/index.ts` is a container barrel
  with no exports beyond the banner (no children → empty). The root does
  NOT contain `export const b`.
- `class_body_renders` — assert `// class Resume\nexport const Resume: any = BAML_PLACEHOLDER;\n`
  appears in the leaf file `lorem/index.ts`.
- `enum_body_renders` — assert `// enum Sentiment\n…` appears in `ipsum/index.ts`.
- `type_alias_body_renders` — assert `// type Foo\n…` appears.
- `function_fans_out_sync_and_async` — assert both
  `export const extract_resume: any = BAML_PLACEHOLDER;` and
  `export const extract_resume_async: any = BAML_PLACEHOLDER;` lines
  appear contiguously.
- `function_with_stream_companion` — assert `extract_resume_stream` and
  `extract_resume_stream_async` appear.
- `function_with_build_request_companion_uses_double_underscore` —
  assert `extract_resume__build_request` and
  `extract_resume__build_request_async`.
- `stream_class_routes_to_stream_types` — assert the leaf file
  `stream_types/lorem/index.ts` contains the `Resume` placeholder (the
  `$stream` suffix is stripped per the `bare_callable_name` analog;
  the `_typemap.ts` entry however keeps the suffix in the FQN key and
  the slash module path: `"user.lorem.Resume$stream": ["baml_sdk/stream_types/lorem", "Resume"]`).
- `leaf_namespace_is_directory_with_index` — **new test** pinning the
  directory-per-namespace rule: assert `lorem/index.ts` exists and a
  flat `lorem.ts` does NOT.
- `container_index_only_reexports_children` — **new test**: assert
  `vendor/index.ts` contains `export * as aws from "./aws";` and contains
  no `export const` / `export class` / `export * from` line (no symbols,
  no flattening).
- `vendor_creates_interior_containers` — assert `vendor/index.ts` and
  `vendor/aws/index.ts` exist as containers, and the leaf
  `vendor/aws/s3/index.ts` exists; the intermediate containers
  contain `export * as aws from "./aws";` / `export * as s3 from "./s3";`.
- `root_stub_populates_root_index` — root `index.ts` contains
  `initializeRuntime(...)` AND the placeholder lines for
  any root-namespace (`user.*` no-ns) symbols, emitted as plain
  `export const` (not members of a `b` object).
- `inlinedbaml_round_trips` — assert key-sorting and contents match
  expected.
- `ts_string_escapes` — assert `ts_string` matches expected escape rules
  (parallel to `py_string_escapes`).

### Success Criteria

**Automated:**
- `cargo nextest run -E 'package(sdkgen_nodejs)'` — full unit test
  suite green.
- `cargo nextest run -E 'package(sdk_test_nodejs_typescript) and test(tsc)'` —
  all three fixtures' `tsc --noEmit` passes.
- `cargo nextest run -E 'package(sdk_test_nodejs_typescript) and test(jest)'` —
  expected partial pass:
  - `type_shapes`: namespace imports + "Foo is reachable" / "Resume
    is reachable" pass (BAML_PLACEHOLDER is truthy, `toBeDefined`
    succeeds).
  - `llm_functions`: namespace imports pass, factory `typeof ===
    "function"` assertions FAIL (placeholder is an object, not a
    function — closes in Phase 4).
  - `docstrings_etc`: namespace imports pass, `Sentiment` enum-key
    assertion FAILS (placeholder isn't an enum — closes in Phase 4).
- `cargo build -p sdk_test_nodejs_typescript` clean (verifies
  `catch_unwind` removal is well-formed).

**Manual:**
- Inspect `sdk_tests/crates/nodejs/generated/type_shapes/baml_sdk/` and
  confirm it matches the §"File layout produced by Phase 2" example.
- Run `pnpm exec tsc --noEmit` manually inside
  `generated/type_shapes/` and confirm no errors.
- Run `pnpm exec jest` manually and visually confirm which tests pass
  vs. fail vs. expected.

## Testing Strategy

### Cargo unit tests (per crate)

- `sdkgen_nodejs::routing::tests` — sub-phase 2.1.
- `sdkgen_nodejs::leaf::tests` — sub-phase 2.3.
- `sdkgen_nodejs::emit::typemap_file::tests` — sub-phase 2.4.
- `sdkgen_nodejs::tests` (lib.rs) — sub-phase 2.6 end-to-end emitter tests.

Run all: `cargo nextest run -p sdkgen_nodejs`.

### Cargo end-to-end tests (sdk_tests)

- `sdk_test_nodejs_typescript::type_shapes::tsc` — passes after sub-phase 2.6.
- `sdk_test_nodejs_typescript::type_shapes::jest` — partial pass after 2.6 (closes
  fully after Phase 4).
- Same for `docstrings_etc` and `llm_functions`.

Run all: `cargo nextest run -p sdk_test_nodejs_typescript`.

### Bridge_nodejs jest tests (sub-phase 2.5)

- `bridge_nodejs/tests/test_typemap.test.ts` — new, covers
  `BamlTypeMap`, `setTypeMap`, `getTypeMap`, `BAML_PLACEHOLDER`.
- Existing `test_engine.test.ts` / `test_collector.test.ts` /
  `call_function.test.ts` / `test_tracing.test.ts` must still pass —
  sub-phase 2.5 only adds exports, doesn't change existing behavior.

Run: `cd /Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs && pnpm test`.

### Phase 2 "stable red" baseline

After Phase 2 completes, this is the expected state of
`cargo nextest run -E 'package(sdk_test_nodejs_typescript)'`:

| Test | Status |
| --- | --- |
| `type_shapes::tsc` | PASS |
| `type_shapes::jest` | PASS (placeholder values are truthy) |
| `docstrings_etc::tsc` | PASS |
| `docstrings_etc::jest` | FAIL (Sentiment.HAPPY enum-key assertion) |
| `llm_functions::tsc` | PASS |
| `llm_functions::jest` | FAIL (typeof ExtractResume === "function") |

The failing jest tests close in Phase 4 when placeholder lines become
real enums / factory functions. Until then they document the missing
behavior.

## References

### Spec docs
- `/Users/sam/thoughts/sam-projects/bridge-node/00b-overview.md` — phase list
- `/Users/sam/thoughts/sam-projects/bridge-node/00b2-overview.md` — Python prior-art cross-reference
- `/Users/sam/thoughts/sam-projects/bridge-node/00a-spec-codegen-mappings.md` — codegen rules (the canonical rule source)

### Python prior art (read these alongside this plan)
- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/lib.rs` — entry point, root init, `_inlinedbaml`, banner
- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/routing.rs` — routing rules, ported verbatim
- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/emit/mod.rs` — `build_emitted`, fan-out
- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/emit/typemap_file.rs` — `_typemap.py` renderer
- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/emit/class.rs` — `PyClass` shape (Node strips fields for Phase 2)
- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/emit/{enum_,function,method,type_alias}.rs` — corresponding shape references
- `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/typemap.py` — runtime `BamlTypeMap` class

### Node target files (new or modified)
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs` — entry point
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/routing.rs` — new
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/leaf.rs` — new
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/emit/mod.rs` — new
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/emit/{class,enum_,function,method,type_alias,typemap_file}.rs` — new
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/Cargo.toml` — add `askama` workspace dep if used for `_inlinedbaml` template
- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/typemap.ts` — new
- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts` — re-exports `BamlTypeMap`, `setTypeMap`, `getTypeMap`, `BAML_PLACEHOLDER`
- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/tests/test_typemap.test.ts` — new
- `/Users/sam/baml3/baml_language/sdk_tests/harness_setup/src/nodejs_typescript.rs` — drop `catch_unwind` (`nodejs_typescript.rs:173-205`; the old `sdk_tests/build/src/nodejs.rs` path no longer exists)

### Test fixtures (read-only, drive the test harness)
- `/Users/sam/baml3/baml_language/sdk_tests/fixtures/type_shapes/baml_src/` — exhaustive type-shape fixture
- `/Users/sam/baml3/baml_language/sdk_tests/fixtures/llm_functions/baml_src/` — LLM function + companion fixture
- `/Users/sam/baml3/baml_language/sdk_tests/fixtures/docstrings_etc/baml_src/` — docstring fixture
- `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs_typescript/type_shapes/customizable/main.test.ts` — import-resolution targets
- `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs_typescript/llm_functions/customizable/main.test.ts` — factory + companion targets
- `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs_typescript/docstrings_etc/customizable/main.test.ts` — enum-shape + import targets
