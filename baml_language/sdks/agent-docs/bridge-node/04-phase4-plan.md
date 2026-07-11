# Phase 4 Plan: Fill in Codegen (sdkgen_nodejs)

## Overview

Phase 4 replaces the Phase 2 scaffolding placeholders inside `sdkgen_nodejs` with real TypeScript bodies for every top in the TIR: type aliases, enums, classes (including `$stream` companions), and function bindings (free functions, static methods, instance methods). It also wires in cross-leaf imports, root `index.ts` re-exports, JSDoc/TSDoc docstring lowering from BAML `///` comments, and the codegen-emitted `_typemap.ts` registrations. After Phase 4 runs, every BAML symbol that Phase 2 routed to a leaf has a typed, importable TS definition; `tsc --noEmit` passes for every fixture, and the non-LLM jest suites pass.

Phase 4 does **not** touch the runtime bridge (Phase 1), the routing/scaffolding code (Phase 2), the `translate_ty` infrastructure (Phase 3), the proto encode/decode logic (Phase 5), or release pipelines (Phase 6).

## Goal

Delivery criteria:

1. For every BAML top (type alias, enum, class, free function, static/instance method, `$stream`/`$build_request`/`$render_prompt`/`$parse`/`$parse_stream` companion), `sdkgen_nodejs::to_source_code` emits a syntactically valid, type-checked TS definition in two files: a `.ts` carrying the runtime + types, and a `.d.ts` carrying the slim public type surface. No placeholder `// class Resume` comments survive in either file.
2. Cross-leaf type references compile: a class field typed `Resume` in leaf `aliases_consumer` correctly imports `Resume` from leaf `lorem`. The `.ts` uses `import { Resume }`; the `.d.ts` uses `import type { Resume }`.
3. `cargo nextest run -E 'package(sdk_test_nodejs) & test(/::tsc$/)'` passes on every fixture (`type_shapes`, `docstrings_etc`, `llm_functions`). `tsc` reads both the `.ts` and the `.d.ts` siblings.
4. `cargo nextest run -E 'package(sdk_test_nodejs) & test(/::jest$/)'` passes on `type_shapes` and `docstrings_etc`. `llm_functions::jest` may have a *subset* still red — only the round-tripping cases that depend on Phase 5's proto encoder/decoder. The reachability / typeof / enum-member-shape cases that already exist in `customizable/llm_functions/main.test.ts` must all pass.
5. `_typemap.ts` lists every class, enum, and type-alias top with the correct `module_path` and `attr_name`; the SDK root `index.ts` calls `BamlRuntime.initialize_runtime(...)` and `setTypeMap(_TYPE_MAP)` exactly like the Python root `__init__.py` does. The `_typemap.d.ts` declares only the public `_TYPE_MAP: BamlTypeMap` surface; the `index.d.ts` carries re-exports only (no runtime bootstrap).
6. Every emitted `.d.ts` is free of runtime constructs: no `defineFunction`, no `defineInstanceFunction`, no `Object.assign`, no `.bind(this)`, no `BamlRuntime.initializeRuntime`. A grep over `generated/*/baml_sdk/**/*.d.ts` for any of those strings must return zero hits.

## Current State Analysis (after Phases 1-3)

### What Phase 1 has delivered (assumed; if missing, Phase 4 stops with a clear panic)

The runtime bridge package `@boundaryml/baml-core-node` (Rust crate `bridge_nodejs`) exports the following from `bridge_nodejs/typescript_src/index.ts`:

- `BamlRuntime` (singleton accessor pattern: `BamlRuntime.initializeRuntime(rootDir, inlinedFiles)`, `getRuntime(): BamlRuntime`)
- `BamlHandle` (handle table wrapper analog of `BamlPyHandle`)
- `AbortController`
- `Collector`, `FunctionLog`, `Timing`, `Usage`, `LLMCall`
- `CtxManager`, `HostSpanManager`
- `BamlError`, `BamlInvalidArgumentError`, `BamlClientError`, `BamlCancelledError`
- `encodeCallArgs`, `decodeCallResult` (Phase 5 fills these in; Phase 4 only needs the imports to resolve)
- `flushEvents`, `wrapNativeError`

**Phase 4 also depends on four Phase 1 additions** that are not yet in `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/`. Phase 4 assumes Phase 1 has added them; if not, Phase 4 either stubs them locally inside the generated SDK or panics:

- **`defineFunction(bamlFqn: string, mode: "sync" | "async", paramNames: readonly string[], requiredPositionalCount?: number): (...args: unknown[]) => unknown`** — analog of Python's `define_function` factory at `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/__init__.py:199-237`. Captures `paramNames` by closure; returned callable accepts mixed positional + keyword args (TS analog: positional `unknown[]` plus a final `Record<string, unknown>` for keyword args; codegen always emits keyword-style call sites so the positional path is rarely hit). Encodes via `encodeCallArgs`, awaits `BamlRuntime.callFunction(Sync)?`, decodes via `decodeCallResult`. Returns `Promise<unknown>` for `async`, `unknown` for `sync`. Used for free functions and static methods.
- **`defineInstanceFunction(bamlFqn: string, mode: "sync" | "async", paramNames: readonly string[]): { bind(self: unknown): (...args: unknown[]) => unknown }` (a `this`-bindable factory)** — the receiver-binding flavor used for instance methods. `paramNames[0]` is always `"self"`. The codegen emits the binding as a class-field initializer `name = defineInstanceFunction(...).bind(this) as () => T;` inside the class body, so `.bind(this)` captures the receiver at construction time; the synthetic `self` param never appears in the surface type, `wrapper.get_value()` works, and a detached `const f = wrapper.get_value` still carries its receiver. ⚠ Spec note: the Python prior art exposes a *single* `define_function` factory and binds instance methods as plain class-body attributes (`get_value = _define_function("…get_value", "sync", ["self"])` — see `generics/__init__.py:34-37`); Python's descriptor protocol injects `self`. TS has no descriptor protocol, so the spec (`00a-example-ts-codegen-type-shapes.md:360-394,463`) introduces a distinct `defineInstanceFunction(...).bind(this)` to capture the receiver. `defineInstanceFunction` is a **separate named export** of `@boundaryml/baml-core-node` (distinct from `defineFunction`); the codegen contract is the class-field `.bind(this)` form.
- **`BamlStream<TStream, TFinal>`** — analog of `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/_stream.py`. Generic class holding a `BamlHandle`; exposes `next()`, `nextAsync()`, `final()`, `finalAsync()` that round-trip through `BamlRuntime.callFunction*("baml.llm.Stream.next" | "baml.llm.Stream.final", { self: this })`.
- **`setTypeMap(typeMap: BamlTypeMap)`** + **`BamlTypeMap.fromLazyEntries({ classes, enums, typeAliases })`** — analog of `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/typemap.py`. `classes` / `enums` / `typeAliases` are `Record<string, [modulePath: string, attrName: string]>` — the same `FQN → (modulePath, attrName)` lazy-entry shape Python uses.

**Assumption A1 (documented for the implementer):** If Phase 1 has not yet shipped `defineFunction`, `defineInstanceFunction`, `BamlStream`, or `setTypeMap`/`BamlTypeMap`, Phase 4 implementation will block at sub-phase 4.4 and require coordination with the Phase 1 implementer to land these symbols in `bridge_nodejs/typescript_src/index.ts` (with corresponding `index.d.ts` declarations). The Python prior art at `_stream.py` (99 lines) and `__init__.py:199-237` (≈40 lines) is small enough that, if Phase 1 is incomplete, Phase 4 can include a "Phase 1 patch" sub-phase to add them. Do not invent a private shim inside the generated SDK — these are runtime helpers, not codegen output.

### What Phase 2 has delivered

`sdkgen_nodejs/src/` has grown beyond the single-file stub at `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs`. Phase 2 has scaffolded:

- `sdkgen_nodejs/src/lib.rs` — `to_source_code(pool, user_baml_files, naming_convention) -> IndexMap<PathBuf, String>` with the directory walk, `__init__` analog (root `index.ts` + per-directory `index.ts`), `_inlinedbaml.ts`, `_typemap.ts` skeletons, and the do-not-edit banner.
- `sdkgen_nodejs/src/routing.rs` — `LeafPath`, `route(key, symbol) -> LeafPath`, `route_class_ref(name) -> LeafPath`. `$stream` companions route under `stream_types/`; `$build_request` etc. route alongside parent.
- `sdkgen_nodejs/src/emit/{class,enum_,type_alias,function,method,mod,typemap_file}.rs` — per-kind structs (`TsClass`, `TsClassProperty`, `TsEnum`, `TsEnumVariant`, `TsTypeAlias`, `TsFunction`, `TsMethodBinding`) mirroring the Python emit structs. `build_emitted(pool) -> Vec<(LeafPath, EmittedSymbol, SortKey)>` walks the pool. Phase 2 emits **placeholder** bodies (`// class Resume — Phase 4 fills this in`).
- `sdkgen_nodejs/src/leaf.rs` — `LeafBody`, `group_and_sort`, `render_leaf_body`/`render_leaf_body_dts`. The header/import-block skeleton is in place but symbol bodies are placeholders.

**Assumption A2:** Phase 2 follows the Python module layout exactly for ease of porting. Any divergence (e.g. fewer modules, different struct field names) is noted in the Phase 2 plan and Phase 4 follows whatever Phase 2 actually shipped, treating the Python emitter as the canonical reference.

### What Phase 3 has delivered

`sdkgen_nodejs/src/translate_ty.rs` implements `translate_ty(ty: &Ty, ctx: &TranslateCtx) -> String` which converts every `Ty` variant (per `/Users/sam/thoughts/sam-projects/bridge-node/00a-spec-codegen-mappings.md`'s "Exhaustive TIR Ty conversions" table, with TS mappings instead of Python — e.g. `Ty::Primitive(Int)` → `"number"`, `Ty::List(T)` → `"T[]"`, `Ty::Optional(T)` → `"T | null"`, `Ty::Union([A, B])` → `"A | B"`, `Ty::Map(K, V)` → `"Record<K, V>"`, `Ty::Class(qtn)` → routed dotted name). `TranslateCtx` carries `current_leaf`, `self_ref`, `defer_name_refs`. The translate-side import collection function (`collect_root_imports` analog) is already in place inside `leaf.rs`.

**Phase 4 calls Phase 3's `translate_ty` from every render function** but does not modify it.

### What is still empty as of Phase 4 start

The render functions in `leaf.rs` (`render_symbol`, `render_symbol_dts`) and emit modules return placeholder bodies. After Phase 4 they emit real TS source:

- type aliases → `export type Foo = <RHS>;`
- enums → `export enum Foo { VARIANT = "VARIANT" }`
- classes → `export class Foo { name!: string; constructor(init: {...}) { Object.assign(this, init); } }`
- functions → `export const extract_resume = defineFunction("user.lorem.extract_resume", "sync", ["text"]) as (text: string) => Resume;`
- imports + JSDoc + typemap registrations

## What We're NOT Doing (Phase 5+)

- **No `translate_ty` modifications.** Phase 3 owns it. If a `Ty` variant has the wrong TS form, fix it in Phase 3, not here.
- **No routing or `LeafPath` changes.** Phase 2 owns them. `$stream` routing, `$build_request` routing, root-vs-namespaced placement — all fixed.
- **No `bridge_nodejs/typescript_src/proto.ts` changes.** Phase 5 implements `encodeCallArgs` / `decodeCallResult`. Phase 4's emitted code calls these but their bodies are Phase 5's problem.
- **No actual round-trip tests.** `llm_functions::jest` cases that *invoke* `extract_resume(...)` and decode the result will stay red until Phase 5. Phase 4 only needs reachability + `typeof === "function"` + enum-member-shape cases to pass.
- **No runtime registration / instance validation logic.** TS classes generated here have no per-field runtime validation. Validation happens in `proto.ts::decodeCallResult` at the boundary (Phase 5). The class definitions are purely for TS type safety and `instanceof` checks.
- **No `BamlStream` runtime logic.** Phase 1 ships it.
- **No release packaging.** Phase 6.
- **No `@@dynamic`, `Checked<T>`, `StreamState<T>`.** Dropped per `00a-spec-codegen-mappings.md` Notes section.
- **No `Ty::Future` / `Ty::Type` handling.** These never reach codegen.

## Implementation Approach

The Python emitter at `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/` is the canonical reference. Each Phase 4 sub-phase pairs a kind of top (alias, enum, class, function, method, companion) with the corresponding chunk of `leaf.rs::render_symbol` / `render_symbol_pyi`. The TS output is *analogous* but TS-idiomatic — see "Example Generated Output" below.

**Output file shape per leaf.** Each leaf renders to **two** sibling files: a `.ts` carrying the runtime + types, and a slim `.d.ts` carrying only the public type surface. Phase 2 already scaffolds both files (`render_leaf_body` + `render_leaf_body_dts`); Phase 4 fills in the bodies.

The `.d.ts` is the load-bearing artifact for the consumer: it lists the typed public shape of every top — fields, constructor signatures, static-method signatures, instance-method signatures — and nothing else. It strips every runtime detail (`Object.assign`, `defineFunction(...)` / `defineInstanceFunction(...)` call sites, `.bind(this)`, the `as (text: string) => Resume` type-assertion plumbing). Hover-docs and "go to type" navigation read from the `.d.ts`, so it stays clean of generator noise.

The `.ts` is the runtime: it contains the same set of exports but with the bodies filled in (constructor `Object.assign`, the `defineFunction(...)` factory calls with their `as (...) => ...` assertions, and the class-field instance-method bindings `name = defineInstanceFunction(...).bind(this) as () => T` written inside the class body). The `.ts` is what executes; the `.d.ts` is what consumers and tooling type-check against.

Rationale for keeping both files:
- The `.d.ts` is the documented public contract. Stripping it of runtime noise (`.bind(this)`, `Object.assign`, `defineFunction("user.lorem.…", "sync", […]) as …`) makes the hover-shape readable and the "go to type definition" target stable.
- Phase 5's typemap consumer (`proto.ts::decodeCallResult`) and Phase 6's npm-package surface both treat the `.d.ts` as authoritative for "what is the public shape of `Resume`". The `.ts` may grow more runtime plumbing over time; the `.d.ts` must not.
- The Python `.pyi` split exists for a different reason (pyright can't follow PEP 562 `__getattr__`), but the same artifact serves the analogous purpose here — give the type-checker a clean view independent of how the runtime is wired.

Things this does NOT imply:
- The generated SDK is not compiled. `tsconfig.json` (`sdk_tests/build/src/nodejs.rs:78-95`) keeps `noEmit: true`; both `.ts` and `.d.ts` are read as source by the user's `tsc`. The `.d.ts` is not a build artifact of compiling the `.ts` — it is independently emitted by `sdkgen_nodejs`.
- The `.ts` and `.d.ts` are not redundant. The `.ts` carries the runtime; the `.d.ts` carries the type surface. They must stay structurally aligned (any export in one appears in the other with a compatible type) but they are not derivable from each other inside the emitter.

**Banner.** Every emitted `.ts` and `.d.ts` file starts with the same do-not-edit banner that `bridge_nodejs/typescript_src/generated-header.txt` carries (lint-disable for ESLint/Prettier/etc., plus the "generated by BAML" notice). Borrow the exact text from there to stay consistent.

**Sub-phase ordering.** Each sub-phase is independently committable and ordered red→green. Every sub-phase emits both the `.ts` (runtime + types) and the `.d.ts` (slim public type surface) variant:
1. Type aliases (simplest — single line, calls `translate_ty`).
2. Enums (string enums, deterministic ordering).
3. Classes without methods (field declarations + constructor).
4. Free functions (single `defineFunction` line + sync/async fan-out + type cast).
5. Static + instance methods (factory-binding lines hung off class).
6. `$stream` companion classes + `$stream`/`$build_request`/`$render_prompt`/`$parse`/`$parse_stream` function companions.
7. Imports + JSDoc docstrings (cross-cutting; do once everything else renders).
8. Root `index.ts` + `_typemap.ts` registration.

## `.d.ts` content rules

For each kind of top, the `.d.ts` strips every runtime detail and keeps only the public type surface. The rules below apply uniformly across all sub-phases — each `render_symbol` arm in `leaf.rs` has a matching `render_symbol_dts` arm that follows these rules. See `/Users/sam/thoughts/sam-projects/bridge-node/04b-ref.md` for a full side-by-side example.

| Kind                | `.ts` shape                                                                                                                | `.d.ts` shape                                                                                                  |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Type alias          | `export type Foo = <RHS>;`                                                                                                 | identical — `export type Foo = <RHS>;` (no `declare` needed; types are ambient by nature)                      |
| Enum                | `export enum Sentiment { POSITIVE = "POSITIVE", ... }`                                                                     | `export declare enum Sentiment { POSITIVE = "POSITIVE", ... }`                                                 |
| Class field         | `name!: string;`                                                                                                           | `name: string;` (drop `!`; definite-assignment is a runtime-construction concern)                              |
| Class constructor   | `constructor(init: { ... }) { Object.assign(this, init); }`                                                                | `constructor(init: { ... });` (signature only)                                                                 |
| Static method       | `static from_url = defineFunction(..., "sync", [...]) as (url: string) => Pdf;` (class-field binding inside the body)       | `static from_url: (url: string) => Pdf;` (property declaration of function type)                               |
| Instance method     | class-field binding inside the body: `summary = defineInstanceFunction(..., "sync", ["self"]).bind(this) as () => string;`  | `summary: () => string;` (member declaration of the bound function type — no `self` in the surface type)       |
| Free function       | `export const extract_resume = defineFunction(..., "sync", [...]) as (text: string) => Resume;`                            | `export declare const extract_resume: (text: string) => Resume;`                                               |
| `_async` companion  | `export const extract_resume_async = defineFunction(..., "async", [...]) as (text: string) => Promise<Resume>;`            | `export declare const extract_resume_async: (text: string) => Promise<Resume>;`                                |
| JSDoc               | preserved verbatim on every top, on every member, on every method binding                                                  | preserved verbatim on every top, on every member, on every method binding                                     |
| Cross-leaf imports  | cross-namespace field/sig refs: single `import type * as <rootns> from ".."`; same-leaf-tree direct refs unchanged          | same single root-namespace `import type * as <rootns> from "..";` (always `import type` — type-only)           |
| Stdlib re-exports   | `export { BamlImage as Image } from "@boundaryml/baml-core-node";` + `export type Image = import("@boundaryml/baml-core-node").BamlImage;`                                       | `export declare const Image: typeof import("@boundaryml/baml-core-node").BamlImage;` + `export type Image = import("@boundaryml/baml-core-node").BamlImage;`                 |

`.d.ts`-specific exclusions:
- No `import { defineFunction }` — `defineFunction` is a runtime symbol with no role in the type surface.
- No `import { BamlRuntime, setTypeMap }` in the root index `.d.ts` — those are runtime bootstrap calls only.
- No `.bind(this)` / `defineInstanceFunction(...)` call sites. The `.d.ts` declares the bound member type directly (`summary: () => string;`).
- No type assertions (`as (text: string) => Resume`). The `.d.ts` declares the typed shape directly.
- No `Object.assign`, no factory call sites. Anything between `{` and `}` on a constructor/binding line is dropped in favor of `;`.

`.d.ts` structural alignment requirement: every exported identifier in the `.ts` must appear in the `.d.ts` with a compatible type (and vice versa). Phase 4's `render_leaf_body_dts` walks the same `LeafBody.symbols` list as `render_leaf_body` and emits a parallel declaration per symbol — there is no second symbol-collection pass. This is verified structurally: a unit test in `leaf.rs` asserts that for every kind, the set of top-level export names in the `.ts` matches the set in the `.d.ts`.

**Assumption A8:** Phase 2's `render_leaf_body_dts` already exists as a placeholder-emitting skeleton (see "What Phase 2 has delivered" above). Phase 4 fills in the per-kind `.d.ts` arms in the same file (`leaf.rs`) as the `.ts` arms, side-by-side, so the structural-alignment invariant is obvious to the reader.

## Phase 4.1 — Type Aliases

### Overview

`Ty::TypeAlias`-routed leaves emit one line per alias: `export type <PyName> = <RHS>;`. RHS comes from `translate_ty(&alias.resolves_to, &ctx)`. Recursive aliases use a different shape (see below).

### Changes Required

**File:** `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/leaf.rs`

Find the `render_symbol` arm for `EmittedSymbol::TypeAlias(a)` (currently emits a placeholder) and replace with:

```rust
EmittedSymbol::TypeAlias(a) => {
    let ctx = TranslateCtx {
        current_leaf: leaf.clone(),
        self_ref: if a.recursive {
            Some(SelfRef {
                routed_leaf: leaf.clone(),
                bare_name: a.source.bare_name().to_string(),
            })
        } else { None },
        defer_name_refs: false, // TS forward-refs are free at parse time; no deferral needed
    };
    let rhs = translate_ty(&a.resolves_to, &ctx);
    if a.recursive {
        // TS doesn't have a `TypeAliasType` analog. Recursive type aliases
        // are first-class in TS: `type Json = string | number | boolean | null | Json[] | { [k: string]: Json };`
        // resolves through the type-checker without an explicit
        // forward-ref shim — the alias name is in scope inside its own RHS.
        // We emit the same `export type Foo = <RHS>;` shape for recursive
        // and non-recursive aliases. The Python `typing_extensions.TypeAliasType`
        // workaround is needed only because pydantic v2 evaluates the
        // RHS at schema-build time; TS has no analog.
    }
    let line = format!("export type {} = {};\n", a.ts_name, rhs);
    line
}
```

(Drop the `recursive` branch entirely — TS doesn't need it. Add a doc comment explaining why this differs from Python.)

**Assumption A3:** TS recursive type aliases work natively in user space. The Phase 3 translator must emit `Self` self-references using the alias's own bare name (not a `forward_ref!("Foo")` string sentinel). If `translate_ty` was implemented to defer self-refs as strings, refactor in Phase 3 — not here.

**`.d.ts` variant.** Identical line — `export type Foo = <RHS>;` is ambient-by-nature, so the same string emits in both files. The matching `render_symbol_dts` arm just calls the same `translate_ty` + `format!` pair.

### Success Criteria

#### Automated Verification
- [ ] `cargo build -p sdkgen_nodejs` passes.
- [ ] Inside `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs/generated/type_shapes/baml_sdk/aliases/index.ts`, every BAML alias from `/Users/sam/baml3/baml_language/sdk_tests/fixtures/type_shapes/baml_src/ns_aliases/` renders as `export type <Name> = <RHS>;`.
- [ ] The corresponding `aliases/index.d.ts` contains the same `export type <Name> = <RHS>;` line for every alias.
- [ ] `cargo nextest run -p sdk_test_nodejs -E 'test(type_shapes::tsc)'` doesn't fail on alias syntax errors. (Note: it may still fail for *other* reasons until later sub-phases.)
- [ ] No `// type alias placeholder` strings remain in any generated `.ts` or `.d.ts` file.

#### Manual Verification
- [ ] Open `generated/type_shapes/baml_sdk/aliases/index.ts` and confirm RHS uses TS idioms (`string[]` not `List<string>`, `number | null` not `Optional<number>`).
- [ ] Recursive alias (if present in `ns_aliases`) compiles without the typescript_extensions workaround.

---

## Phase 4.2 — Enums

### Overview

`Ty::Enum`-routed leaves emit `export enum <Name> { VARIANT = "VARIANT", ... }`. String enums (not numeric) for wire compatibility with the proto `enum_value` payload, which carries the variant name as a string.

### Changes Required

**File:** `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/leaf.rs`

Replace the `EmittedSymbol::Enum(e)` arm:

```rust
EmittedSymbol::Enum(e) => {
    let mut out = String::new();
    // JSDoc docstring (sub-phase 4.7 fills this in; for 4.2 it can be a no-op)
    out.push_str(&format!("export enum {} {{\n", e.ts_name));
    for v in &e.variants {
        // Value is the variant's wire identifier (Python emits it as a string literal).
        // Pre-Phase 3 translate_ty does not normalize this — emit verbatim.
        let value_str = ts_string(&v.value); // mirror of py_string
        out.push_str(&format!("    {} = {},\n", v.ident, value_str));
    }
    out.push_str("}\n");
    out
}
```

`ts_string(s)` is a TS string-literal escaper analogous to `py_string` (handle `\`, `"`, `\n`, `\r`, `\t`; use double-quotes throughout for consistency).

**`.d.ts` variant.** Same body, but the keyword is `export declare enum` rather than `export enum`. The variant list is identical (declared enum members carry their string values for tooling that reads them).

### Success Criteria

#### Automated Verification
- [ ] `cargo nextest run -p sdk_test_nodejs -E 'test(type_shapes::tsc)'` no longer fails on enum syntax.
- [ ] In `generated/type_shapes/baml_sdk/enums/index.ts`, every BAML enum from `/Users/sam/baml3/baml_language/sdk_tests/fixtures/type_shapes/baml_src/ns_enums/` is present; `enums/index.d.ts` mirrors with `export declare enum`.
- [ ] In `generated/docstrings_etc/baml_sdk/docs/index.ts`, `Priority` and `Sentiment` enums emit correctly; `docs/index.d.ts` mirrors.
- [ ] The `customizable/docstrings_etc/main.test.ts` "Sentiment enum has expected members" and "Priority enum has expected members" cases pass (after sub-phase 4.7 lands the import; can verify with a partial fixture).

#### Manual Verification
- [ ] Variant order matches BAML declaration order.
- [ ] No numeric enum values (every `=` RHS is a quoted string).

---

## Phase 4.3 — Classes (without methods)

### Overview

`Ty::Class`-routed leaves emit `export class <Name>` with typed fields. Phase 4.5 will add static and instance methods; this sub-phase covers the field-only shape.

### Design decision: TS `class` vs `interface` vs `type`

We emit a TS **`class`** rather than an `interface` or `type` because:

1. **`instanceof` checks.** Phase 5's `proto.ts::decodeCallResult` will need to construct typed instances for `class_value` proto messages. `decodeCallResult` does `new Resume({ name, email })`. An `interface` has no runtime; a `class` is constructible.
2. **Identity for the typemap.** `_typemap.ts` registers `("user.lorem.Resume", Resume)`. The value side has to be a runtime object; `class` is the only TS construct that gives both a type and a value at the same name.
3. **TS `class` field declarations with `!` definite-assignment** give the same shape as Python's `pydantic.BaseModel` declared fields: `name!: string`. The `!` is required under `strict: true` since the constructor uses `Object.assign(this, init)` and the checker can't prove every field is set.

Constructor takes a single `init` object literal so call sites read like Pydantic's kwargs: `new Resume({ name: "x", email: null })`. Keyword-only is consistent with the BAML "always keyword-only" rule (`00a-spec-codegen-mappings.md` §"Rules" line 36).

### Changes Required

**File:** `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/leaf.rs`

Replace the `EmittedSymbol::Class(c)` arm (still skip the media re-export early-return, which keeps the import line shape):

```rust
EmittedSymbol::Class(c) => {
    if let Some((module, runtime_name, public_name)) = media_reexport_ts_name(c) {
        // Media/Stream stdlib types are pure re-exports. (Phase 1 ships them.)
        // The runtime exports them under their `Baml`-prefixed names only and
        // does NOT alias; codegen does the aliasing on re-export, binding the
        // runtime `BamlImage` to the public `Image`. Emit both the value
        // re-export (aliased) and the paired `export type` so the symbol
        // resolves in type and value position
        // (00a-example-ts-codegen-type-shapes.md:531-573):
        //   export { BamlImage as Image } from "@boundaryml/baml-core-node";
        //   export type Image = import("@boundaryml/baml-core-node").BamlImage;
        return format!(
            "export {{ {runtime_name} as {public_name} }} from \"{module}\";\n\
             export type {public_name} = import(\"{module}\").{runtime_name};\n",
        );
    }
    let ctx = TranslateCtx { /* ...as in 4.1... */ };
    let mut out = String::new();
    // JSDoc docstring (sub-phase 4.7)
    let generic_clause = if c.generic_params.is_empty() {
        String::new()
    } else {
        format!("<{}>", c.generic_params.join(", "))
    };
    out.push_str(&format!("export class {}{} {{\n", c.ts_name, generic_clause));
    // Field declarations
    for prop in &c.properties {
        let ty_ts = translate_ty(&prop.ty, &ctx);
        // JSDoc per-field (sub-phase 4.7)
        out.push_str(&format!("    {}!: {};\n", prop.name, ty_ts));
    }
    // Constructor (always emitted, even for zero-field classes — uniform shape)
    out.push_str(&format!("\n    constructor(init: {}) {{\n", init_type_literal(c, &ctx)));
    out.push_str("        Object.assign(this, init);\n");
    out.push_str("    }\n");
    // (Phase 4.5 inserts static/instance method bindings here.)
    out.push_str("}\n");
    out
}

fn init_type_literal(c: &TsClass, ctx: &TranslateCtx) -> String {
    if c.properties.is_empty() {
        return "{}".to_string();
    }
    let mut s = String::from("{ ");
    for (i, prop) in c.properties.iter().enumerate() {
        if i > 0 { s.push_str("; "); }
        s.push_str(&prop.name);
        s.push_str(": ");
        s.push_str(&translate_ty(&prop.ty, ctx));
    }
    s.push_str(" }");
    s
}

fn media_reexport_ts_name(c: &TsClass) -> Option<(&'static str, &'static str, &'static str)> {
    // Returns (module, runtime export name, public name). The runtime exports
    // these under their `Baml`-prefixed names (`BamlImage`, …, `BamlStream`)
    // and does not alias; codegen aliases them to the public bare names on
    // re-export (`export { BamlImage as Image }`).
    match c.source.to_string().as_str() {
        "baml.media.Image" => Some(("@boundaryml/baml-core-node", "BamlImage", "Image")),
        "baml.media.Video" => Some(("@boundaryml/baml-core-node", "BamlVideo", "Video")),
        "baml.media.Audio" => Some(("@boundaryml/baml-core-node", "BamlAudio", "Audio")),
        "baml.media.Pdf"   => Some(("@boundaryml/baml-core-node", "BamlPdf",   "Pdf")),
        "baml.llm.Stream"  => Some(("@boundaryml/baml-core-node", "BamlStream", "Stream")),
        _ => None,
    }
}
```

**Type-side re-export (required).** The media leaf always emits both a value re-export with the alias (`export { BamlImage as Image } from "@boundaryml/baml-core-node";`) and the paired `export type Image = import("@boundaryml/baml-core-node").BamlImage;`, so the symbol resolves in both type and value position (`00a-example-ts-codegen-type-shapes.md:531-573`). The `.d.ts` uses `export declare const Image: typeof import("@boundaryml/baml-core-node").BamlImage;` + the same `export type` line (00a-example:562-573).

**Assumption A4:** `Image` / `Video` / `Audio` / `Pdf` / `Stream` are re-exported (as values and types) from `@boundaryml/baml-core-node` (Phase 1). The package name is the runtime npm package shipped from `bridge_nodejs/`. If Phase 1 has a different package name in `package.json`, update the constants here.

### `$stream` companion classes

Sub-phase 4.6 handles `$stream`-routed classes. The class body shape is identical to the non-stream shape — Phase 2 routing already pre-handles the field type widening (`baml_compiler` produces a `$stream` class TIR with each field type wrapped to allow partial values). Phase 4 does not re-derive `Partial<T>`; it just renders the routed TIR class as-is. This matches the Python rule (`00a-spec-codegen-mappings.md` line 47).

**`.d.ts` variant.** `export declare class Resume {` opener, then per the rules in `.d.ts content rules`: drop `!` on every field, emit the constructor as a signature-only line `constructor(init: { ... });` (reuse `init_type_literal`), drop the `Object.assign` body, drop the trailing comment. Stdlib re-exports (`baml.media.Image` etc.) emit the `export declare const Image: typeof import("@boundaryml/baml-core-node").BamlImage;` + `export type Image = import("@boundaryml/baml-core-node").BamlImage;` pair in the `.d.ts` (00a-example:562-573); the `.ts` emits the `export { BamlImage as Image } from "@boundaryml/baml-core-node";` aliased value re-export plus the paired `export type`.

### Success Criteria

#### Automated Verification
- [ ] `cargo nextest run -p sdk_test_nodejs -E 'test(type_shapes::tsc)'` passes for class-only fixtures.
- [ ] `generated/type_shapes/baml_sdk/lorem/index.ts` contains `export class Resume`; `lorem/index.d.ts` contains `export declare class Resume` with the same fields but without `!` and without the `Object.assign` constructor body.
- [ ] `generated/llm_functions/baml_sdk/lorem/index.ts` contains `export class Resume` with all fields from the BAML source; `lorem/index.d.ts` mirrors.
- [ ] `customizable/type_shapes/main.test.ts` "Resume is reachable" passes (after import wiring lands in 4.7).
- [ ] `customizable/llm_functions/main.test.ts` "lorem.Resume is reachable" and "lorem.StreamingDoc is reachable" pass.

#### Manual Verification
- [ ] In the `.ts`, fields use `!:` (definite-assignment); in the `.d.ts`, the same fields use `:`.
- [ ] Constructor signature compiles with strict mode on, in both files.
- [ ] Generic classes emit `<T>` after the class name in both files.

---

## Phase 4.4 — Free Functions

### Overview

`EmittedSymbol::Function` leaves emit two bindings per BAML function: a sync `defineFunction(..., "sync", ...)` and an async `defineFunction(..., "async", ...)`. Each is typed via a TS type assertion using `translate_ty` on the param types and return type.

### Naming

Per `00a-spec-codegen-mappings.md` line 34, codegen always appends `_async` verbatim. So BAML `extract_resume` → TS `extract_resume` + `extract_resume_async`. The Phase 2 expander already fans out into two `TsFunction`s; Phase 4 just renders them.

### Changes Required

**File:** `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/leaf.rs`

Replace the `EmittedSymbol::Function(f)` arm with a real renderer:

```rust
EmittedSymbol::Function(f) => {
    let ctx = TranslateCtx { /* ...same as 4.1... */ };
    let mode_str = match f.mode {
        SyncAsync::Sync => "\"sync\"",
        SyncAsync::Async => "\"async\"",
    };
    let param_array = render_param_array(&f.param_names);
    let required_positional_count = required_positional_count(&f.arg_defaults, 0);
    let count_arg = render_required_positional_count_arg(required_positional_count, f.param_names.len());
    // Build the typed signature: `(text: string) => Resume` for sync,
    // `(text: string) => Promise<Resume>` for async.
    let typed_sig = render_function_signature(&f, &ctx);
    let mut out = String::new();
    // JSDoc (sub-phase 4.7)
    out.push_str(&format!(
        "export const {name} = defineFunction({fqn}, {mode_str}, {params}{count_arg}) as {typed_sig};\n",
        name = f.ts_name,
        fqn = ts_string(&f.baml_fqn),
        params = param_array,
        count_arg = count_arg,
        typed_sig = typed_sig,
    ));
    out
}

fn render_function_signature(f: &TsFunction, ctx: &TranslateCtx) -> String {
    let params = f.param_names.iter().zip(f.arg_tys.iter())
        .map(|(n, t)| format!("{}: {}", n, translate_ty(t, ctx)))
        .collect::<Vec<_>>().join(", ");
    let ret = translate_ty(&f.return_ty, ctx);
    match f.mode {
        SyncAsync::Sync => format!("({}) => {}", params, ret),
        SyncAsync::Async => format!("({}) => Promise<{}>", params, ret),
    }
}

fn render_param_array(names: &[String]) -> String {
    if names.is_empty() {
        return "[]".to_string();
    }
    let inner = names.iter().map(|n| ts_string(n)).collect::<Vec<_>>().join(", ");
    format!("[{}]", inner)
}
```

`required_positional_count` / `render_required_positional_count_arg` mirror the Python helpers verbatim.

### Imports

Each leaf with any free function or static method needs `import { defineFunction } from "@boundaryml/baml-core-node";`; each leaf with any instance method also needs `defineInstanceFunction` (`import { defineFunction, defineInstanceFunction } from "@boundaryml/baml-core-node";` — see `00a-example-ts-codegen-type-shapes.md:360`). The `needs_define_function` predicate on `LeafBody` already exists (ported in Phase 2); add a `needs_define_instance_function` predicate keyed on the presence of any instance method. Sub-phase 4.7 wires both.

The `.d.ts` does NOT import `defineFunction` or `defineInstanceFunction` — there is no runtime binding to resolve in the type surface.

**`.d.ts` variant.** Drop the `defineFunction(...)` call and the `as` type assertion; declare the typed shape directly:

```rust
// In `render_symbol_dts` for EmittedSymbol::Function:
let typed_sig = render_function_signature(&f, &ctx); // same helper as the .ts arm
let line = format!("export declare const {name}: {typed_sig};\n", name = f.ts_name, typed_sig = typed_sig);
```

So the `.d.ts` for the same example emits:
```ts
export declare const extract_resume:       (text: string) => Resume;
export declare const extract_resume_async: (text: string) => Promise<Resume>;
```

### Success Criteria

#### Automated Verification
- [ ] `generated/llm_functions/baml_sdk/lorem/index.ts` contains both `export const ExtractResume = defineFunction("user.lorem.ExtractResume", "sync", ["text"]) as (text: string) => Resume;` and the `_async` sibling.
- [ ] `generated/llm_functions/baml_sdk/lorem/index.d.ts` contains `export declare const ExtractResume: (text: string) => Resume;` and the `_async` sibling — no `defineFunction` reference.
- [ ] `customizable/llm_functions/main.test.ts` "lorem.ExtractResume sync + async factories are callable" passes.
- [ ] `cargo nextest run -p sdk_test_nodejs -E 'test(/::tsc/)'` passes the function-typing constraints.

#### Manual Verification
- [ ] Each function emits two lines (sync + async) in each file; both lines align visually.
- [ ] Function arguments use BAML field names verbatim in both files.
- [ ] Async return type is wrapped in `Promise<T>` in both files.

---

## Phase 4.5 — Static + Instance Methods

### Overview

`PyMethodBinding` analog `TsMethodBinding` carries `kind: MethodKind::{Static,Instance}`, `mode: SyncAsync`, `param_names`, `arg_tys`, `return_ty`, `baml_fqn`, `ts_name`. Phase 2 already places these on `TsClass.static_methods` and `TsClass.instance_methods`.

Phase 4.5 emits method bindings as **class-field initializers inside the class body** — both statics and instance methods. There is no public-shim method, no `private static _*` factory storage, and no prototype method. Both kinds are typed property initializers:

- **Static method:** `static from_url = defineFunction("baml.media.Pdf.from_url", "sync", ["url"]) as (url: string) => Pdf;` — a `static` class-field initializer. Uses `defineFunction` (the value-class form): the call site is `Pdf.from_url(url)`, with `self` NOT prepended. This matches the locked decision "Static methods use the value-class form `Image.from_url()`" and the spec's `Image.from_url()` (`00a-spec-codegen-mappings.md:19,69`).
- **Instance method:** `get_value = defineInstanceFunction("user.generics.WrapperMethods.get_value", "sync", ["self"]).bind(this) as () => T;` — a non-`static` class-field initializer written **inside the class body**. `param_names` starts with `"self"` (Phase 2's expander prepends it). The synthetic `self` param is consumed by `defineInstanceFunction`/`.bind(this)` and never appears in the surface type (`() => T`). See `00a-example-ts-codegen-type-shapes.md:378-394,463`.

**Why the class-field `.bind(this)` form (locked decision).** The instance-method binding *is* the class member itself: a field initializer `name = defineInstanceFunction(...).bind(this)` evaluated in the constructor's field-init phase. `.bind(this)` captures the receiver once at construction time, so:
1. `wrapper.get_value()` works idiomatically.
2. A detached reference — `const f = wrapper.get_value; f()` — still carries its receiver (because the bound function closed over `this`).
3. The synthetic `self` never surfaces: the typed cast is `() => T`, not `(self: …) => T`.

The cost is one bound-function allocation per instance method per instance. Using `defineInstanceFunction` (not `defineFunction`) marks the receiver-binding flavor.

⚠ **Spec note (Python divergence):** the Python prior art binds instance methods as plain class-body attributes off a *single* `_define_function` factory (`generics/__init__.py:34-37`: `get_value = _define_function("…", "sync", ["self"])`), relying on Python's descriptor protocol to inject `self`. The `.pyi` declares them as `def get_value(self) -> T: ...` (method declarations). TS has no descriptor protocol, so the spec uses the distinct `defineInstanceFunction(...).bind(this)` form and declares them in the `.d.ts` as bound members `get_value: () => T;` rather than method-syntax `get_value(): T;`. Phase 4 follows the **TS spec** (`00a-example`), not the Python `.pyi` method-syntax shape.

### Companion method bindings

Methods can have `$build_request` / `$render_prompt` / `$parse` / `$parse_stream` / `$stream` companions. Phase 2's expander already fans these out into additional `TsMethodBinding` entries with the suffix-mangled name (`build_from_linkedin__build_request`, etc., per `00a-spec-codegen-mappings.md:84`). Phase 4.5 renders each as one more class-field binding line — `defineFunction(...)` for static-attached companions, `defineInstanceFunction(...).bind(this)` for instance-attached companions.

### Changes Required

In the class-body renderer from sub-phase 4.3, after the constructor block, insert:

```rust
for m in &c.static_methods {
    let line = render_method_binding(m, &ctx);   // `static <name> = defineFunction(...) as <sig>;`
    out.push_str(&format!("    {}\n", line));
}
for m in &c.instance_methods {
    let line = render_instance_method_binding(m, &ctx); // `<name> = defineInstanceFunction(...).bind(this) as <sig>;`
    out.push_str(&format!("    {}\n", line));
}
```

`render_method_binding` for statics is essentially the free-function renderer indented by 4 spaces and prefixed with `static`. `render_instance_method_binding` emits the single class-field line using `defineInstanceFunction(...).bind(this)`; the typed cast drops the leading `self` param (instance methods' `arg_tys` already exclude `self`, so `render_function_signature` over `param_names[1..]`/`arg_tys` yields `() => T`).

**`.d.ts` variants.** Inside the matching `export declare class` body:

- Static method: `static from_url: (url: string) => Pdf;` — property declaration of function type. Drop the `defineFunction(...)` call and the `as` assertion.
- Instance method: `get_value: () => T;` — a **bound-member declaration** (property of function type), NOT method syntax `get_value(): T;`. This matches the `.bind(this)`-produced shape and `00a-example-ts-codegen-type-shapes.md:438-441`. Drop the `defineInstanceFunction(...).bind(this)` call and the `as` assertion.
- `_async` siblings follow the same rules, with `Promise<T>` return.

Concretely, for the `WrapperMethods` example (`00a-example` §"Generics And Instance Methods"), the `.ts` arm emits one line per binding (`get_value = …`, `get_value_async = …`), and the `.d.ts` arm emits one matching member declaration per binding (`get_value: () => T;`, `get_value_async: () => Promise<T>;`).

### Success Criteria

#### Automated Verification
- [ ] `cargo nextest run -p sdk_test_nodejs -E 'test(/::tsc/)'` passes for fixtures with methods.
- [ ] `customizable/llm_functions/main.test.ts` "lorem.ExtractResume companion bindings exist" passes for static-method companions if any are exercised; method-level companions are exercised when a class with methods is in the fixture.
- [ ] In any generated class with a static method, the `.ts` contains `static from_url = defineFunction(...) as (...) => ...;` and the `.d.ts` contains `static from_url: (...) => ...;`.
- [ ] In any generated class with an instance method, the `.ts` contains the class-field `<name> = defineInstanceFunction(...).bind(this) as () => ...;` line and the `.d.ts` contains the bound-member declaration `<name>: () => ...;`.
- [ ] No `private static _`, no `.bind(this)`, no `defineInstanceFunction` references appear in any `.d.ts`.

#### Manual Verification
- [ ] An instance method called from a typed TS test calls through correctly (deferred verification — requires Phase 5 to actually round-trip).
- [ ] Static method visible at `Class.method`; instance method visible at `(new Class(...)).method` and as a detached reference `const f = inst.method; f()` (the `.bind(this)` keeps the receiver).

---

## Phase 4.6 — `$stream` Companion Classes + Function Companions

### Overview

`$stream` **classes** route to `stream_types/<ns>/...` per Phase 2 routing. Phase 4.6 ensures the class body renders correctly there (same shape as a normal class — Phase 2's routing already moved it).

`$stream` / `$build_request` / `$render_prompt` / `$parse` / `$parse_stream` **functions** are extra `defineFunction(...)` lines on the same leaf as the parent. Phase 2's `expand_callable` analog generates these as additional `TsFunction`s. Phase 4.6 verifies they render through the Phase 4.4 code path.

### Naming rules (per `00a-spec-codegen-mappings.md` lines 48-52)

- `$stream` companions use **single underscore**: BAML `extract_resume$stream` → TS `extract_resume_stream` + `extract_resume_stream_async`.
- `$build_request`, `$render_prompt`, `$parse`, `$parse_stream` use **double underscore**: BAML `extract_resume$build_request` → TS `extract_resume__build_request` + `extract_resume__build_request_async`.
- Companions on methods follow the same rule: BAML `Resume.from_linkedin$build_request` → TS `Resume.from_linkedin__build_request`.

Phase 2's expander applies these naming rules when producing `ts_name`. Phase 4 just uses `f.ts_name` verbatim.

### Changes Required

**No new code paths.** Sub-phases 4.3, 4.4, 4.5 already handle the rendering. Sub-phase 4.6 is a **verification + minor naming-edge-case** sub-phase:

1. Verify `stream_types/lorem/index.ts` is emitted and contains `export class Resume` (the `$stream` version) with the appropriate field shape from PPIR.
2. Verify `lorem/index.ts` contains all five companion functions (each `_async` doubled).
3. Verify the FQN passed to `defineFunction` carries the `$<suffix>` tail (`"user.lorem.extract_resume$build_request"`) — that's what the engine looks up.
4. Verify method-level companions render correctly inside their parent class.

If any of (1-4) doesn't render correctly, the bug is in Phase 2 routing or expander; fix there, not here. Phase 4 just confirms.

### Success Criteria

#### Automated Verification
- [ ] `customizable/llm_functions/main.test.ts` "stream_types/lorem exposes at least one $stream companion class" passes.
- [ ] `customizable/llm_functions/main.test.ts` companion-bindings tests pass for `ExtractResume`, `StreamingExtract`, and `ClassifySentiment`.
- [ ] Inside `generated/llm_functions/baml_sdk/lorem/index.ts`, the line `export const ExtractResume__build_request = defineFunction("user.lorem.ExtractResume$build_request", "sync", ["text"]) as (text: string) => Request;` appears (or the right return type — depends on the BAML signature).

#### Manual Verification
- [ ] `stream_types/lorem/index.ts` doesn't accidentally include non-`$stream` classes.
- [ ] No name collisions between `extract_resume` and `extract_resume_stream` (`_stream` suffix is single-underscore by design).

---

## Phase 4.7 — Imports + JSDoc Docstrings

### Overview

This sub-phase wires up two cross-cutting concerns:

1. **Cross-namespace imports.** Each leaf that references a symbol in a different namespace needs a SINGLE `import * as <rootns> from ".."` (root-namespace) import; cross-namespace symbols are then reached as `<rootns>.<ns-path>.<Name>`, never via per-leaf flattened named imports. Phase 2's `LeafBody::root_imports_py` analog walks the symbols and produces the root-import set. Phase 4.7 renders it as the single root-namespace `import` line at the top of each leaf file. (See "Import shape" below and `00a-spec-codegen-mappings.md:273-301`.)

2. **JSDoc docstrings.** BAML `///` doc comments need to surface as `/** ... */` JSDoc blocks above each generated top. Class- and enum-level docstrings include an `Attributes:` / `Members:` section listing per-field / per-variant docstrings (mirrors Python's `format_class_docstring` at `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/utils.rs:73-144`).

### Import shape

TS uses ES module imports. Eager imports are fine (TS handles tree-shaking) — no PEP 562 `__getattr__` analog. Each leaf `.ts` emits, in order:

1. The do-not-edit banner.
2. `import { defineFunction } from "@boundaryml/baml-core-node";` if any free function or static method is in the leaf; add `defineInstanceFunction` if any instance method is in the leaf.
3. `import { BamlHandle } from "@boundaryml/baml-core-node";` if any field/param/return type translates to a `BamlHandle` (analog of Python's `_BamlPyHandle` import for `Ty::RustType`).
4. `import { BamlStream } from "@boundaryml/baml-core-node";` if any field/param/return type involves `baml.llm.Stream`.
5. **Cross-namespace references use a SINGLE root-namespace import**, not per-leaf flattened named imports. When a field/alias/signature references a symbol in a *different* namespace, emit exactly one `import * as <rootns> from "<path-to-root>";` (runtime import when a class field or alias RHS needs the binding to resolve at module load; `import type * as <rootns>` for signature-only refs) and refer to symbols by their full root-relative path `<rootns>.fizz.foo.Bar`. This matches `00a-spec-codegen-mappings.md:273-301` (the correct/wrong appendix block) and the `Ipsum` example in `00a-example-ts-codegen-type-shapes.md:471-499`. Do NOT emit per-leaf `import * as symbol_collisions_fizz_foo from "../fizz/foo";` flattened imports — that is the spec's explicit "wrong" form.

The matching `.d.ts` emits a slimmer import block, in order:

1. The do-not-edit banner.
2. (No `defineFunction` / `defineInstanceFunction` import — runtime symbols with no type-surface role.)
3. `import type { BamlHandle } from "@boundaryml/baml-core-node";` if any field/param/return type translates to `BamlHandle`. The `type` qualifier asserts at parse time that this is a type-only import (no runtime binding).
4. `import type { BamlStream } from "@boundaryml/baml-core-node";` if any field/param/return type involves `baml.llm.Stream`.
5. The same single root-namespace cross-namespace import, always `import type * as <rootns> from "..";` (the `.d.ts` is pure type surface, so it is always type-only).

**Cross-namespace vs same-leaf refs.** A reference to a symbol in the *same* leaf needs no import (it is in scope). The single-root-namespace rule covers refs across namespaces; the root namespace is imported once and every cross-namespace symbol is reached as `<rootns>.<ns-path>.<Name>`.

**Root-namespace import path.** From a leaf at depth `d` (e.g. `symbol_collisions/lorem`, depth 2), the path to the SDK root is `".."` repeated to reach the package root — for the `Ipsum` example (`00a-example:473`) it is `import type * as symbol_collisions from "..";` because `symbol_collisions` is the root container. Compute `depth = current.segments.len()` and emit the appropriate `..`-ladder; the relative-path computation is identical for `.ts` and `.d.ts` (only `import` vs `import type` differs).

**TS quirk:** `import * as ns from "./bar"` resolves to `./bar.ts` or `./bar/index.ts` automatically under `moduleResolution: "node"`. Each leaf is `<name>/index.ts` per the file-layout rule (`00a-spec-codegen-mappings.md:53-62`), so the import target is `"./<name>"` / `".."` — TS resolves both.

**Assumption A6:** All cross-namespace import paths use `"./"` / `"../"` style relative paths (no module-name aliases) and the single-root-namespace form. This matches the project's `tsconfig.json` (`moduleResolution: "node"`, no path aliases) and `preserveSymlinks: true`.

### JSDoc format

Python format (from `format_class_docstring`):

```python
"""
Application configuration.

Attributes:
    timeout: Timeout in seconds.
    retries: Number of retry attempts.
"""
```

TS analog (JSDoc / TSDoc):

```ts
/**
 * Application configuration.
 *
 * @property timeout - Timeout in seconds.
 * @property retries - Number of retry attempts.
 */
export class Foo { ... }
```

For enums:

```ts
/**
 * Sentiment scale.
 *
 * @remarks
 * Members:
 * - HAPPY: Smiling face.
 * - SAD
 * - NEUTRAL
 */
export enum Sentiment { ... }
```

For functions:

```ts
/**
 * Extracts a resume from raw text.
 *
 * @param text - The raw resume text.
 * @returns The parsed resume.
 */
export const extract_resume = defineFunction(...) as ...;
```

Per-field JSDoc inside a class body:

```ts
export class Resume {
    /** The candidate's name. */
    name!: string;
    /** Email, if present. */
    email!: string | null;
}
```

(JSDoc on a class member is the TS-idiomatic place to document a field, *in addition to* the class-level `@property` summary. This double-document is intentional — TS tooling shows both.)

JSDoc rendering is identical in `.ts` and `.d.ts` — every JSDoc block emitted on a top/field/method in the `.ts` is also emitted in the `.d.ts` at the same position. The function-level shim in `format_class_docstring_ts` / `format_docstring_ts` is called from both `render_symbol` and `render_symbol_dts`.

### Changes Required

**File:** `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/utils.rs` (new file).

Implement `format_class_docstring_ts` (analog of Python's, but emitting JSDoc), `format_member_docstring_ts` (single-line `/** ... */` for fields/params), and a docstring escaper that handles `*/` (TS doesn't have a `\"""` analog; escape `*/` → `*\/`).

**File:** `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/leaf.rs`

In each `render_symbol` and `render_symbol_dts` arm, prepend the JSDoc block produced by `utils::format_class_docstring_ts` (for classes/enums) or `utils::format_docstring_ts` (for functions/methods/aliases). Same JSDoc text in both files.

In `render_leaf_body`, after the banner, emit the import block: stdlib (`defineFunction`, `BamlHandle`, etc.), then cross-leaf imports computed from `LeafBody::all_rel_imports_ts()` (analog of `all_rel_imports_py`).

In `render_leaf_body_dts`, after the banner, emit the slim `import type` block (no `defineFunction`, type-qualified `import type` for everything else).

### Success Criteria

#### Automated Verification
- [ ] `cargo nextest run -p sdk_test_nodejs -E 'test(/::tsc/)'` passes on `type_shapes`, `docstrings_etc`, `llm_functions`.
- [ ] `customizable/type_shapes/main.test.ts` "every namespace module imports cleanly" passes.
- [ ] In `generated/docstrings_etc/baml_sdk/docs/index.ts`, every documented top has a `/** ... */` block immediately preceding it; `docs/index.d.ts` carries the same JSDoc at the same positions.
- [ ] In `generated/llm_functions/baml_sdk/lorem/index.ts`, the top of the file imports `defineFunction` (and `defineInstanceFunction` if any instance method exists) and, for any cross-namespace ref, a SINGLE root-namespace import (e.g. `import * as <rootns> from "..";`), reaching `<rootns>.ipsum.Sentiment`; `lorem/index.d.ts` uses the same `import type * as <rootns>` and does NOT import `defineFunction`/`defineInstanceFunction`.

#### Manual Verification
- [ ] Hovering a generated symbol in VS Code shows the JSDoc summary + per-field / per-param descriptions.
- [ ] Cross-namespace refs use the single root-namespace import + dotted path (`<rootns>.ipsum.Sentiment`), NOT per-leaf flattened named imports.
- [ ] No `*/` inside any docstring is unescaped (check both files).
- [ ] No `.d.ts` contains a runtime `import { defineFunction }` / `import { defineInstanceFunction }` line.

---

## Phase 4.8 — Root `index.ts` + Typemap Registration

### Overview

Two leaves remain after the per-symbol leaves: the SDK root (`baml_sdk/index.ts`) and the `_typemap.ts` data module.

### Root `index.ts`

Mirrors Python's `render_root_init` at `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/lib.rs:325-339`. Eagerly imports `_inlinedbaml` and `_typemap`; calls `BamlRuntime.initializeRuntime("baml_src", _inlinedbaml.FILES)` and `setTypeMap(_TYPE_MAP)`; re-exports every top-level child namespace **namespace-preservingly** via `import * as <child> from "./<child>"` + `export { <child>, ... }` (equivalently `export * as <child> from "./<child>"`). This mirrors the root `.pyi`'s `from . import lorem / ipsum / ...` (child-namespace re-exports), NOT a flattening `from .lorem import *`. Per `00a-spec-codegen-mappings.md:56-58` and `00a-example-ts-codegen-type-shapes.md:91`, parent modules must NOT flatten child symbols — `baml_sdk.symbol_collisions.lorem.Ipsum` must never be reachable as `baml_sdk.symbol_collisions.Ipsum`. There is no aggregated `b` client object: root-namespace functions are plain module-level exports, and users alias the whole package (`import * as b from "baml_sdk"`).

```ts
// Banner...
import { BamlRuntime, setTypeMap, defineFunction } from "@boundaryml/baml-core-node";
import { FILES } from "./_inlinedbaml";
import { _TYPE_MAP } from "./_typemap";
import * as lorem from "./lorem";
import * as ipsum from "./ipsum";
// ...

BamlRuntime.initializeRuntime("baml_src", FILES);
setTypeMap(_TYPE_MAP);

export { lorem, ipsum /* , ... */ };
// Plus root-namespaced symbols exported inline (the root leaf body) — plain
// module-level exports, NOT methods on an aggregated `b` client:
export class Foo { ... }
export type Bar = ...;
export const make_foo = defineFunction("user.make_foo", "sync", ["v"]) as (v: number) => Foo;
// ...
```

**Assumption A7:** TS `export * as <child> from "./child"` (or the `import * as <child>` + `export { <child> }` pair) exposes the child as a nested namespace WITHOUT flattening its symbols. This matches the root `.pyi`'s `from . import <child>` pattern. Two child namespaces cannot collide because each is reached only through its own namespace path; the BAML compiler additionally assigns distinct namespaces (`00a-spec-codegen-mappings.md:38-42`).

### Per-directory `index.ts`

For non-root directory leaves (e.g. `stream_types/`, `vendor/`, `baml/`), emit a small `index.ts` that re-exports its children namespace-preservingly (NOT flattening):

```ts
// Banner...
export * as aws from "./aws";
export * as gcp from "./gcp";
```

Phase 2 already enumerates `children` for each directory; Phase 4 just renders the namespace-preserving re-exports. Container `index.ts` files export child namespaces only — they never contain generated BAML symbols (`00a-spec-codegen-mappings.md:57`).

### `_inlinedbaml.ts`

Same shape as Python's `_inlinedbaml.py` but as a TS module:

```ts
// Banner...
export const FILES: Record<string, string> = {
    "ns_lorem/main.baml": "...",
    "ns_ipsum/main.baml": "...",
    // ...
};
```

Each value is a TS string literal — use `ts_string()` for safe escaping (handles `\`, `"`, `\n`, `\r`, `\t`, plus `\u{...}` for non-ASCII if needed).

### `_typemap.ts`

Three sorted records of `FQN → [modulePath, attrName]`, plus a final `BamlTypeMap.fromLazyEntries` call:

```ts
// Banner...
import { BamlTypeMap } from "@boundaryml/baml-core-node";

const _CLASS_ENTRIES: Record<string, [string, string]> = {
    "user.lorem.Resume": ["./lorem", "Resume"],
    // ...
};
const _ENUM_ENTRIES: Record<string, [string, string]> = {
    "user.ipsum.Sentiment": ["./ipsum", "Sentiment"],
    // ...
};
const _ALIAS_ENTRIES: Record<string, [string, string]> = {
    "user.lorem.StringList": ["./lorem", "StringList"],
    // ...
};

export const _TYPE_MAP = BamlTypeMap.fromLazyEntries({
    classes: _CLASS_ENTRIES,
    enums: _ENUM_ENTRIES,
    typeAliases: _ALIAS_ENTRIES,
});
```

**`modulePath` shape.** Python uses dotted Python module paths (`"baml_sdk.lorem"`). TS uses relative paths so `_typemap.ts` can `import(...)` them lazily at lookup time: `"./lorem"` for a leaf at `baml_sdk/lorem/index.ts`, `"./stream_types/lorem"` for a stream-type leaf, etc. (each namespace is a directory with an `index.ts` per the file-layout rule).

`BamlTypeMap.fromLazyEntries` is implemented in Phase 1 and uses dynamic `import(modulePath)` + property access on the resolved module to fetch the class.

### `.d.ts` variants for the root files

- **`index.d.ts`** drops the runtime bootstrap (no `BamlRuntime.initializeRuntime`, no `setTypeMap`, no `_inlinedbaml`/`_typemap` imports — those are runtime concerns). It keeps only the namespace-preserving re-exports (matching the root `.pyi`'s `from . import <child>`), plus root-namespaced symbols re-declared inline:
  ```ts
  // Banner...
  import * as lorem from "./lorem";
  import * as ipsum from "./ipsum";
  export { lorem, ipsum /* , ... */ };
  // Plus root-namespaced symbols re-declared inline (no aggregated `b` client):
  export declare class Foo { ... }
  export type Bar = ...;
  export declare const make_foo: (v: number) => Foo;
  ```
- **`_inlinedbaml.d.ts`** declares the `FILES` shape only:
  ```ts
  // Banner...
  export declare const FILES: Record<string, string>;
  ```
- **`_typemap.d.ts`** declares only the `_TYPE_MAP` export (its internal `_CLASS_ENTRIES` etc. records are private):
  ```ts
  // Banner...
  import type { BamlTypeMap } from "@boundaryml/baml-core-node";
  export declare const _TYPE_MAP: BamlTypeMap;
  ```
- **Per-directory `index.d.ts`** (for `stream_types/`, `vendor/`, `baml/`) mirrors the `.ts` re-exports verbatim — `export * as <child> from "./<child>";` works in both.

### Changes Required

**File:** `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs`

Add (or fill in) `render_root_index`, `render_root_index_dts`, `render_package_index`, `render_package_index_dts`, `render_inlinedbaml_ts`, `render_inlinedbaml_dts`, `render_typemap_module_ts`, `render_typemap_module_dts`, and wire into the existing `to_source_code` walk:

```rust
out.insert(PathBuf::from("_inlinedbaml.ts"),  render_inlinedbaml_ts(user_baml_files));
out.insert(PathBuf::from("_inlinedbaml.d.ts"), render_inlinedbaml_dts());
out.insert(PathBuf::from("_typemap.ts"),       render_typemap_module_ts(&bodies, "baml_sdk"));
out.insert(PathBuf::from("_typemap.d.ts"),     render_typemap_module_dts());

for dir in &all_dirs {
    let leaf_path = LeafPath { segments: dir.clone() };
    let kids = children.get(dir).cloned().unwrap_or_default();
    let empty_body = LeafBody { leaf: leaf_path.clone(), symbols: Vec::new() };
    let body = bodies.get(&leaf_path).unwrap_or(&empty_body);
    // .ts: runtime bootstrap + leaf body
    let mut ts_content = if dir.is_empty() {
        render_root_index(&kids)
    } else {
        render_package_index(&kids)
    };
    ts_content.push_str(&render_leaf_body(body));
    out.insert(index_ts_path(dir), ts_content);
    // .d.ts: slim re-exports + leaf body (no runtime bootstrap)
    let mut dts_content = if dir.is_empty() {
        render_root_index_dts(&kids)
    } else {
        render_package_index_dts(&kids)
    };
    dts_content.push_str(&render_leaf_body_dts(body));
    out.insert(index_dts_path(dir), dts_content);
}
```

`render_typemap_module_ts` is the analog of Python's `render_typemap_module` at `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/emit/typemap_file.rs`. Walk `bodies`, collect three sorted vectors, render as above. `render_typemap_module_dts` is a 3-line constant — the same `export declare const _TYPE_MAP: BamlTypeMap;` shape for every fixture.

### Success Criteria

#### Automated Verification
- [ ] `generated/<fixture>/baml_sdk/index.ts` exists and calls `BamlRuntime.initializeRuntime` and `setTypeMap`.
- [ ] `generated/<fixture>/baml_sdk/index.d.ts` exists, re-exports every top, and contains NO `BamlRuntime.initializeRuntime` / `setTypeMap` / `_inlinedbaml` / `_typemap` references.
- [ ] `generated/<fixture>/baml_sdk/_typemap.ts` lists every class, enum, and alias from the fixture; `_typemap.d.ts` declares only `_TYPE_MAP: BamlTypeMap`.
- [ ] `generated/<fixture>/baml_sdk/_inlinedbaml.ts` round-trips every BAML source byte-identical; `_inlinedbaml.d.ts` declares `FILES: Record<string, string>`.
- [ ] `customizable/type_shapes/main.test.ts` "baml_sdk root imports cleanly" passes.
- [ ] `customizable/llm_functions/main.test.ts` "baml_sdk root imports cleanly" passes.
- [ ] `cargo nextest run -p sdk_test_nodejs -E 'test(/::jest$/)'` passes for `type_shapes` and `docstrings_etc`. May still have failures in `llm_functions::jest` that depend on Phase 5 round-trip.

#### Manual Verification
- [ ] Importing `from "./baml_sdk"` resolves every documented entry point.
- [ ] `_typemap.ts` entries are alphabetically sorted by FQN inside each of the three record blocks.
- [ ] No re-export ambiguity errors from tsc.
- [ ] The `.d.ts` files contain no runtime-only constructs (no `defineFunction`, no `defineInstanceFunction`, no `.bind(this)`, no `Object.assign`, no `BamlRuntime.initializeRuntime`).

---

## Testing Strategy

### Test layers per fixture

For each fixture `<fixture>` (one of `type_shapes`, `docstrings_etc`, `llm_functions`), the build emits two cargo tests:

- `<fixture>::tsc`: runs `node node_modules/typescript/bin/tsc --noEmit` against the entire generated `baml_sdk/` + the symlinked `customizable/<fixture>/main.test.ts`. Pass means all generated TS type-checks cleanly.
- `<fixture>::jest`: runs `node node_modules/jest/bin/jest.js`. Pass means all jest assertions in `main.test.ts` succeed.

Driver: `/Users/sam/baml3/baml_language/sdk_tests/build/src/nodejs.rs::write_fixtures_tests_rs` (lines 251-285).

### Expected state at Phase 4 end

| Fixture          | `tsc` | `jest` notes                                                                                                                                  |
| ---------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `type_shapes`    | green | All cases pass (pure import + reachability).                                                                                                  |
| `docstrings_etc` | green | All cases pass (import + enum shape).                                                                                                         |
| `llm_functions`  | green | All cases listed in `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs/customizable/llm_functions/main.test.ts` pass — every case is reachability + `typeof === "function"` + enum-member shape. No round-trip assertions exist in this fixture yet (per the file's header comment, those are deferred until proto bytes settle). |

### Running the suite

```bash
# Run everything (tsc + jest across all fixtures):
cargo nextest run -p sdk_test_nodejs

# Subset:
cargo nextest run -p sdk_test_nodejs -E 'test(/::tsc$/)'
cargo nextest run -p sdk_test_nodejs -E 'test(/::jest$/)'

# Single fixture:
cargo nextest run -p sdk_test_nodejs -E 'test(type_shapes::)'

# Re-trigger codegen + pnpm install if BAML fixtures or codegen change:
cargo build -p sdk_test_nodejs
```

The build script (`/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs/build.rs`) calls `sdk_test_build::nodejs::run_all()` which runs codegen for every fixture into `generated/<fixture>/baml_sdk/` and then runs `pnpm install` serially per fixture. The build.rs is wired to rerun on changes via `watch_dir(fixtures_root)` + `watch_dir(customizable_root)` (`nodejs.rs:155-157`).

### Per-sub-phase TDD anchors

| Sub-phase | First failing tests at start of sub-phase                                                                                                                                                  | Tests green at end of sub-phase                                  |
| --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| 4.1       | `type_shapes::tsc` fails: alias placeholders are not valid TS.                                                                                                                             | `type_shapes::tsc` no longer errors *on alias lines*.            |
| 4.2       | `docstrings_etc::tsc` fails on enum placeholders.                                                                                                                                          | Enum tsc errors gone.                                            |
| 4.3       | `type_shapes::tsc` + `llm_functions::tsc` fail on class placeholders.                                                                                                                      | Class tsc errors gone (excluding methods).                       |
| 4.4       | `llm_functions::jest` fails: `typeof ExtractResume === "function"` is false (it's the placeholder string).                                                                                 | Free-function jest cases pass.                                   |
| 4.5       | `tsc` may fail if any static/instance method exists in a fixture (none of the current fixtures have them under the user namespace — the `baml.media.*` and `baml.io.*` stdlib types have them). | Static/instance method tsc + jest pass.                          |
| 4.6       | `llm_functions::jest` fails companion-binding tests; `stream_types/lorem` may not exist.                                                                                                   | All companion + stream-types tests pass.                         |
| 4.7       | `type_shapes::tsc` fails: cross-leaf imports missing.                                                                                                                                      | All `tsc` jobs green; all docstrings render.                     |
| 4.8       | `<*>::jest` fails: `baml_sdk` root doesn't initialize the runtime.                                                                                                                         | All `jest` jobs green (excluding deferred Phase 5 round-trips). |

---

## Example Generated Output

Side-by-side: BAML input → Python codegen output → target TS codegen output (after Phase 4).

### Example 1: Simple class + enum + free function

**BAML** (file `ns_lorem/main.baml`):

```baml
/// A job applicant's resume.
class Resume {
    /// The candidate's full name.
    name string
    /// Email, when present.
    email string?
}

function ExtractResume(text: string) -> Resume {
    client "openai/gpt-4o-mini"
    prompt #"
        Extract a resume from {{ text }}.
        JSON.
    "#
}
```

**Python output** (`baml_sdk/lorem/__init__.py`, abbreviated):

```python
# Banner...
from __future__ import annotations

import typing
import pydantic

from baml_core import define_function as _define_function


class Resume(pydantic.BaseModel):
    """
    A job applicant's resume.

    Attributes:
        name: The candidate's full name.
        email: Email, when present.
    """
    model_config = pydantic.ConfigDict(extra="forbid")
    name: str
    email: typing.Optional[str]


ExtractResume       = _define_function("user.lorem.ExtractResume", "sync",  ["text"])
ExtractResume_async = _define_function("user.lorem.ExtractResume", "async", ["text"])
ExtractResume__build_request       = _define_function("user.lorem.ExtractResume$build_request", "sync",  ["text"])
ExtractResume__build_request_async = _define_function("user.lorem.ExtractResume$build_request", "async", ["text"])
# ... $render_prompt, $parse, $parse_stream companions ...


__all__ = ["Resume", "ExtractResume", "ExtractResume_async", ...]
```

**TS output target** (`baml_sdk/lorem/index.ts`, Phase 4 result):

```ts
// ----------------------------------------------------------------------------
//
//  Welcome to Baml! To use this generated code, please run the following:
//
//  $ pnpm install @boundaryml/baml-core-node
//
// ----------------------------------------------------------------------------

// This file was generated by BAML: please do not edit it. Instead, edit the
// BAML files and re-generate this code using: baml-cli generate
//
// eslint-disable
// prettier-ignore

import { defineFunction } from "@boundaryml/baml-core-node";

/**
 * A job applicant's resume.
 *
 * @property name - The candidate's full name.
 * @property email - Email, when present.
 */
export class Resume {
    /** The candidate's full name. */
    name!: string;
    /** Email, when present. */
    email!: string | null;

    constructor(init: { name: string; email: string | null }) {
        Object.assign(this, init);
    }
}

export const ExtractResume = defineFunction("user.lorem.ExtractResume", "sync", ["text"]) as (text: string) => Resume;
export const ExtractResume_async = defineFunction("user.lorem.ExtractResume", "async", ["text"]) as (text: string) => Promise<Resume>;
export const ExtractResume__build_request = defineFunction("user.lorem.ExtractResume$build_request", "sync", ["text"]) as (text: string) => /* baml.http.Request type */ unknown;
export const ExtractResume__build_request_async = defineFunction("user.lorem.ExtractResume$build_request", "async", ["text"]) as (text: string) => Promise<unknown>;
// ... $render_prompt, $parse, $parse_stream companions ...
```

### Example 2: Enum

**BAML:**

```baml
/// Sentiment scale.
enum Sentiment {
    /// Smiling.
    HAPPY
    SAD
    NEUTRAL
}
```

**Python:**

```python
class Sentiment(str, enum.Enum):
    """
    Sentiment scale.

    Members:
        HAPPY: Smiling.
        SAD
        NEUTRAL
    """
    HAPPY = "HAPPY"
    SAD = "SAD"
    NEUTRAL = "NEUTRAL"
```

**TS:**

```ts
/**
 * Sentiment scale.
 *
 * @remarks
 * Members:
 * - HAPPY: Smiling.
 * - SAD
 * - NEUTRAL
 */
export enum Sentiment {
    HAPPY = "HAPPY",
    SAD = "SAD",
    NEUTRAL = "NEUTRAL",
}
```

### Example 3: Type alias

**BAML:**

```baml
type StringList = string[]
```

**Python:**

```python
StringList: typing.TypeAlias = typing.List[str]
```

**TS:**

```ts
export type StringList = string[];
```

### Example 4: Static method on a stdlib class

**BAML:**

```baml
class Pdf {
    function from_url(url: string) -> Pdf
}
```

**Python** (in `baml_sdk/baml/media/__init__.py`):

```python
from baml_core.baml_py import BamlPdf as Pdf
```

(Media classes are pure re-exports; the `from_url` method lives inside `bridge_python/src/media.rs` and is exposed on the PyO3 class.)

**TS** (in `baml_sdk/baml/media/index.ts`):

```ts
export { BamlPdf as Pdf } from "@boundaryml/baml-core-node";
export type Pdf = import("@boundaryml/baml-core-node").BamlPdf;
```

(Same — the runtime class lives in the bridge package. Phase 4 emits a value re-export under the bare name `Pdf` plus the paired `export type` so it resolves in type position; see `00a-example-ts-codegen-type-shapes.md:550-573`. The Python `as Pdf` rename is a Python-binding detail; the TS runtime exports `Pdf` directly.)

### Example 5: Cross-leaf class reference

**BAML** (in `ns_aliases_consumer/`):

```baml
class Holder {
    items StringList  // StringList is defined in ns_aliases
}
```

**TS output** (in `baml_sdk/aliases_consumer/index.ts`). `aliases` is a different namespace, so the cross-namespace ref uses the SINGLE root-namespace import, not a flattened `import { StringList } from "./aliases"`:

```ts
import { defineFunction } from "@boundaryml/baml-core-node";
import type * as baml_sdk from "..";

export class Holder {
    items!: baml_sdk.aliases.StringList;
    constructor(init: { items: baml_sdk.aliases.StringList }) {
        Object.assign(this, init);
    }
}
```

(The root-namespace alias is conventionally the SDK package root; `baml_sdk` here stands for whatever the root container is named. Because `StringList` is a type alias used in a value-bearing field annotation that TS only needs at type-check time, `import type` is correct; a field whose *runtime* construction needs a class binding to resolve would use a non-`type` `import * as ...`.)

### Example 6: Root `index.ts`

**TS output** (`baml_sdk/index.ts`). Child namespaces are re-exported with namespace-preserving `export * as <ns>` (NOT flattening `export * from`), per `00a-spec-codegen-mappings.md:56-58` and `00a-example-ts-codegen-type-shapes.md:91`:

```ts
// Banner...

import { BamlRuntime, setTypeMap } from "@boundaryml/baml-core-node";
import { FILES } from "./_inlinedbaml";
import { _TYPE_MAP } from "./_typemap";
import * as lorem from "./lorem";
import * as ipsum from "./ipsum";
import * as aliases from "./aliases";
import * as aliases_consumer from "./aliases_consumer";
// ...

BamlRuntime.initializeRuntime("baml_src", FILES);
setTypeMap(_TYPE_MAP);

export { lorem, ipsum, aliases, aliases_consumer /* , ... */ };
// Root-namespaced symbols (e.g. BAML `class Foo` with no namespace) are plain
// module-level exports — no aggregated `b` client object:
export class Foo { ... }
export const make_foo = defineFunction("user.make_foo", "sync", ["v"]) as (v: number) => Foo;
```

### Example 7: `_typemap.ts`

```ts
// Banner...

import { BamlTypeMap } from "@boundaryml/baml-core-node";

const _CLASS_ENTRIES: Record<string, [string, string]> = {
    "user.aliases_consumer.Holder": ["./aliases_consumer", "Holder"],
    "user.lorem.Resume": ["./lorem", "Resume"],
    "user.lorem.Resume$stream": ["./stream_types/lorem", "Resume"],
    // ... alphabetically sorted by FQN ...
};
const _ENUM_ENTRIES: Record<string, [string, string]> = {
    "user.ipsum.Sentiment": ["./ipsum", "Sentiment"],
};
const _ALIAS_ENTRIES: Record<string, [string, string]> = {
    "user.aliases.StringList": ["./aliases", "StringList"],
};

export const _TYPE_MAP = BamlTypeMap.fromLazyEntries({
    classes: _CLASS_ENTRIES,
    enums: _ENUM_ENTRIES,
    typeAliases: _ALIAS_ENTRIES,
});
```

---

## Assumptions Catalog

Consolidated for the implementer; each documented inline where it first appears.

- **A1.** Phase 1 has shipped `defineFunction`, `defineInstanceFunction`, `BamlStream`, `BamlTypeMap`, `setTypeMap` in `@boundaryml/baml-core-node`. If not, coordinate with Phase 1 and add them before sub-phase 4.4. Do not create a private shim inside generated SDK output.
- **A2.** Phase 2 follows the Python module layout closely. Field name divergences (e.g. `py_name` → `ts_name`) are renames; structurally identical.
- **A3.** TS recursive type aliases work natively. Phase 3's `translate_ty` emits same-leaf self-refs as the alias's bare name (not a string sentinel).
- **A4.** The runtime exports the stdlib value classes under their `Baml*` names (`BamlImage`, `BamlAudio`, `BamlVideo`, `BamlPdf`, `BamlStream`) from npm package `@boundaryml/baml-core-node` and does not alias. Codegen aliases them to the public `Image`/`Audio`/`Video`/`Pdf`/`Stream` on re-export (value + paired type). If Phase 1 chose different names, update `media_reexport_ts_name`.
- **A5.** Phase 1's `defineInstanceFunction(...).bind(self)` captures the receiver and zips it against `paramNames[0] === "self"` (matching the semantics of Python's descriptor-injected `self`). Instance-method bindings are class-field initializers; `defineFunction` (statics/free functions) zips positional args against `paramNames`.
- **A6.** Cross-namespace refs use the single root-namespace import + dotted path (`<rootns>.ns.Name`) with relative module paths (`".."`); no path aliases, no per-leaf flattened named imports.
- **A7.** `export * as <child> from "./<child>"` (namespace-preserving) exposes child namespaces without flattening their symbols; BAML's namespace design forbids collisions across user namespaces.

---

## References

- Phase overview: `/Users/sam/thoughts/sam-projects/bridge-node/00b-overview.md`
- Python prior-art cross-reference: `/Users/sam/thoughts/sam-projects/bridge-node/00b2-overview.md`
- Codegen spec / mappings: `/Users/sam/thoughts/sam-projects/bridge-node/00a-spec-codegen-mappings.md`
- Python emitter (canonical reference):
  - `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/lib.rs` (top-level walk; `render_root_init` at lines 325-339; `render_inlinedbaml` at 377-395)
  - `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/leaf.rs` (per-leaf body render; `render_symbol` ≈ line 767, `render_factory_binding` ≈ line 947, `render_method_binding` ≈ line 969)
  - `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/utils.rs` (`format_class_docstring` at lines 73-144 — analog to port for TS JSDoc)
  - `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/emit/typemap_file.rs` (full file — direct port target for `render_typemap_module_ts`)
  - `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/emit/{class,enum_,type_alias,function,method}.rs` (per-kind struct shapes; ports already in Phase 2)
- Python runtime peers Phase 4's emitted code calls:
  - `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/__init__.py:199-237` (`define_function` factory)
  - `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/_stream.py` (full file — `BamlStream` runtime wrapper)
  - `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/typemap.py` (full file — `BamlTypeMap.from_lazy_entries`)
- Node.js codegen entry point (currently a stub Phase 4 fills in):
  - `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs`
- Node.js bridge runtime (Phase 1 owner; Phase 4 consumer):
  - `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts`
  - `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts` (Phase 5 owner)
- Test driver:
  - `/Users/sam/baml3/baml_language/sdk_tests/build/src/nodejs.rs`
  - `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs/build.rs`
  - `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs/Cargo.toml`
- Test fixtures (BAML source):
  - `/Users/sam/baml3/baml_language/sdk_tests/fixtures/type_shapes/baml_src/`
  - `/Users/sam/baml3/baml_language/sdk_tests/fixtures/docstrings_etc/baml_src/`
  - `/Users/sam/baml3/baml_language/sdk_tests/fixtures/llm_functions/baml_src/`
- Test cases (jest):
  - `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs/customizable/type_shapes/main.test.ts`
  - `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs/customizable/docstrings_etc/main.test.ts`
  - `/Users/sam/baml3/baml_language/sdk_tests/crates/nodejs/customizable/llm_functions/main.test.ts`
- Python equivalents (helpful for understanding test contract):
  - `/Users/sam/baml3/baml_language/sdk_tests/crates/python_pydantic2/customizable/`
