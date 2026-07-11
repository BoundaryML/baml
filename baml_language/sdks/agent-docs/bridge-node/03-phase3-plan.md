# Phase 3 Plan: Implement translate_ty (sdkgen_nodejs)

## Overview

Phase 3 of the Node.js / TypeScript SDK port delivers a single pure function — `translate_ty(ty, ctx) -> TranslatedType` — that converts every variant of `baml_codegen_types::Ty` to the corresponding TypeScript type expression as a string. This is the engine that Phase 4 will call from every per-leaf emitter (class fields, function args, function return types, type-alias bodies, method signatures). Phase 3 owns the BAML→TS type-mapping table and an exhaustive unit-test matrix; Phase 3 does NOT wire the function into any emitter.

The Python prior art (`/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/translate_ty.rs`, 851 lines) is the model: a 1-to-1 port for the structure and the test matrix, with TS-idiomatic substitutions for each mapping.

## Goal

Delivery criteria:

1. `sdkgen_nodejs::translate_ty::translate_ty(ty, ctx)` is implemented and exhaustively covers every `Ty` variant.
2. The function returns a `TranslatedType { expr: String, imports: BTreeSet<LeafPath> }` where:
   - `expr` is a TypeScript type expression embeddable inside a class field, function param, or return-type annotation
   - `imports` is the set of cross-leaf `LeafPath`s referenced by `expr` (Phase 4 will materialize these as TS `import` statements)
3. The unit-test matrix inside the module passes via `cargo test -p sdkgen_nodejs translate_ty`.
4. The test matrix structurally mirrors `codegen_python::translate_ty::tests::translate_ty_covers_phase_g3_matrix` — every Python `Case` has a Node.js counterpart with the TS-idiomatic expectation.

Phase 3 does not move the global TDD anchor (`cargo nextest run -E 'package(/^sdk_test_nodejs_/)'`); those tests stay red until Phase 4 wires `translate_ty` into emitters.

## Current State Analysis

### What Phase 2 delivered (assumed)

- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/routing.rs` — Node analog of `codegen_python/src/routing.rs`, exposing:
  - `pub(crate) struct LeafPath { pub(crate) segments: Vec<String> }`
  - `pub(crate) fn route_class_ref(name: &Name) -> LeafPath`
  - `pub(crate) fn route(name: &Name, symbol: &Symbol) -> LeafPath`
  - The same routing rules as Python: user under root, vendor under `vendor/<pkg>/…`, baml under `baml/…`, `$stream` classes under `stream_types/…`. Module-segment sanitation (e.g. `assert` → `assert_`) carries over verbatim — TypeScript also reserves `assert` (it's a contextual type-system keyword) and a bare `import * as assert from …` is benign but `assert` as an identifier collides with `node:assert` ambient typings. The same sanitizer policy is fine to inherit.
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs` — the stub crate entry point with `pub fn to_source_code(...)`. Phase 2 will have added `mod routing;` (and presumably stubs for `mod emit;`/`mod leaf;`).
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/Cargo.toml` already depends on `baml_codegen_types`. Phase 3 needs to add `baml_base = { workspace = true }` for `Literal`, `MediaKind`, and `Name` access.

### What does not exist yet at Phase 3 start

- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/translate_ty.rs` — must be created.
- No `ts_string` string-escaping helper — Phase 3 introduces one (TS uses `"` strings with `\\`, `\"`, `\n`, `\r`, `\t` escapes, same shape as Python's `py_string` in `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/lib.rs:400`).
- No TS analog of `Ty::BamlOptions` runtime symbol yet — Phase 1/2 will eventually expose `BamlOptions` from `baml_sdk/baml/index`. Phase 3 emits the bare reference `baml.Options` (analogous to Python) and lets Phase 4 wire imports.
- No `_BamlNodeHandle` or `BamlHandle` re-export from `baml_sdk/index` yet — Phase 1's `bridge_nodejs` package owns the `BamlHandle` class (already exported at `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts:17`). Phase 3 emits the literal token `_BamlHandle` (a leading-underscore alias the emitter will pull in, mirroring Python's `_BamlPyHandle` decision; the spec maps `Ty::RustType` → `BamlHandle` / runtime opaque handle imported from `@boundaryml/baml-core-node` in the 00a-spec "Exhaustive Ty conversions" table).

### Key discoveries from the Python prior art

- `TranslateCtx` is three fields: `current_leaf`, `self_ref`, `defer_name_refs`. The first is intrinsic to TS too; the third is a Python-specific workaround for `TypeAliasType(...)` eagerness — TypeScript does NOT need it (see assumption A4 below). Phase 3 drops `defer_name_refs` from `TranslateCtx`.
- `should_quote_self_ref` exists because Python class bodies don't have the class name in scope while parsing the class body. TypeScript's type system resolves class-body forward references natively (interface `Node<T> { children: Node<T>[] }` compiles fine). Phase 3 drops self-ref quoting too (assumption A4).
- `render_name_ref` is the only place where `route_class_ref` is consulted. Phase 3 splits this into `render_name_ref_string` (returns the bare-or-dotted string) and `note_cross_leaf_import` (records the `LeafPath` in `TranslatedType.imports`).

## What We're NOT Doing

Strictly out of scope for Phase 3:

- **Phase 1 (bridge runtime)**: media classes (`Image`/`Audio`/`Video`/`Pdf`), `BamlHandle`, `BamlOptions`. Phase 3 references their generated TS symbols by name only.
- **Phase 2 (codegen scaffolding)**: routing, file-layout enumeration, placeholder symbols, `LeafBody`/`PyClass`/`PyEnum`/`PyFunction` analogs. Phase 3 imports `route_class_ref` from Phase 2's `routing.rs`.
- **Phase 4 (emitters)**: wiring `translate_ty` into class-field rendering, function-signature rendering, type-alias-body rendering, method-signature rendering. Phase 4 also owns import collection — it walks `TranslatedType.imports` from every call to `translate_ty` made by a given leaf and emits the `import { Foo } from '../path'` statements at the top of the leaf's `.ts`/`.d.ts` files.
- **Phase 4 (docstrings)**: type expressions don't carry doc comments — those belong on the enclosing field/param/return position, which Phase 4 owns.
- **Phase 5 (proto encode/decode)**: runtime BAML→TS type mapping for class lookup (the runtime mirror of `_baml_ty_to_python_type` at `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/proto.py:234-291`).
- **Phase 6 (release)**: nothing about packaging or CI in Phase 3.

Also explicitly out of scope:

- **TS namespace syntax** (`namespace lorem { export class Resume {} }`). TypeScript modules are not namespaces; we use module imports. The cross-leaf reference `lorem.Resume` is a *root-relative symbolic dotted path*. Phase 4 resolves it by importing the package root once (`import type * as <rootns> from "..";`) and prefixing the alias: `<rootns>.lorem.Resume`. Phase 3 emits the dotted tail as a placeholder plus the import set; Phase 4 prefixes the root alias (per spec, NOT per-leaf flattened imports — see Assumption A6).
- **JSDoc emission inside type expressions** (`/** @internal */ Resume`). All JSDoc lives outside the type expression.
- **Generic param declarations** (e.g. `class Box<T> { … }` — the `<T>` slot). `translate_ty` only sees `Ty::TypeVar(name)` references *inside* a type expression and renders the bare name; Phase 4 owns the generic-param declaration.
- **Conditional types, mapped types, template literal types**. None of the BAML `Ty` variants require them.
- **`Checked<T>` / `@check`, `StreamState<T>` / `@stream.state`, `@@dynamic`**. These are dropped from the codegen surface (00a-spec "Exhaustive Ty conversions" §Notes). They never appear as `baml_codegen_types::Ty` variants, so `translate_ty` has nothing to handle — no `Partial<T>` / `StreamState<T>` wrapping is ever derived. `$stream` companion classes are consumed as ordinary `Ty::Class` references whose `Name` carries the `$stream` suffix (routed via `route_class_ref` to `stream_types/…`); their optional-field shape is produced upstream by the compiler, not by `translate_ty`.

## Implementation Approach

### Public API

```rust
// sdkgen_nodejs/src/translate_ty.rs

use std::collections::BTreeSet;

use baml_base::{Literal, MediaKind};
use baml_codegen_types::{Name, Ty};

use crate::routing::{LeafPath, route_class_ref};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranslateCtx {
    pub(crate) current_leaf: LeafPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TranslatedType {
    pub(crate) expr: String,
    pub(crate) imports: BTreeSet<LeafPath>,
}

pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> TranslatedType { … }
```

Rationale for returning a struct rather than just `String`:

- TypeScript needs explicit `import` statements; the dotted-path placeholder `lorem.Resume` can't be left in the final source the way Python's lazy `__getattr__` allows. Phase 4 needs to know *which* leaves to import; collecting that during translation (rather than re-walking the AST later) is simpler and matches how TS emitters in the wild (e.g. `tsc`, `swc`) work.
- The Python version returns just `String` because Python wraps cross-leaf imports in `if typing.TYPE_CHECKING:` blocks at runtime and uses lazy `__getattr__` for the runtime path; the codegen doesn't need a precise import set per type. TS is stricter — every type reference must resolve.

### Same-leaf vs cross-leaf

For each `Ty::Class` / `Ty::Enum` / `Ty::TypeAlias`:

1. `let routed = route_class_ref(name);`
2. If `routed == ctx.current_leaf` or `routed.segments.is_empty()` and `ctx.current_leaf.segments.is_empty()` → emit bare name, no import.
3. If `routed.segments.is_empty()` (root leaf) but `ctx.current_leaf` is non-root → emit bare name (root-routed symbols are re-exported from every leaf via the root barrel; alternative is to emit `_root.Foo` but bare is cleaner — see assumption A3).
4. Otherwise → emit `<dotted_path>.BareName` (where `<dotted_path>` is `routed.segments.join(".")`) and add `routed` to the `imports` set.

The dotted-path string (e.g. `lorem.Resume`, `vendor.aws.s3.Bucket`, `stream_types.lorem.Resume`) is a stable, root-relative placeholder that Phase 4 prefixes with a single root-namespace alias. The dotted path *is* the namespace path from the package root, so the only import a leaf needs — regardless of how many distinct cross-leaf symbols it references — is one root import.

**Assumption A6** (corrected to match the re-edited spec): Phase 4 emits a SINGLE root-namespace import per leaf and references every cross-leaf symbol through it:

```ts
import type * as symbol_collisions from "..";
// ...
bar1!: symbol_collisions.foo.Bar;
bar2!: symbol_collisions.fizz.foo.Bar;
```

This is the rule mandated by the 00a-spec appendix ("Cross-Namespace References" correct/wrong block). Phase 4 does NOT emit per-leaf flattened imports such as `import * as symbol_collisions_fizz_foo from "../fizz/foo"` and `bar2!: symbol_collisions_fizz_foo.Bar` — that form is explicitly called out as wrong in the spec because it maximizes the chance of name collisions. The root-namespace alias (e.g. `import type * as <rootns> from ".."`, using the relative path back to the package root) plus the fully-qualified dotted path leaves minimal room for collisions.

⚠ Spec note: the spec's appendix example imports `from ".."` (one level up). The exact relative depth (`".."`, `"../.."`, …) depends on the current leaf's directory depth, which Phase 4 computes from `ctx.current_leaf`. Phase 3 only emits the root-relative *dotted* path; Phase 4 owns choosing the relative specifier and the root alias name.

Edge case: multi-segment cross-leaf paths (e.g. `vendor.aws.s3.Bucket`) require no special handling under the single-root-import rule — `vendor.aws.s3.Bucket` is already the fully-qualified path from the root namespace, so prefixing the root alias yields `<rootns>.vendor.aws.s3.Bucket` directly. The container barrels (`vendor/index.ts`, `vendor/aws/index.ts`) that re-export child namespaces make this path resolvable; Phase 4 emits those barrels. **Assumption A7**: `translate_ty` records the *full* root-relative `LeafPath` in `imports` purely so Phase 4 knows the package root must be imported; the dotted `expr` is reused verbatim under the root alias.

Concretely: Phase 3 emits the root-relative dotted path that mirrors Python's output verbatim, and Phase 4 prefixes it with the single root-namespace alias. The Phase 3 test matrix asserts on the dotted-path form (matching Python).

### Self-reference and recursive aliases (dropped)

TypeScript handles forward references in class bodies and type aliases natively:

```typescript
// Class self-reference: works without quoting
class Node<T> { children: Node<T>[] = [] }

// Recursive type alias: works directly
type JsonValue = string | number | boolean | null | JsonValue[] | { [k: string]: JsonValue };

// Mutual recursion across classes in the same file: works
class A { b: B | null = null }
class B { a: A | null = null }
```

So Phase 3 drops `SelfRef` and `defer_name_refs` from `TranslateCtx`. The Python self-ref test cases collapse: every `ctx_with_self(…)` and `ctx_recursive_alias_body(…)` case in the Python matrix becomes a plain `ctx(…)` case in the TS matrix, with the expected output identical to the non-self-ref case (no quoting).

One caveat to investigate: TypeScript *does* reject some "directly recursive type aliases" (e.g. `type T = T`) at the type-checker level. But indirect recursion through a constructor like `T[]`, union, or record is fine — which is exactly what recursive BAML type aliases produce (a recursive alias is always `RecList = number | RecList[]` or similar, never a bare self-loop). **Assumption A4**: TS handles every BAML recursive alias shape without quoting. If a counterexample is found during Phase 3, document it and add a fallback (re-introduce `defer_name_refs` and emit `T extends T ? T : never` or similar). The test matrix should include the recursive-alias cases from the Python matrix to validate this.

### Test infrastructure

The test matrix lives in `#[cfg(test)] mod tests` inside `translate_ty.rs`, mirroring the Python file. The `Case` struct, `ctx(...)` helper, and `name(...)` helper port verbatim. The `assert_ty` helper extends to also assert on `imports`:

```rust
struct Case {
    label: &'static str,
    ty: Ty,
    ctx: TranslateCtx,
    expected_expr: &'static str,
    expected_imports: &'static [&'static [&'static str]],  // each &[&str] is one LeafPath's segments
}

fn assert_ty(case: &Case) {
    check_exhaustive(&case.ty);
    let result = translate_ty(&case.ty, &case.ctx);
    assert_eq!(result.expr, case.expected_expr, "expr mismatch for case {}", case.label);
    let expected_imports: BTreeSet<LeafPath> = case
        .expected_imports
        .iter()
        .map(|segs| LeafPath { segments: segs.iter().map(|s| s.to_string()).collect() })
        .collect();
    assert_eq!(result.imports, expected_imports, "imports mismatch for case {}", case.label);
}
```

`check_exhaustive` is the same one-arm match-on-`Ty` from Python — it forces the test file to be updated whenever a `Ty` variant is added.

---

## Phase 3.1: Scaffold module + primitives + literals

### Overview

Get the file existing, `mod translate_ty;` wired into `lib.rs`, and the simplest variants returning correct TS.

### Changes Required

**File**: `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/translate_ty.rs` (new)

Contents (skeleton):

```rust
//! BAML `Ty` → TypeScript type-expression translation. Pure function;
//! Phase 4 emitters call this from every type position (class fields,
//! function args, return types, type-alias bodies, method signatures).
//!
//! Rule sources:
//! - /Users/sam/thoughts/sam-projects/bridge-node/00a-spec-codegen-mappings.md §"Exhaustive Ty conversions"
//! - /Users/sam/thoughts/sam-projects/bridge-node/03-phase3-plan.md §"BAML→TS Type Map Table"
//! - Python prior art: /Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/translate_ty.rs

use std::collections::BTreeSet;

use baml_base::{Literal, MediaKind};
use baml_codegen_types::{Name, Ty};

use crate::{
    routing::{LeafPath, route_class_ref},
    ts_string,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TranslateCtx {
    pub(crate) current_leaf: LeafPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TranslatedType {
    pub(crate) expr: String,
    pub(crate) imports: BTreeSet<LeafPath>,
}

impl TranslatedType {
    fn bare(expr: impl Into<String>) -> Self {
        Self { expr: expr.into(), imports: BTreeSet::new() }
    }

    fn map_expr(mut self, f: impl FnOnce(String) -> String) -> Self {
        self.expr = f(self.expr);
        self
    }

    fn merge_from(&mut self, other: &TranslatedType) {
        for p in &other.imports {
            self.imports.insert(p.clone());
        }
    }
}

pub(crate) fn translate_ty(ty: &Ty, ctx: &TranslateCtx) -> TranslatedType {
    match ty {
        Ty::Int => TranslatedType::bare("number"),
        Ty::Bigint => TranslatedType::bare("bigint"),
        Ty::Float => TranslatedType::bare("number"),
        Ty::String => TranslatedType::bare("string"),
        Ty::Bool => TranslatedType::bare("boolean"),
        Ty::Null => TranslatedType::bare("null"),
        Ty::Uint8Array => TranslatedType::bare("Uint8Array"),
        Ty::BuiltinUnknown => TranslatedType::bare("unknown"),
        Ty::Unit => TranslatedType::bare("null"),
        Ty::BamlOptions => TranslatedType::bare("baml.Options"),
        Ty::RustType => TranslatedType::bare("_BamlHandle"),
        Ty::Literal(Literal::Int(value)) => TranslatedType::bare(format!("{value}")),
        // `bigint` literal types use the `n` suffix in TypeScript: `42n`.
        Ty::Literal(Literal::Bigint(value)) => TranslatedType::bare(format!("{value}n")),
        Ty::Literal(Literal::String(value)) => TranslatedType::bare(ts_string(value)),
        Ty::Literal(Literal::Bool(true)) => TranslatedType::bare("true"),
        Ty::Literal(Literal::Bool(false)) => TranslatedType::bare("false"),
        Ty::Literal(Literal::Float(_)) => TranslatedType::bare("number"),
        Ty::Media(MediaKind::Image) => media_ref("Image", ctx),
        Ty::Media(MediaKind::Audio) => media_ref("Audio", ctx),
        Ty::Media(MediaKind::Video) => media_ref("Video", ctx),
        Ty::Media(MediaKind::Pdf) => media_ref("Pdf", ctx),
        Ty::Media(MediaKind::Generic) => TranslatedType::bare("unknown"),
        // … remaining variants in subsequent sub-phases
        _ => unimplemented!("phase 3.{N}"),
    }
}

fn media_ref(bare: &str, ctx: &TranslateCtx) -> TranslatedType {
    let name = Name::new(
        baml_base::Name::new("baml"),
        vec![baml_base::Name::new("media")],
        baml_base::Name::new(bare),
    );
    render_name_ref(&name, ctx)
}

fn render_name_ref(name: &Name, ctx: &TranslateCtx) -> TranslatedType {
    let routed = route_class_ref(name);
    if routed == ctx.current_leaf || routed.segments.is_empty() {
        TranslatedType::bare(name.bare_name().to_string())
    } else {
        let dotted = routed.segments.join(".");
        let mut imports = BTreeSet::new();
        imports.insert(routed);
        TranslatedType {
            expr: format!("{dotted}.{}", name.bare_name()),
            imports,
        }
    }
}
```

**File**: `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs`

**Change**: Add `mod translate_ty;` near the existing `mod routing;` (Phase 2 added that). Add `pub(crate) fn ts_string(s: &str) -> String` (port of `py_string` from `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/lib.rs:400-418`, using TS's `"..."` form — TS escape rules for `\\`, `\"`, `\n`, `\r`, `\t`, and `\xNN` for control bytes are identical to Python's).

```rust
pub(crate) fn ts_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                write!(out, "\\x{:02x}", c as u32).unwrap();
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
```

**File**: `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/Cargo.toml`

**Change**: Add `baml_base = { workspace = true }` to `[dependencies]`. This is needed for `Literal`, `MediaKind`, and `Name`.

### Success Criteria

#### Automated Verification:
- [ ] `cargo build -p sdkgen_nodejs` compiles
- [ ] `cargo test -p sdkgen_nodejs translate_ty -- --test-threads=1` runs (matrix still partial; cases for primitives/literals/media/builtin/unit pass) — including `Ty::Bigint` → `bigint` and `Ty::Literal(Bigint(_))` → `42n`
- [ ] `cargo clippy -p sdkgen_nodejs -- -D warnings` clean

#### Manual Verification:
- [ ] `ts_string("has \"quotes\"")` returns `"\"has \\\"quotes\\\"\""`

---

## Phase 3.2: Container types (Optional, List, Map, Union)

### Overview

Wire the recursive container variants. These compose by calling `translate_ty` on their inner type(s) and using `merge_from` to gather imports.

### Changes Required

**File**: `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/translate_ty.rs`

Add to the `match ty {` arms:

```rust
Ty::Optional(inner) => {
    let inner = translate_ty(inner, ctx);
    TranslatedType {
        expr: format!("{} | null", inner.expr),
        imports: inner.imports,
    }
}
Ty::List(inner) => {
    let inner = translate_ty(inner, ctx);
    // Spec mandates the postfix form `T[]` (00a-spec Ty table: `List(T)` → `T[]`),
    // matching every worked example in 00a-example-ts-codegen-type-shapes.md
    // (`string[]`, etc.). Postfix `[]` binds tighter than `|`, so a union/optional
    // element must be parenthesized: `Optional<String>` inside a list renders as
    // `(string | null)[]`, not `string | null[]`.
    let elem = if inner.expr.contains(" | ") {
        format!("({})", inner.expr)
    } else {
        inner.expr
    };
    TranslatedType {
        expr: format!("{elem}[]"),
        imports: inner.imports,
    }
}
Ty::Map { key, value } => {
    let key = translate_ty(key, ctx);
    let value = translate_ty(value, ctx);
    let mut imports = key.imports;
    imports.extend(value.imports);
    TranslatedType {
        expr: format!("Record<{}, {}>", key.expr, value.expr),
        imports,
    }
}
Ty::Union(items) => {
    let mut imports = BTreeSet::new();
    let parts: Vec<String> = items
        .iter()
        .map(|item| {
            let t = translate_ty(item, ctx);
            imports.extend(t.imports);
            t.expr
        })
        .collect();
    TranslatedType {
        expr: parts.join(" | "),
        imports,
    }
}
```

Notes:

- `T[]` vs `Array<T>`: the 00a-spec Ty table mandates the postfix form `T[]` (List(T) → `T[]`), and every worked example in `00a-example-ts-codegen-type-shapes.md` uses it (`string[]`, etc.). Use `T[]`, parenthesizing union/optional element types (`(string | null)[]`). The Python prior art uses the verbose `typing.List[T]`; `T[]` is the spec-mandated TS analog.
- `Record<K, V>` is the right choice because `Ty::Map` validation (`/Users/sam/baml3/baml_language/crates/baml_codegen_types/src/ty.rs:181-184`) guarantees the key is `Ty::String` or `Ty::Enum(_)` — both of which TypeScript accepts as `Record` keys. Enums in TS are assignable to `string` (numeric enums are also assignable, but BAML enums always have string-shaped values). The Python equivalent is `typing.Dict[K, V]`.
- `Ty::Union(items)` flat union — `validate()` on `Ty::Union` rejects nested unions, `Optional`, `Null`, `Unit`, so the rendered TS union is always flat. No parens needed.
- `Ty::Optional(T)` → `T | null` over `T | undefined`: matches what `proto.ts` returns (`null` from `decode_value`, see `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts:79,124`). Documented as assumption A1 below.

### Success Criteria

#### Automated Verification:
- [ ] `cargo test -p sdkgen_nodejs translate_ty` — container test cases pass
- [ ] `Ty::Optional(Box::new(Ty::String))` produces `string | null`
- [ ] `Ty::List(Box::new(Ty::Int))` produces `number[]`
- [ ] `Ty::Map { key: Box::new(Ty::String), value: Box::new(Ty::Int) }` produces `Record<string, number>`
- [ ] `Ty::Union(vec![Ty::Int, Ty::String, Ty::Bool])` produces `number | string | boolean`
- [ ] Nested cases compose correctly (e.g. `Optional<List<String>>` → `string[] | null`, `List<Optional<String>>` → `(string | null)[]`)

---

## Phase 3.3: Class / Enum / TypeAlias name refs + cross-leaf imports

### Overview

Wire `Ty::Class(name, args)`, `Ty::Enum(name)`, `Ty::TypeAlias(name)`. This is where `render_name_ref` does its real work and where the `imports` set first gets populated.

### Changes Required

**File**: `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/translate_ty.rs`

Add to the `match ty {` arms:

```rust
Ty::Class(name, args) => {
    let mut result = render_name_ref(name, ctx);
    if !args.is_empty() {
        let mut arg_imports = BTreeSet::new();
        let arg_strs: Vec<String> = args
            .iter()
            .map(|a| {
                let t = translate_ty(a, ctx);
                arg_imports.extend(t.imports);
                t.expr
            })
            .collect();
        result.expr = format!("{}<{}>", result.expr, arg_strs.join(", "));
        result.imports.extend(arg_imports);
    }
    result
}
Ty::Enum(name) => render_name_ref(name, ctx),
Ty::TypeAlias(name) => render_name_ref(name, ctx),
Ty::TypeVar(name) => TranslatedType::bare(name.as_str().to_string()),
```

The `route_class_ref` helper from Phase 2's `routing.rs` already does the heavy lifting:

- `name("user", &["lorem"], "Resume")` → `LeafPath { segments: ["lorem"] }` → emits `Resume` (same leaf as `ctx.current_leaf == ["lorem"]`) or `lorem.Resume` (cross-leaf)
- `name("user", &["lorem"], "Resume$stream")` → `LeafPath { segments: ["stream_types", "lorem"] }` → emits `stream_types.lorem.Resume`
- `name("aws", &["s3"], "Bucket")` → `LeafPath { segments: ["vendor", "aws", "s3"] }` → emits `vendor.aws.s3.Bucket`
- `name("baml", &["http"], "Response")` → `LeafPath { segments: ["baml", "http"] }` → emits `baml.http.Response`
- `name("user", &[], "Foo")` → `LeafPath { segments: [] }` (root leaf) → emits `Foo` for any `ctx.current_leaf` (the root leaf is special-cased in `render_name_ref`)

For each cross-leaf reference, the `LeafPath` is inserted into `imports`. Phase 4 collects these.

### Success Criteria

#### Automated Verification:
- [ ] `cargo test -p sdkgen_nodejs translate_ty` — class/enum/alias cases pass
- [ ] Same-leaf class reference returns bare name and empty imports
- [ ] Cross-leaf class reference returns dotted path and `imports == {routed_leaf}`
- [ ] `$stream` class reference correctly prefixes with `stream_types`
- [ ] Vendor and baml stdlib references emit the correct prefix
- [ ] Root-leaf names emit bare even from non-root leaves (no spurious import — relies on Phase 4 re-exporting root names from every leaf or globally; documented as assumption A3)

---

## Phase 3.4: Generics, Callable

### Overview

The two remaining variants. `Ty::Class` already handles its `args` slot in Phase 3.3; the generics test cases are just additional `Case`s. `Ty::Callable` is new in this sub-phase.

### Changes Required

**File**: `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/translate_ty.rs`

Add to the `match ty {` arms:

```rust
Ty::Callable { params, ret } => {
    let ret_t = translate_ty(ret, ctx);
    let mut imports = ret_t.imports;
    let any_optional = params
        .iter()
        .any(|p| p.mode == baml_codegen_types::CodegenFunctionParamMode::Optional);
    let expr = if any_optional {
        // TS can't express per-param optionality in a function type
        // without naming each param. Mirror Python's fallback: collapse
        // to a rest-args function type.
        format!("(...args: unknown[]) => {}", ret_t.expr)
    } else {
        let param_strs: Vec<String> = params
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let t = translate_ty(&p.ty, ctx);
                imports.extend(t.imports);
                let arg_name = p
                    .name
                    .as_ref()
                    .map(|n| n.as_str().to_string())
                    .unwrap_or_else(|| format!("arg{idx}"));
                format!("{arg_name}: {}", t.expr)
            })
            .collect();
        format!("({}) => {}", param_strs.join(", "), ret_t.expr)
    };
    TranslatedType { expr, imports }
}
```

Notes:

- TS function-type syntax `(p: T) => R` requires named parameters, unlike Python's `typing.Callable[[T], R]`. When the BAML `CallableParam.name` is `None`, fabricate `arg0`, `arg1`, etc. The names are purely for the type — they don't affect call-site usage.
- The optional-param fallback: `(...args: unknown[]) => R` is the TS equivalent of Python's `typing.Callable[..., R]`. Document as assumption A2.
- `Ty::Callable` cases will exercise generics-inside-callable, e.g. `Callable<[List<Int>], Optional<String>>` → `(arg0: number[]) => string | null`.

### Generics test cases

Generics test cases inherit from Phase 3.3 — `Ty::Class(name, vec![Ty::Int])` produces `Box<number>` (same leaf) or `lorem.Box<number>` (cross-leaf). Add the Python-matrix cases verbatim:

```
generic class same leaf concrete int            : Box<number>
generic class cross leaf concrete int           : lorem.Box<number>     imports: {lorem}
generic class with list arg                     : Box<number[]>
generic class nested generic arg                : Box<Box<number>>
generic class stream from non-stream leaf       : stream_types.lorem.Box<number>     imports: {stream_types/lorem}
generic class with typevar arg                  : Box<T>
bare typevar                                    : T
map with typevar key and value                  : Record<string, V>
```

### Success Criteria

#### Automated Verification:
- [ ] `cargo test -p sdkgen_nodejs translate_ty` — callable + generics cases pass
- [ ] Callable with required params produces `(arg0: T0, arg1: T1) => Ret`
- [ ] Callable with optional params produces `(...args: unknown[]) => Ret`
- [ ] Callable with zero params produces `() => Ret`
- [ ] Generic class produces `Name<Arg1, Arg2>`
- [ ] Nested generics compose (e.g. `Box<Box<number>>`)

---

## Phase 3.5: Exhaustive test matrix + check_exhaustive

### Overview

Port every `Case` from `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/translate_ty.rs:307-845` to the Node module, adapt expected strings to TS, and assert on `imports` for every cross-leaf case.

### Changes Required

**File**: `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/translate_ty.rs`

Add `#[cfg(test)] mod tests` block. Port the helpers:

```rust
#[cfg(test)]
mod tests {
    use baml_base::Name as BaseName;
    use super::*;

    struct Case {
        label: &'static str,
        ty: Ty,
        ctx: TranslateCtx,
        expected_expr: &'static str,
        expected_imports: &'static [&'static [&'static str]],
    }

    fn leaf(segments: &[&str]) -> LeafPath {
        LeafPath { segments: segments.iter().map(ToString::to_string).collect() }
    }
    fn ctx(segments: &[&str]) -> TranslateCtx {
        TranslateCtx { current_leaf: leaf(segments) }
    }
    fn name(pkg: &str, namespace_path: &[&str], bare_name: &str) -> Name {
        Name::new(
            BaseName::new(pkg),
            namespace_path.iter().map(|s| BaseName::new(*s)).collect(),
            BaseName::new(bare_name),
        )
    }
    fn callable_param(ty: Ty) -> baml_codegen_types::CallableParam {
        baml_codegen_types::CallableParam {
            name: None,
            ty,
            mode: baml_codegen_types::CodegenFunctionParamMode::Required,
        }
    }
    fn optional_callable_param(name: &str, ty: Ty) -> baml_codegen_types::CallableParam {
        baml_codegen_types::CallableParam {
            name: Some(BaseName::new(name)),
            ty,
            mode: baml_codegen_types::CodegenFunctionParamMode::Optional,
        }
    }

    fn assert_ty(case: &Case) {
        check_exhaustive(&case.ty);
        let result = translate_ty(&case.ty, &case.ctx);
        assert_eq!(
            result.expr, case.expected_expr,
            "expr mismatch for case {}", case.label
        );
        let expected_imports: BTreeSet<LeafPath> = case
            .expected_imports
            .iter()
            .map(|segs| LeafPath {
                segments: segs.iter().map(|s| s.to_string()).collect(),
            })
            .collect();
        assert_eq!(
            result.imports, expected_imports,
            "imports mismatch for case {}", case.label
        );
    }

    fn check_exhaustive(ty: &Ty) {
        match ty {
            Ty::Int | Ty::Bigint | Ty::Float | Ty::String | Ty::Bool | Ty::Null
            | Ty::Literal(_) | Ty::Uint8Array | Ty::Media(_)
            | Ty::Class(_, _) | Ty::Enum(_) | Ty::TypeAlias(_) | Ty::TypeVar(_)
            | Ty::Optional(_) | Ty::List(_) | Ty::Map { .. } | Ty::Union(_)
            | Ty::BuiltinUnknown | Ty::Callable { .. } | Ty::Unit
            | Ty::RustType | Ty::BamlOptions => {}
        }
    }

    #[test]
    fn translate_ty_covers_phase3_matrix() {
        let cases = vec![ /* full case list — see "TS Test Matrix" section below */ ];
        for case in &cases {
            assert_ty(case);
        }
    }
}
```

The full case list (in vec order, identical structure to Python tests) is given in the **TS Test Matrix** section below.

### Success Criteria

#### Automated Verification:
- [ ] `cargo test -p sdkgen_nodejs translate_ty` — full matrix passes (every Python case has a TS counterpart)
- [ ] `cargo clippy -p sdkgen_nodejs -- -D warnings` clean
- [ ] `cargo fmt -p sdkgen_nodejs -- --check` clean
- [ ] Adding a new `Ty` variant fails to compile in `check_exhaustive` — verified by temporarily adding a fake variant and checking `cargo build` fails

#### Manual Verification:
- [ ] Every `Case` in `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/translate_ty.rs:307-845` has a counterpart in the new TS test (use Python labels verbatim — `"int"`, `"float"`, `"class same leaf root namespace"`, etc.)
- [ ] Cases marked self-ref / recursive-alias in the Python matrix produce *unquoted* output in the TS matrix (TS recursion works natively — see assumption A4)
- [ ] Phase 3.5 file diff in `sdkgen_nodejs/src/translate_ty.rs` is roughly 600–800 lines (mostly test data), similar to the Python file's ~700-line test block

---

## Testing Strategy

### Unit Tests (inside `translate_ty.rs`)

- One `#[test] fn translate_ty_covers_phase3_matrix` with the full case list (every Python case ported plus the imports assertions).
- `check_exhaustive(&ty)` guards against new `Ty` variants slipping through; the test will fail to compile when `Ty` grows.
- Tests must run with `cargo test -p sdkgen_nodejs translate_ty` and pass with no warnings.

### Integration Tests

- **None for Phase 3.** Phase 4 is where `translate_ty` becomes observable from the SDK test harness. The overall TDD anchor `cargo nextest run -E 'package(/^sdk_test_nodejs_/)'` stays red through Phase 3.

### TDD inner loop

For each sub-phase 3.1 → 3.5:

1. Write the test cases first (commented out or with `expected_expr: "TODO"`).
2. Implement the production code in `translate_ty.rs`.
3. Fill in the expected strings.
4. Run `cargo test -p sdkgen_nodejs translate_ty -- --nocapture` and iterate until green.

The Python prior art is the source of truth for the case set — when in doubt, mirror the Python label, mirror the input `Ty` and `ctx`, and translate the `expected` string mechanically using the type map table below.

---

## BAML → TS Type Map Table

This is the Phase 3 contract. Phase 4 emitters consume this map.

| TIR `Ty` variant | Example BAML | Python output (existing) | **Generated TS expression** | Notes |
|---|---|---|---|---|
| `Ty::Int` | `age int` | `int` | `number` | JS `number` is 53-bit-safe; BAML i64 round-trip via `proto.ts` already uses `Number(...)` on `int64` (`/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts:81`). |
| `Ty::Bigint` | `value bigint` | `int` (Python widens to `int`) | `bigint` | TS has a native `bigint` type; unlike Python (which collapses `Ty::Bigint` to `int`), TS keeps it distinct. Matches `proto.ts`'s `bigintValue` decode output (a `bigint`). |
| `Ty::Float` | `score float` | `float` | `number` | TS doesn't distinguish int from float. |
| `Ty::String` | `name string` | `str` | `string` | |
| `Ty::Bool` | `active bool` | `bool` | `boolean` | |
| `Ty::Null` | `null` in union | `None` | `null` | TS uses `null` as both type and value (no separate `None`). |
| `Ty::Uint8Array` | `data uint8array` | `bytes` | `Uint8Array` | Browser-portable. Matches what `proto.ts` returns. Not `Buffer` (Node-only). |
| `Ty::Media(Image)` | `photo image` | `baml_sdk.baml.media.Image` | `baml.media.Image` | Cross-leaf placeholder routed to the `baml/media` leaf. Per the spec, that leaf re-exports the media classes from the runtime package `@boundaryml/baml-core-node` (it does not emit a generated `class` body); the type still resolves through the root-relative dotted path `baml.media.Image`. Phase 4 owns the `baml/media` re-export. |
| `Ty::Media(Audio)` | `clip audio` | `baml_sdk.baml.media.Audio` | `baml.media.Audio` | |
| `Ty::Media(Video)` | `clip video` | `baml_sdk.baml.media.Video` | `baml.media.Video` | |
| `Ty::Media(Pdf)` | `doc pdf` | `baml_sdk.baml.media.Pdf` | `baml.media.Pdf` | |
| `Ty::Media(Generic)` | n/a — defensive | `typing.Any` | `unknown` | TS counterpart of Python `Any`. |
| | | | | |
| `Ty::Literal(Int(v))` | `answer 42` | `typing.Literal[42]` | `42` | TS literal types use the value directly. |
| `Ty::Literal(Bigint(v))` | `answer 42n` | `typing.Literal[42]` (Python widens) | `42n` | TS `bigint` literal types use the `n` suffix. Python widens bigint literals to `typing.Literal[42]`; TS keeps the `n`. |
| `Ty::Literal(String(v))` | `status "draft"` | `typing.Literal["draft"]` | `"draft"` | Use `ts_string(v)` to escape. |
| `Ty::Literal(Bool(true))` | `flag true` | `typing.Literal[True]` | `true` | |
| `Ty::Literal(Bool(false))` | `flag false` | `typing.Literal[False]` | `false` | |
| `Ty::Literal(Float(s))` | n/a — parser rejects | `typing.Any` | `number` | Fallback; matches Python. |
| | | | | |
| `Ty::Class(name, args)` | `resume Resume` | `Resume` / `lorem.Resume` / `stream_types.lorem.Resume` | same dotted form: `Resume` / `lorem.Resume` / `stream_types.lorem.Resume` | Routing logic identical to Python. Generic args: `Box<T1, T2>` (TS angle-bracket syntax, not Python square-bracket). |
| `Ty::Enum(name)` | `sentiment Sentiment` | `Sentiment` / `ipsum.Sentiment` | `Sentiment` / `ipsum.Sentiment` | Same routing. No generics. |
| `Ty::TypeAlias(name)` | `items StringList` | `StringList` / `util.StringList` | `StringList` / `util.StringList` | Same routing. No generics today. |
| `Ty::TypeVar(name)` | `T` in `class Box<T>` | `T` (bare) | `T` (bare) | Phase 4 declares the `<T>` slot on the enclosing decl. |
| | | | | |
| `Ty::Optional(T)` | `name string?` | `typing.Optional[T]` | `T \| null` | `null` mirrors `proto.ts` decode output. Assumption A1. |
| `Ty::List(T)` | `tags string[]` | `typing.List[T]` | `T[]` | Spec-mandated postfix form (00a-spec Ty table; all example shapes use `string[]`). Union/optional element types are parenthesized: `(string \| null)[]`. |
| `Ty::Map { key, value }` | `metadata map<string,int>` | `typing.Dict[K, V]` | `Record<K, V>` | `Ty::Map` validation guarantees key is `Ty::String` or `Ty::Enum`. Enums stringify under TS. |
| `Ty::Union(types)` | `string \| int` | `typing.Union[str, int]` | `string \| number` | Flat union. Validation forbids nested. |
| | | | | |
| `Ty::Unit` | `-> void` | `None` | `null` | See assumption A5. |
| `Ty::BuiltinUnknown` | `unknown` keyword | `typing.Any` | `unknown` | Strict TS `unknown` (TS `any` is too loose — `unknown` forces narrowing at use site). |
| `Ty::Callable { params, ret }` | `(string, float) -> int` | `typing.Callable[[str, float], int]` | `(arg0: string, arg1: number) => number` | TS requires named params. Auto-generate `arg0`, `arg1`, … when `CallableParam.name` is `None`. |
| `Ty::Callable { params with optional, ret }` | `(string, float?) -> int` | `typing.Callable[..., int]` | `(...args: unknown[]) => number` | Mirrors Python fallback for unrepresentable optional. Assumption A2. |
| `Ty::BamlOptions` | (synthesized for `with_options` args) | `baml.Options` | `baml.Options` | Cross-leaf placeholder; Phase 4 imports from `baml_sdk/baml/index`. |
| `Ty::RustType` | `$rust_type` field | `_BamlPyHandle` | `_BamlHandle` | Leading underscore mirrors Python convention to avoid shadowing by local `baml` module. The runtime class is `BamlHandle` from `bridge_nodejs` (`/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/native.d.ts:16`); Phase 4 emits `import { BamlHandle as _BamlHandle } from '@boundaryml/baml-core-node';` at the top of each leaf that needs it. ⚠ Spec note: the spec names the runtime package `@boundaryml/baml-core-node` (00a Ty table / 00b §"special baml-core types"). Phase 4 must use that exact specifier, not a bare `baml_core/bridge`. |

---

## TS Test Matrix

Full ordered case list for the `translate_ty_covers_phase3_matrix` test. Every label below mirrors the Python label at `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/translate_ty.rs:307-845`. The expected output uses the TS column from the type map above.

`imports` defaults to `&[]` unless noted.

### Primitives & literals (1.x)

| Label | Input | Ctx | `expected_expr` | `expected_imports` |
|---|---|---|---|---|
| `int` | `Ty::Int` | `&["lorem"]` | `number` | `&[]` |
| `bigint` | `Ty::Bigint` | `&["lorem"]` | `bigint` | `&[]` |
| `float` | `Ty::Float` | `&["lorem"]` | `number` | `&[]` |
| `string` | `Ty::String` | `&["lorem"]` | `string` | `&[]` |
| `bool` | `Ty::Bool` | `&["lorem"]` | `boolean` | `&[]` |
| `null` | `Ty::Null` | `&["lorem"]` | `null` | `&[]` |
| `uint8array` | `Ty::Uint8Array` | `&["lorem"]` | `Uint8Array` | `&[]` |
| `builtin unknown` | `Ty::BuiltinUnknown` | `&["lorem"]` | `unknown` | `&[]` |
| `unit` | `Ty::Unit` | `&["lorem"]` | `null` | `&[]` |
| `baml options` | `Ty::BamlOptions` | `&["lorem"]` | `baml.Options` | `&[]` (special-case; Phase 4 handles import — assumption A8) |
| `literal int` | `Ty::Literal(Int(42))` | `&["lorem"]` | `42` | `&[]` |
| `literal negative int` | `Ty::Literal(Int(-1))` | `&["lorem"]` | `-1` | `&[]` |
| `literal bigint` | `Ty::Literal(Bigint(42))` | `&["lorem"]` | `42n` | `&[]` |
| `literal string` | `Ty::Literal(String("draft"))` | `&["lorem"]` | `"draft"` | `&[]` |
| `literal escaped string` | `Ty::Literal(String("has \"quotes\""))` | `&["lorem"]` | `"has \"quotes\""` | `&[]` |
| `literal bool true` | `Ty::Literal(Bool(true))` | `&["lorem"]` | `true` | `&[]` |
| `literal bool false` | `Ty::Literal(Bool(false))` | `&["lorem"]` | `false` | `&[]` |
| `literal float fallback` | `Ty::Literal(Float("3.14"))` | `&["lorem"]` | `number` | `&[]` |

### Media (2.x)

| Label | Input | Ctx | `expected_expr` | `expected_imports` |
|---|---|---|---|---|
| `media image` | `Ty::Media(Image)` | `&["lorem"]` | `baml.media.Image` | `&[&["baml", "media"]]` |
| `media audio` | `Ty::Media(Audio)` | `&["lorem"]` | `baml.media.Audio` | `&[&["baml", "media"]]` |
| `media video` | `Ty::Media(Video)` | `&["lorem"]` | `baml.media.Video` | `&[&["baml", "media"]]` |
| `media pdf` | `Ty::Media(Pdf)` | `&["lorem"]` | `baml.media.Pdf` | `&[&["baml", "media"]]` |
| `media generic fallback` | `Ty::Media(Generic)` | `&["lorem"]` | `unknown` | `&[]` |

### Class / enum / type alias name refs (3.x)

| Label | Input | Ctx | `expected_expr` | `expected_imports` |
|---|---|---|---|---|
| `class same leaf root namespace` | `Class(name("user", &["lorem"], "Resume"), [])` | `&["lorem"]` | `Resume` | `&[]` |
| `class cross leaf root namespace` | `Class(name("user", &["lorem"], "Resume"), [])` | `&["ipsum"]` | `lorem.Resume` | `&[&["lorem"]]` |
| `class same leaf root init` | `Class(name("user", &[], "Foo"), [])` | `&[]` | `Foo` | `&[]` |
| `class root init from namespaced leaf` | `Class(name("user", &[], "Foo"), [])` | `&["lorem"]` | `Foo` | `&[]` |
| `class vendor cross leaf` | `Class(name("aws", &["s3"], "Bucket"), [])` | `&["lorem"]` | `vendor.aws.s3.Bucket` | `&[&["vendor", "aws", "s3"]]` |
| `class vendor same leaf` | `Class(name("aws", &["s3"], "Bucket"), [])` | `&["vendor", "aws", "s3"]` | `Bucket` | `&[]` |
| `class vendor other vendor leaf` | `Class(name("aws", &["s3"], "Bucket"), [])` | `&["vendor", "aws", "ec2"]` | `vendor.aws.s3.Bucket` | `&[&["vendor", "aws", "s3"]]` |
| `class stdlib cross leaf` | `Class(name("baml", &["http"], "Response"), [])` | `&["lorem"]` | `baml.http.Response` | `&[&["baml", "http"]]` |
| `class stdlib same leaf` | `Class(name("baml", &["http"], "Response"), [])` | `&["baml", "http"]` | `Response` | `&[]` |
| `class stream from non stream leaf` | `Class(name("user", &["lorem"], "Resume$stream"), [])` | `&["lorem"]` | `stream_types.lorem.Resume` | `&[&["stream_types", "lorem"]]` |
| `class stream same leaf` | `Class(name("user", &["lorem"], "Resume$stream"), [])` | `&["stream_types", "lorem"]` | `Resume` | `&[]` |
| `class non stream from stream leaf` | `Class(name("user", &["lorem"], "Resume"), [])` | `&["stream_types", "lorem"]` | `lorem.Resume` | `&[&["lorem"]]` |
| `enum same leaf` | `Enum(name("user", &["ipsum"], "Sentiment"))` | `&["ipsum"]` | `Sentiment` | `&[]` |
| `enum cross leaf` | `Enum(name("user", &["ipsum"], "Sentiment"))` | `&["lorem"]` | `ipsum.Sentiment` | `&[&["ipsum"]]` |
| `type alias same leaf` | `TypeAlias(name("user", &["util"], "StringList"))` | `&["util"]` | `StringList` | `&[]` |
| `type alias cross leaf` | `TypeAlias(name("user", &["util"], "StringList"))` | `&["lorem"]` | `util.StringList` | `&[&["util"]]` |

### Containers (4.x)

| Label | Input | Ctx | `expected_expr` | `expected_imports` |
|---|---|---|---|---|
| `optional string` | `Optional(String)` | `&["lorem"]` | `string \| null` | `&[]` |
| `list int` | `List(Int)` | `&["lorem"]` | `number[]` | `&[]` |
| `map string int` | `Map(String, Int)` | `&["lorem"]` | `Record<string, number>` | `&[]` |
| `map enum to class` | `Map(Enum(ipsum.Sentiment), Class(lorem.Resume, []))` | `&["lorem"]` | `Record<ipsum.Sentiment, Resume>` | `&[&["ipsum"]]` |
| `union int string` | `Union([Int, String])` | `&["lorem"]` | `number \| string` | `&[]` |
| `union int string bool` | `Union([Int, String, Bool])` | `&["lorem"]` | `number \| string \| boolean` | `&[]` |
| `optional list same leaf class` | `Optional(List(Class(lorem.Resume, [])))` | `&["lorem"]` | `Resume[] \| null` | `&[]` |
| `list optional string` | `List(Optional(String))` | `&["lorem"]` | `(string \| null)[]` | `&[]` |
| `map vendor list` | `Map(String, List(Class(aws.s3.Bucket, [])))` | `&["lorem"]` | `Record<string, vendor.aws.s3.Bucket[]>` | `&[&["vendor", "aws", "s3"]]` |
| `optional media` | `Optional(Media(Image))` | `&["lorem"]` | `baml.media.Image \| null` | `&[&["baml", "media"]]` |
| `optional stdlib class` | `Optional(Class(baml.http.Response, []))` | `&["lorem"]` | `baml.http.Response \| null` | `&[&["baml", "http"]]` |
| `list vendor class` | `List(Class(aws.s3.Bucket, []))` | `&["lorem"]` | `vendor.aws.s3.Bucket[]` | `&[&["vendor", "aws", "s3"]]` |
| `map enum to stream vendor class` | `Map(Enum(ipsum.Sentiment), Class(aws.s3.Bucket$stream, []))` | `&["lorem"]` | `Record<ipsum.Sentiment, stream_types.vendor.aws.s3.Bucket>` | `&[&["ipsum"], &["stream_types", "vendor", "aws", "s3"]]` |
| `union across placements` | `Union([Class(lorem.Resume), Class(aws.s3.Bucket), Class(baml.http.Response)])` | `&["lorem"]` | `Resume \| vendor.aws.s3.Bucket \| baml.http.Response` | `&[&["vendor", "aws", "s3"], &["baml", "http"]]` |
| `union stream and non stream classes` | `Union([Class(lorem.Resume, []), Class(lorem.Resume$stream, [])])` | `&["lorem"]` | `Resume \| stream_types.lorem.Resume` | `&[&["stream_types", "lorem"]]` |

### Callable (5.x)

| Label | Input | Ctx | `expected_expr` | `expected_imports` |
|---|---|---|---|---|
| `callable two params` | `Callable([Int, String], Bool)` | `&["lorem"]` | `(arg0: number, arg1: string) => boolean` | `&[]` |
| `callable no params` | `Callable([], Unit)` | `&["lorem"]` | `() => null` | `&[]` |
| `callable nested params` | `Callable([List(Int)], Optional(String))` | `&["lorem"]` | `(arg0: number[]) => string \| null` | `&[]` |
| `callable optional params` | `Callable([String, optional_callable_param("limit", Int)], Bool)` | `&["lorem"]` | `(...args: unknown[]) => boolean` | `&[]` |

### Generics (6.x)

| Label | Input | Ctx | `expected_expr` | `expected_imports` |
|---|---|---|---|---|
| `generic class same leaf concrete int` | `Class(lorem.Box, [Int])` | `&["lorem"]` | `Box<number>` | `&[]` |
| `generic class cross leaf concrete int` | `Class(lorem.Box, [Int])` | `&["ipsum"]` | `lorem.Box<number>` | `&[&["lorem"]]` |
| `generic class with list arg` | `Class(lorem.Box, [List(Int)])` | `&["lorem"]` | `Box<number[]>` | `&[]` |
| `generic class nested generic arg` | `Class(lorem.Box, [Class(lorem.Box, [Int])])` | `&["lorem"]` | `Box<Box<number>>` | `&[]` |
| `generic class stream from non-stream leaf` | `Class(lorem.Box$stream, [Int])` | `&["lorem"]` | `stream_types.lorem.Box<number>` | `&[&["stream_types", "lorem"]]` |
| `generic class with typevar arg` | `Class(lorem.Box, [TypeVar("T")])` | `&["lorem"]` | `Box<T>` | `&[]` |
| `bare typevar` | `TypeVar("T")` | `&["lorem"]` | `T` | `&[]` |
| `map with typevar key and value` | `Map(String, TypeVar("V"))` | `&["lorem"]` | `Record<string, V>` | `&[]` |

### Recursive aliases / self-refs (7.x — TS doesn't need quoting)

These cases collapse vs. Python: drop the self-ref / recursive-body machinery and produce unquoted output. Tests still ported to confirm TS handles them without special handling.

| Label | Input | Ctx | `expected_expr` | `expected_imports` |
|---|---|---|---|---|
| `recursive alias self ref` | `TypeAlias(util.RecList)` | `&["util"]` | `RecList` | `&[]` |
| `self-ref class no args` | `Class(lorem.Node, [])` | `&["lorem"]` | `Node` | `&[]` |
| `self-ref generic class wraps args inside quotes` (renamed: `self-ref generic class — no quoting in TS`) | `Class(lorem.Node, [String])` | `&["lorem"]` | `Node<string>` | `&[]` |
| `self-ref generic class nested in list — no quoting in TS` | `List(Class(lorem.Node, [Int]))` | `&["lorem"]` | `Node<number>[]` | `&[]` |
| `recursive alias inside list — no quoting in TS` | `List(TypeAlias(util.RecList))` | `&["util"]` | `RecList[]` | `&[]` |
| `recursive alias inside union — no quoting in TS` | `Union([Int, List(TypeAlias(util.RecList))])` | `&["util"]` | `number \| RecList[]` | `&[]` |
| `recursive alias leaves other refs unquoted under self_ref-only` | `List(Class(util.Other, []))` | `&["util"]` | `Other[]` | `&[]` |
| `recursive body same-leaf sibling — no quoting in TS` | `List(Class(util.Other, []))` | `&["util"]` | `Other[]` | `&[]` |
| `recursive body cross-leaf class — no forward-ref in TS` | `List(Class(util.Bar, []))` | `&["lorem"]` | `util.Bar[]` | `&[&["util"]]` |
| `recursive body root-routed name — no forward-ref in TS` | `Class(name("user", &[], "Foo"), [])` | `&["lorem"]` | `Foo` | `&[]` |
| `recursive body cross-leaf enum — no forward-ref in TS` | `Enum(name("user", &["ipsum"], "Sentiment"))` | `&["lorem"]` | `ipsum.Sentiment` | `&[&["ipsum"]]` |
| `non recursive alias same leaf` | `TypeAlias(util.RecList)` | `&["util"]` | `RecList` | `&[]` |
| `non recursive alias cross leaf` | `TypeAlias(util.RecList)` | `&["lorem"]` | `util.RecList` | `&[&["util"]]` |

---

## Assumptions

These are the design forks Phase 3 resolves; documented for future revisits.

- **A1**: `Ty::Optional(T)` → `T | null` (not `T | undefined`). Rationale: matches what `bridge_nodejs/proto.ts::decodeValue` returns for null (`null`, not `undefined`). Caller-side encoding also accepts both, so this is a one-way choice on the type side. Source: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts:79,124`.
- **A2**: `Ty::Callable { params with at-least-one-optional, ret }` collapses to `(...args: unknown[]) => Ret`. Rationale: TS function types cannot express per-param optionality without naming each param explicitly, and even with names the optional-trailing semantics differ from Python's `Callable[..., R]`. Matches the documented fallback in the 00a-spec "Exhaustive Ty conversions" table (`Ty::Callable` row: "`(...args) => ret` only if function values become serializable").
- **A3**: Root-leaf names (`route_class_ref(name).segments.is_empty()`) emit bare (no import) from any leaf. Rationale: Phase 4 will re-export every root symbol from a single root barrel (`baml_sdk/index.ts`) and every leaf imports the root via `import * as _root from '../...';` — but for type-level use, the names should resolve via TS's module-level imports that Phase 4 emits as `import { Foo } from '../..';` at the top of each leaf. Phase 3 reports no import dependency in this case; Phase 4 detects "root-routed reference seen in non-root leaf" by checking whether the resulting expr is a bare name with no `.` and the `Ty` is a `Class`/`Enum`/`TypeAlias` whose routed path is root, and emits the correct import. (Alternative: report `LeafPath::root()` in `imports` — rejected for simplicity; the cost is Phase 4 has to re-route the names. Picked the simpler-Phase-3 option.)
- **A4**: TypeScript native recursive types are sufficient for every BAML recursive alias and self-ref shape. Drop `defer_name_refs` and `SelfRef` from `TranslateCtx`. If a counterexample is found (e.g. a recursive alias TS rejects), Phase 3 will add quoting only for that case and document the trigger; until then, the test matrix asserts unquoted output.
- **A5**: `Ty::Unit` → `null` (not `void`). Rationale: TS `void` is a return-position-only type in function signatures; using it inside a class field or generic arg slot is unidiomatic. Python uses `None` (a value type) — `null` is its TS analog. Phase 4 may special-case return-position `Ty::Unit` to emit `void` instead of `null` (a return-position type-narrowing the emitter does, not `translate_ty`).
- **A6** (corrected to the re-edited spec): Phase 4 emits a SINGLE root-namespace import per leaf (`import type * as <rootns> from "..";`, relative path to the package root) and references every cross-leaf symbol through it as `<rootns>.<dotted-path>` (e.g. `symbol_collisions.fizz.foo.Bar`). Phase 4 does NOT emit per-leaf flattened imports (`import * as symbol_collisions_fizz_foo from "../fizz/foo"`), which the 00a-spec appendix ("Cross-Namespace References") flags as wrong. The dotted-path placeholder `lorem.Resume` that Phase 3 emits is the root-relative tail; Phase 4 prefixes the root alias. Source: `/Users/sam/thoughts/sam-projects/bridge-node/00a-spec-codegen-mappings.md` appendix (correct/wrong import blocks).
- **A7**: For multi-segment cross-leaf paths (e.g. `vendor.aws.s3.Bucket`), the `imports` set carries the full root-relative `LeafPath`. Under the single-root-import rule no per-segment import is emitted: the dotted path is already fully qualified from the root namespace, so Phase 4 reuses it verbatim as `<rootns>.vendor.aws.s3.Bucket`. Container barrels (`vendor/index.ts`, `vendor/aws/index.ts`) re-export child namespaces so the path resolves. Phase 3 records the full path; Phase 4 owns the root alias + barrels.
- **A8**: `Ty::BamlOptions` → `baml.Options` with empty imports (special-case). Rationale: `BamlOptions` is not a routable `Name` (no `Ty::Class(name, _)` wrapping it), and its import is hard-coded at the top of every leaf that references it — Phase 4 detects the literal string `baml.Options` in any emitted expr and adds the appropriate import (the `Options`/`BamlCallOptions` symbol from the runtime package `@boundaryml/baml-core-node`, or a generated `baml` leaf re-export of it). Phase 3 emits the literal token; Phase 4 owns the import. Same treatment for `Ty::RustType` → `_BamlHandle` (also `@boundaryml/baml-core-node`). ⚠ Spec note: the spec calls the generated function-options type `BamlCallOptions` (00a-spec §Rules / example-shapes), while the `Ty` table column is `baml.Options`; reconcile the exact emitted token in Phase 4.
- **A9**: `Uint8Array` is the chosen TS analog for `Ty::Uint8Array`, not `Buffer`. Browser-portable; matches `proto.ts`'s `uint8arrayValue` output type (`/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts:84`).
- **A10**: `Ty::BuiltinUnknown` → `unknown` (not `any`). Rationale: TS `unknown` is the formally correct top type (forces narrowing at use site); `any` defeats the type-checker. Python uses `Any` because that's all `typing` exposes — TS has a strictly better option.
- **A11**: Cross-leaf path string format is `segments.join(".")` (e.g. `vendor.aws.s3.Bucket`) — not `segments.join("/")` or `import("./...")`. This mirrors Python's output verbatim and lets the test matrix port mechanically.

---

## File Inventory

### Files created by Phase 3

- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/translate_ty.rs` — new, ~700 lines including test matrix

### Files modified by Phase 3

- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs` — add `mod translate_ty;` declaration; add `pub(crate) fn ts_string(s: &str) -> String` helper (port of `py_string`)
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/Cargo.toml` — add `baml_base = { workspace = true }` to `[dependencies]`

### Files NOT touched by Phase 3

- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/routing.rs` — owned by Phase 2
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/emit/*.rs` — owned by Phase 2 (scaffolding) / Phase 4 (filled in)
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/leaf.rs` — owned by Phase 4
- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/**` — owned by Phase 1
- `/Users/sam/baml3/baml_language/sdks/python/**` — Python SDK; reference only

---

## References

### Primary sources

- `/Users/sam/thoughts/sam-projects/bridge-node/00b-overview.md` — six-phase plan (Phase 3 verbatim scope at lines 40-44: "implement translate_ty … use the exhaustive TIR `Ty` conversion table in `00a`")
- `/Users/sam/thoughts/sam-projects/bridge-node/00a-prior-art-python-type-mappings.md` — Python prior-art cross-reference (Python "Exhaustive Ty conversions" table)
- `/Users/sam/thoughts/sam-projects/bridge-node/00a-spec-codegen-mappings.md` — codegen rules, especially the "Exhaustive Ty conversions" table (the authoritative BAML→TS type map; re-edited 2026-05-29) and the appendix "Cross-Namespace References" correct/wrong import blocks

### Python prior art

- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/translate_ty.rs` — full reference implementation (851 lines, ~700 of which is the test matrix to port)
- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/routing.rs` — routing rules (Phase 2 ports this)
- `/Users/sam/baml3/baml_language/sdks/python/rust/codegen_python/src/lib.rs:400-418` — `py_string` helper to mirror as `ts_string`

### Type definitions

- `/Users/sam/baml3/baml_language/crates/baml_codegen_types/src/ty.rs` — `Ty` enum (lines 79-144, includes `Bigint` at line 83 and `Literal` carrying `baml_base::Literal` at line 88), `Name` struct (lines 18-50), `CallableParam` (lines 62-73), `CodegenFunctionParamMode` (lines 69-73)
- `/Users/sam/baml3/baml_language/crates/baml_base/src/core_types.rs` — `MediaKind` enum (lines 261-267, includes `Generic`) and `Literal` enum (lines 295-307, includes `Bigint(num_bigint::BigInt)`)

### Node.js bridge runtime context

- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/native.d.ts:16` — `BamlHandle` class declaration
- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts:17` — re-export of `BamlHandle`
- `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts` — encode/decode entry points; documents that decoded null is `null` (not `undefined`) — supports assumption A1

### Codegen scaffolding (Phase 2 deliverables, assumed)

- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/lib.rs` — stub at Phase 2 start; Phase 2 adds `mod routing;`
- `/Users/sam/baml3/baml_language/sdks/nodejs/sdkgen_nodejs/src/routing.rs` — Phase 2 creates this as a port of the Python routing module

### Plan template

- `/Users/sam/thoughts/sam-commands/2-create-plan.md` — plan structure template used for this document
