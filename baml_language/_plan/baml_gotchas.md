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
- **A class field named `type` is fully supported** (decl, construction, `.type` access — user
  code always worked; the *stdlib* builtins codegen used to crash generating Rust for it). FIXED:
  both generators now escape Rust-keyword field names as raw identifiers (`r#type`); the
  non-rawable set (`self`/`Self`/`super`/`crate`/`_`) gets a trailing underscore. Wire classes can
  use `type: string` directly — no `@alias` workaround needed.
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
- **E0097 (extraneous throws) fires on `implements`-block methods that declare the interface's
  full error channel without throwing into it.** Throws tracking is exact, with ONE widening: a
  declared *interface* fact is justified iff some thrown class implements it. Repeating the
  interface's channel "for contract's sake" warns; declare only what the body throws. Narrowing
  below the interface channel is legal (covariant), down to **`throws never` for throw-nothing
  bodies** — legal since the `Never`-is-bottom fix in `check.rs::ty_nominal_subtype` (it used to
  be rejected as a signature mismatch). NOTE: these warnings surfaced on 45 stdlib declarations
  only when the std-diagnostics snapshot was regenerated — snapshot suites don't run on every
  change; regen `compiles/__baml_std__` after stdlib edits.

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
- **A bare object literal as a `match`-arm value parses as a block, not a record** —
  `null => { "type": "base64", … }` errors (E0010 `Expected expression, found ':'`) because `{`
  at statement position opens a block. Put object literals in unambiguous expression position:
  `return { … }` from the arm, or bind `let x: json = { … };` first.
- **`spawn { … }`'s error type infers as `null`, not `never`** — annotate future arrays as
  `baml.future.Future<T, null>[]`.
- **`?? []` needs a typed binding** (`let xs: T[] = maybe ?? [];`) — bare coalesce against `[]`
  infers a bogus union.
- **User-package classes CAN implement a stdlib interface that `requires` another** — the E0125
  false positive (the `requires`-satisfaction probe resolved the parent against the *user's*
  package instead of the interface's own) is FIXED in `baml_lsp2_actions/src/check.rs`
  (regression test: `interfaces.rs::cross_package_requires_satisfied_by_sibling_implements_is_ok`).
  User-authored providers no longer need to live in the stdlib.
- **Stdlib edits need `touch` + rebuild** — cargo doesn't always notice `.baml` mtime changes for
  the embedded std; `touch` the file before `cargo build -p baml_cli`.
- **The formatter can't process functions with a `client`-named param** (pre-existing, e.g. parts
  of `ns_llm/llm_types.baml`) — it errors rather than reformatting; harmless.

## Tooling (added post-strict-mode round)

- **`cargo fmt -p baml_tests` reformats the entire package** — including sibling test files
  you didn't touch; a `git checkout` to undo it can nuke uncommitted work in those files.
  Format single files with `rustfmt <file>`.
- **Host ops CAN produce typed BAML throws**: declare `throws root.errors.Unsupported` on the
  `$rust_io_function` and return `SysOpOutput::err(VmBamlError::Unsupported{..})` — the codegen's
  error-category mapping round-trips it as a catchable typed error (no UnknownError box).
- **A `match` on a CONCRETE provider type makes interface arms irrefutable** — `match (p /* OpenAi */)
  { let h: HttpProvider => …, _ => … }` flags the `_` unreachable (E0063). Type the binding as the
  existential (`let p: baml.ai.Provider = …`) when you want runtime negotiation.

## Streaming

- **`Stream.next()` partials are best-effort, not guaranteed** — the pull loop batches SSE
  events per network read (`_sse.next()` returns everything buffered), and when a batch
  completes the stream (`is_done()`), `next()` returns `StreamFinished` WITHOUT yielding that
  batch's content — it only surfaces via `final()`. A short/fast response arriving in one read
  therefore yields **zero** partials (reproduced on mock AND live, OpenAI and Anthropic alike).
  Don't assert partial counts in tests; UIs must treat `next()` as opportunistic and always
  read `final()`.

## Realtime / WebSocket round

- **The `OpenAI-Beta: realtime=v1` header now selects the RETIRED beta shape** and gets
  rejected (`beta_api_shape_disabled`). GA realtime: `wss://api.openai.com/v1/realtime?model=…`
  with `Authorization` only; `response.create` takes `output_modalities` (not `modalities`);
  text arrives as `response.output_text.delta`; terminal event `response.done`.
- **`baml.ws` ops are gated under sys_native's `bundle-http` feature** (reuse the rustls
  provider); non-bundle / wasm builds fall back to Unsupported.
