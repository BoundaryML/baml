# BAML gotchas — field notes from building `baml.ai`

Hard-won, reproduced-in-anger notes from implementing the provider model in BAML.
Each entry: the symptom, the rule, the workaround. Compiler-bug candidates are marked ⚠.
(Longer-form context lives in [`deviations.md`](./deviations.md).)

## Match / runtime type-tests

- ⚠ **`match (v) { let t: T => … }` on a *generic* `T` is an irrefutable catch-all.** It binds
  *everything* (no runtime test), making later arms unreachable (E0063). Corollary: `v is T` on a
  generic parameter always returns `false`. There is no way to runtime-test "is this value a
  (monomorphized) `T`" today.
- ⚠ **Matching `unknown` (or `unknown?`, e.g. `map<…, unknown>.get(k)`) against a concrete class
  binding is also an irrefutable catch-all** — same shape as above, `_` arm unreachable.
- ⚠ **Matching `unknown` against a type-alias union (`json`) does NOT bind** — the arm falls
  through to `_` even when the value is json-shaped. (Opposite failure of the previous entry.)
- ⚠ **A `_` arm after a media-typed binding breaks the media runtime type-test.**
  `match (p.image /* image? */) { let i: image => …, _ => … }` takes `_` even for a genuine image.
  **Workaround (the house style): null-eliminate instead** — `match (x) { null => {}, let v: T => … }`
  puts the binding last where it's irrefutable, so no runtime test runs. Method calls on the bound
  value work fine either way.
- **Interface-membership matching works** (`match (p) { let h: HttpProvider => …, _ => … }`) —
  the capability-negotiation backbone. Class narrowing within proper unions also works.

## Reserved / magic names

- **`client` is a keyword** — can't name a function `client`.
- **`function` is a keyword** — a class field can't be named `function`; navigate JSON keys named
  `"function"` with `baml.json.field(j, "function")`.
- ⚠ **A class field named `type` crashes the builtins codegen** (`expected identifier, found
  keyword \`type\``) — at Rust-generation time, not BAML-check time. Workaround for `type`-tagged
  wire envelopes: name the field `kind` with `@alias("type")` and decode via SAP (see below).
- ⚠ **A parameter/local named `env` is shadowed by the env-var magic** — `env.output` resolves as
  an environment-variable access (E0004 suggesting `baml.env.get_or_panic("output")`), not your
  binding. Pick another name.
- **A function with a param named `prompt` whose body doesn't start with `let` is mis-parsed as an
  LLM declarative body** (E0010 "Expected LLM function missing 'client' field"). Lead the body
  with a `let`.

## JSON / serialization

- **`baml.json.from_json<T>` does NOT honor `@alias`; `baml.sap.parse<T>` DOES.** For wire classes
  with renamed fields (`kind string @alias("type")`), decode with SAP.
- **`baml.json.to_string<T>` is type-driven and fails on `T = unknown`; `baml.json.to_json<T>`
  dispatches on the value's *runtime* type and works.** Use `to_json<unknown>(v)` to convert an
  unknown to `json`.
- **Quoted keys in nested client-option blocks are silently dropped** —
  `headers { "x-custom" "v" }` compiles but never reaches `options.headers`; use the bare-key form
  `headers { x-custom "v" }`.
- **`baml.json.path<T>(j, ".a[0].b")`** (jq-style, throws `JsonPathError`) is the ergonomic
  accessor for one-off reads; prefer typed wire classes + SAP for whole envelopes or
  `type`-tagged arrays that need filtering.

## Errors / throws

- **The throws-checker is strict and infers.** Every throwing call must be caught or declared,
  including foreign errors (`Io`/`Timeout`/`Json*`) — normalize with a trailing
  `catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["…"] } }`.
- ⚠ **Inferred throw sets narrow `catch` reachability**: if a callee only ever throws
  `UnknownError`, a `catch` arm for the *declared* interface channel (`let c: CallError =>`) is
  flagged **unreachable** — strictly under `baml-cli`, only a warning under the `baml_test!`
  harness. Always strict-check stdlib changes via `baml-cli run --file <trivial>.baml`.
- **`catch` is a postfix on an expression** (not a trailing function block), and arms must produce
  the expression's type — use `return` inside arms to exit with a different value.

## Host boundary (`$rust_io_function`)

- **Cross-namespace types in signatures don't resolve in codegen.** A decl in `ns_llm` returning
  `root.ai.ChatMessage[]` generated `Vec<BexExternalValue>` (and didn't even compile); declaring
  the fn in the classes' own namespace (`ns_ai`) generates proper owned structs. Cross-namespace
  *params* arrive untyped (`BexExternalValue`) — unwrap manually.
- **Media at the host boundary is the `baml.media.*` instance form** (`Instance { class_name:
  "baml.media.Image", fields: { "_data": RustData(Arc<MediaValue>) } }`), mirroring bex_vm's
  `copy::media` constructors — not the bare media ADT.
- **Unions don't cross host construction** — a host-built class with a union-typed field won't
  round-trip; model host-facing shapes as product types with optional fields
  (`MessagePart { text?, image?, … }`).
- **`Response.text()` is not idempotent** — reading the body consumes it; a second read returns
  empty. Read once (e.g. `type Body = string`) and share the string.

## Misc

- **Closures are `(x: T) -> R { body }`**, not `x => body`. A callback param's `throws` must be
  named and threaded explicitly (the `Iterator.map<R, E2>` pattern).
- **Generic type aliases (`type Foo<E> = …`) don't exist** — spell unions inline.
- **`spawn { … }`'s error type infers as `null`, not `never`** — annotate future arrays as
  `baml.future.Future<T, null>[]`.
- **`?? []` needs a typed binding** (`let xs: T[] = maybe ?? [];`) — bare coalesce against `[]`
  infers a bogus union.
- **User-package classes cannot implement a stdlib interface that `requires` another** (E0125 —
  the in-body `implements Provider {}` isn't seen by the `requires` check across packages).
  Until fixed, provider classes must live in the stdlib. **This is the top blocker for
  user-authored providers.**
- **Stdlib edits need `touch` + rebuild** — cargo doesn't always notice `.baml` mtime changes for
  the embedded std; `touch` the file before `cargo build -p baml_cli`.
- **The formatter can't process functions with a `client`-named param** (pre-existing, e.g. parts
  of `ns_llm/llm_types.baml`) — it errors rather than reformatting; harmless.
