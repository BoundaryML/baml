---
date: 2026-07-08
repository: baml4
---
Here are the rules we're going to use for Node.js TypeScript codegen, and more generally for host-language codegen that calls BAML functions from Node.

Principle: in each host language, use the most language-idiomatic generated API for that language.
	- For Node.js TypeScript, that means Promise-returning async calls use explicit `_async` export names, blocking sync calls use the preserved BAML function name (both as module-level exports reached through the package/namespace path — there is no aggregated client object), BAML data shapes are class-backed structural TypeScript types, and companion functions preserve their BAML `$` names verbatim (`extract_resume$stream` for stream companions, `extract_resume$build_request` / `extract_resume$parse` / etc. for modular companions) — because `$` is a valid TypeScript identifier character, codegen does not translate to Python's `_stream` / `__build_request` form.
	- Do not make the Node.js API mirror Python naming or package structure just because another host language works differently.

Requirements:
- async vs sync callers: rules for `await b.extract_resume_async()` vs `b.extract_resume()`
- namespaces: user's package `user.Resume` vs other package `aws.s3.Bucket` vs std library `baml.http.Request`
- companion types: `Resume` vs `Resume$stream`
- function flavors (all reached through the package/namespace path, e.g. `b.lorem.*`; no aggregated client object)
	- free functions `b.lorem.extract_resume()` vs
	- companion functions `b.lorem.extract_resume$build_request()` / `b.lorem.extract_resume$parse()` vs
	- static functions `Image.from_url()` vs instance functions `file.close()`

Decisions:
- Generated Node TypeScript projects use `baml_sdk` as the public generated package name.
- All decoded BAML classes become real TypeScript class instances, so generated instance methods can be called on decoded values.

# Rules
This section lays out the proposed Node.js TypeScript codegen rules; see "Rationales" below for more detail and rejected alternatives.

- In BAML, only LLM-backed functions have companion functions like `$build_request()`, `$render_prompt()`, `$parse()`, and `$stream()`.
- Top-level generated package/module is `baml_sdk`.
- Runtime-owned stdlib values are imported from `@boundaryml/baml-bridge`; generated user/vendor types live in the generated package.
- Function binding codegen generates a sync export and an async export per BAML function, as plain module-level exports in the function's namespace module. There is no aggregated `b` client object: the package module itself is the client.
	- package alias: `import * as b from "baml_sdk"` (or any name; `b` is just the conventional alias for the whole package).
	- BAML function `extract_resume()` in namespace `lorem` becomes module exports `extract_resume()` and `extract_resume_async()` in `lorem/index.ts`, reached as `b.lorem.extract_resume()` / `b.lorem.extract_resume_async()`. A root-namespace function is reached directly: `b.make_foo()`.
	- Async calls return `Promise<T>`: `await b.lorem.extract_resume_async(args)`.
	- Sync calls return `T`: `b.lorem.extract_resume(args)`.
	- Namespaced functions are never hoisted to the package root; they are reached only through their namespace path.
	- Codegen appends `_async` verbatim to the preserved BAML source function name.
	- Function arguments are emitted as positional parameters in BAML declaration order, followed by optional `bamlOptions?: BamlCallOptions`.
	- Codegen never provides default values for function args. Optionality comes only from the BAML type.
- TypeScript name preservation:
	- Codegen does no case translation from BAML names to TypeScript names.
	- Type, enum, class, namespace segment, value-class, function, and method names preserve the BAML source spelling when it is already a valid TypeScript identifier.
	- invalid TypeScript identifiers are escaped or renamed deterministically, and the generated serializer map keeps the original BAML name.
	- enum variants preserve the BAML variant name as the TypeScript member when valid; otherwise codegen escapes or renames deterministically and records the wire name in the serializer map.
	- baml compiler: future work planned to warn on source names that collide after TS escaping/renaming or after appending `_async`.
- Package/namespace codegen rule:
	- user namespaces and symbols go directly under the generated package root.
	- user symbols with no namespace (e.g. BAML `user.Resume`) become root module exports: `baml_sdk.Resume` and `baml_sdk.extract_resume` (reached as `b.Resume`, `b.extract_resume()`).
	- user namespace `user.lorem.Resume` becomes exported namespace/type path `baml_sdk.lorem.Resume`.
	- third-party symbols go into `baml_sdk.vendor`, e.g. `baml_sdk.vendor.aws.s3.Bucket`.
	- standard library symbols go into `baml_sdk.baml`, e.g. `baml_sdk.baml.http.Request`.
	- baml compiler: future work planned to prevent users from defining `vendor` and `baml` namespaces. (`stream_types` is no longer reserved — stream companion types live beside their base type, so there is no dedicated namespace to protect.)
- File layout rule:
	- TypeScript codegen emits one directory per BAML namespace, each with a single `index.ts` file. Unlike Python (which splits `__init__.py` / `__init__.pyi`), Node emits no sibling `index.d.ts` declaration file: the generated `index.ts` is already fully typed (real `export class`, typed `as` casts on every binding), so a separate declaration file would be redundant and risk drift. Codegen does not pack multiple BAML namespaces into one generated file.
	- The package root file (`index.ts`) exports root-namespace user symbols and re-exports child namespaces. It does not define an aggregated `b` client object — the package module itself is the client, aliased by users as `import * as b from "baml_sdk"`.
	- Parent modules must not flatten/re-export symbols from child namespaces. For example, `baml_sdk.symbol_collisions` must expose `lorem` as a child namespace, but must not export `Ipsum` directly from `symbol_collisions/lorem`.
	- Container barrel files may exist to expose nested namespace paths, e.g. `vendor/index.ts` and `vendor/aws/index.ts`; they only export child namespaces and do not contain generated BAML symbols.
	- Use namespace-preserving exports like `export * as lorem from "./lorem"` or equivalent imported namespace bindings. Do not use flattening exports like `export * from "./lorem"` in parent/container modules.
	- User namespace `user.lorem` emits `lorem/index.ts`, exposed publicly as `baml_sdk.lorem`.
	- Third-party namespace `aws.s3` emits `vendor/aws/s3/index.ts`, exposed publicly as `baml_sdk.vendor.aws.s3`.
	- Standard-library namespace `baml.http` emits `baml/http/index.ts`, exposed publicly as `baml_sdk.baml.http`.
	- Stream companion types emit in the same namespace module as their base type, e.g. BAML `user.lorem.Resume$stream` emits in `lorem/index.ts` as `export class Resume$stream`, beside `Resume`. There is no separate `stream_types/` directory.
- Generated data types:
	- BAML classes emit `export class`; all decoded class values are instances of those generated classes and can host static/instance functions.
	- Generated classes expose declared BAML fields as public properties and accept a field object in the constructor.
	- BAML enums emit `export enum` with string-valued members, preserving BAML variant names when they are valid TypeScript enum member names.
	- BAML type aliases emit `export type`.
	- Stdlib handle-backed media and IO types (`Image`, `Audio`, `Video`, `Pdf`, `File`, etc.) are runtime value classes imported/re-exported from `@boundaryml/baml-bridge`, because callers need constructors/static helpers and handle identity. The runtime exports them under their `Baml`-prefixed names (`BamlImage`, `BamlStream`, …) and does not alias; codegen aliases them on re-export (`export { BamlImage as Image } from "@boundaryml/baml-bridge"`), so the public surface is `baml_sdk.baml.media.Image`. `defineFunction` and `defineInstanceFunction` are likewise separate named exports of `@boundaryml/baml-bridge`.
	- Static functions are emitted on the value class or a merged namespace: `Image.from_url(url)`.
	- Instance functions are emitted as methods on the generated class or runtime wrapper: `file.close()`.
- Companion type rules (stream types):
	- stream companion types live beside their base type in the same namespace, with the `$stream` suffix preserved, e.g. `baml_sdk.lorem.Resume$stream`.
	- there is no reserved `stream_types` namespace; the `$stream`-suffixed name itself distinguishes the companion from its base type.
	- enums do not get `$stream` companions.
	- resolution of stream vs non-stream is purely by fully-qualified name: BAML `user.lorem.Resume` -> TS `baml_sdk.lorem.Resume`; BAML `user.lorem.Resume$stream` -> TS `baml_sdk.lorem.Resume$stream`.
	- There is no per-field or context-sensitive retargeting: a field typed `Resume` inside `Resume$stream`'s TIR still resolves to `baml_sdk.lorem.Resume`.
	- Codegen does not re-derive a `Partial<T>`, optional-field, or `StreamState<T>` transform for stream companion types. It consumes the compiler-produced stream companion class shape as an ordinary codegen type.
- Companion function rules (stream API and modular API):
	- Stream API naming: BAML `extract_resume$stream()` maps to `b.extract_resume$stream()` and `b.extract_resume$stream_async()` (the BAML `$stream` name is preserved verbatim; `_async` is appended for the async binding).
	- Async streaming returns `Promise<BamlStream<lorem.Resume$stream, lorem.Resume>>` or the runtime's equivalent typed stream wrapper. If the runtime constructs the stream synchronously, the `_async` method may return the stream directly, but callers still use the generated `_async` name.
	- Streaming in the generated code is `b.extract_resume$stream()` and `b.extract_resume$stream_async()`. Both are valid: and return an instance of `BamlStream` which is under the hood just an iterator.
	- Modular API: BAML `extract_resume$build_request()`, `$render_prompt()`, `$parse()`, and future companions are preserved verbatim as `b.extract_resume$build_request()`, `b.extract_resume$render_prompt()`, `b.extract_resume$parse()`, etc.
	- Each modular companion also gets an async binding by appending `_async`: `b.extract_resume$build_request_async()`, `b.extract_resume$render_prompt_async()`, `b.extract_resume$parse_async()`.
	- Companions on static or instance methods follow the same rule: BAML `Resume.build_from_linkedin$build_request()` becomes `Resume.build_from_linkedin$build_request()` and `Resume.build_from_linkedin$build_request_async()`; same goes for `$stream` on method-attached LLM functions (`Resume.build_from_linkedin$stream()` / `Resume.build_from_linkedin$stream_async()`).
	- baml compiler: future work planned to warn on `_async` naming collisions after TS escaping/renaming (the `$`-suffixed companion names come straight from BAML source, so their uniqueness is the compiler's responsibility upstream).

# Example generated code
Given this BAML code:

```
// user's package, in namespace `lorem`
// fully qualified BAML symbol: user.lorem.Resume
class Resume
class Resume$stream  // bamlc-generated companion type; field shapes are produced by PPIR stream expansion inside the compiler. Host-language codegen consumes this as a regular TIR class.
function extract_resume() -> Resume
function extract_resume$stream() -> Resume$stream
function extract_resume$build_request() -> baml.http.Request

// user's package, in namespace `ipsum`
// fully qualified BAML symbol: user.ipsum.Sentiment
enum Sentiment
function classify_sentiment() -> Sentiment
function classify_sentiment$build_request() -> baml.http.Request

// `aws` package, in namespace `s3`
// fully qualified BAML symbol: aws.s3.Bucket
class Bucket
function create_bucket() -> Bucket

// `baml` package aka standard library, in namespace `http`
// fully qualified BAML symbol: baml.http.Response
class Response
function fetch(url: string) -> Response

// `baml` package aka standard library, in namespace `media`
class Pdf
  function from_url(url: string) -> Pdf

// `baml` package aka standard library, in namespace `io`
class File
  function open() -> File
  function close(self)
```

Users call the generated SDK like this:

```ts
import * as b from "baml_sdk";

// `extract_resume` lives in namespace `lorem`, reached via `b.lorem.*` — never hoisted to the root.
const resume = await b.lorem.extract_resume_async() // baml_sdk.lorem.Resume
const resume_sync = b.lorem.extract_resume() // baml_sdk.lorem.Resume

const resume_stream = await b.lorem.extract_resume$stream_async()
// BamlStream<baml_sdk.lorem.Resume$stream, baml_sdk.lorem.Resume>

const resume_request = await b.lorem.extract_resume$build_request_async() // baml_sdk.baml.http.Request
const resume_request_sync = b.lorem.extract_resume$build_request() // baml_sdk.baml.http.Request

const sentiment = await b.ipsum.classify_sentiment_async() // baml_sdk.ipsum.Sentiment
const sentiment_sync = b.ipsum.classify_sentiment() // baml_sdk.ipsum.Sentiment
const sentiment_request = await b.ipsum.classify_sentiment$build_request_async() // baml_sdk.baml.http.Request
const sentiment_request_sync = b.ipsum.classify_sentiment$build_request() // baml_sdk.baml.http.Request

const bucket = await b.vendor.aws.s3.create_bucket_async() // baml_sdk.vendor.aws.s3.Bucket
const bucket_sync = b.vendor.aws.s3.create_bucket() // baml_sdk.vendor.aws.s3.Bucket

const response = await b.baml.http.fetch_async(url) // baml_sdk.baml.http.Response
const response_sync = b.baml.http.fetch(url) // baml_sdk.baml.http.Response

const pdf = await b.baml.media.Pdf.from_url_async(url) // baml_sdk.baml.media.Pdf
const pdf_sync = b.baml.media.Pdf.from_url(url) // baml_sdk.baml.media.Pdf

const file = await b.baml.io.File.open_async() // baml_sdk.baml.io.File
const file_sync = b.baml.io.File.open() // baml_sdk.baml.io.File
await file.close_async() // Promise<void>
file.close() // void
```

Generated `index.ts` examples live in [00a-example-ts-codegen-type-shapes.md](thoughts-ruby/bridge-node/00a-example-ts-codegen-type-shapes.md). Those examples are derived from the Python runtime/stub pairs in `baml_language/sdk_tests/crates/python_pydantic2/type_shapes/generated/baml_sdk` (Node collapses Python's `__init__.py` + `__init__.pyi` pair into a single typed `index.ts`).

## Exhaustive Ty conversions

Node SDK codegen should consume `baml_codegen_types::Ty`, not raw `baml_compiler2_tir::ty::Ty`. The first column below names the upstream TIR shape when there is one; the second column names the codegen-facing variant that the Node emitter should match. For `Ty::Class` / `Ty::Enum` / `Ty::TypeAlias`, references route to the generated package leaf. `$stream` class references route beside their base type in the same namespace module, with the `$stream` suffix preserved (e.g. `baml_sdk.lorem.Resume$stream`); `$stream` functions route beside their parent function in the generated namespace module (there is no aggregated client object).

| tir-ty                                      | codegen-ty                                     | Example BAML                        | Generated TypeScript symbol                                                                                                      |
| ------------------------------------------- | ---------------------------------------------- | ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `Ty::Primitive(Int)`                        | `Ty::Int`                                      | `age int`                           | `number`                                                                                                                         |
| `Ty::Primitive(Bigint)`                     | `Ty::Bigint`                                   | `value bigint`                      | `bigint`                                                                                                                         |
| `Ty::Primitive(Float)`                      | `Ty::Float`                                    | `score float`                       | `number`                                                                                                                         |
| `Ty::Primitive(String)`                     | `Ty::String`                                   | `name string`                       | `string`                                                                                                                         |
| `Ty::Primitive(Bool)`                       | `Ty::Bool`                                     | `active bool`                       | `boolean`                                                                                                                        |
| `Ty::Primitive(Null)`                       | `Ty::Null`                                     | `null` in a union                   | `null`                                                                                                                           |
| `Ty::Primitive(Uint8Array)`                 | `Ty::Uint8Array`                               | `data uint8array`                   | `Uint8Array`                                                                                                                     |
| `Ty::Primitive(Image)`                      | `Ty::Media(Image)`                             | `photo image`                       | `baml_sdk.baml.media.Image` or imported `Image` from `@boundaryml/baml-bridge`                                                          |
| `Ty::Primitive(Audio)`                      | `Ty::Media(Audio)`                             | `clip audio`                        | `baml_sdk.baml.media.Audio` or imported `Audio` from `@boundaryml/baml-bridge`                                                          |
| `Ty::Primitive(Video)`                      | `Ty::Media(Video)`                             | `clip video`                        | `baml_sdk.baml.media.Video` or imported `Video` from `@boundaryml/baml-bridge`                                                          |
| `Ty::Primitive(Pdf)`                        | `Ty::Media(Pdf)`                               | `doc pdf`                           | `baml_sdk.baml.media.Pdf` or imported `Pdf` from `@boundaryml/baml-bridge`                                                              |
| generic media source type                   | `Ty::Media(Generic)`                           | `media` / any media shape           | `unknown` until the runtime exposes a common typed media wrapper                                                                  |
| `Ty::Literal(Int(v), ...)`                  | `Ty::Literal(Int(v))`                          | `answer 42`                         | `42`                                                                                                                             |
| `Ty::Literal(Bigint(v), ...)`               | `Ty::Literal(Bigint(v))`                       | `answer 42n`                        | `42n`                                                                                                                            |
| `Ty::Literal(Float(v), ...)`                | `Ty::Literal(Float(v))`                        | float literal type                  | numeric literal type if it reaches codegen; otherwise this should be removed upstream if the parser rejects float literal types   |
| `Ty::Literal(String(v), ...)`               | `Ty::Literal(String(v))`                       | `status "draft"`                    | `"draft"`                                                                                                                        |
| `Ty::Literal(Bool(v), ...)`                 | `Ty::Literal(Bool(v))`                         | `flag true`                         | `true`                                                                                                                           |
| `Ty::EnumVariant(qtn, variant, ...)`        | `Ty::Enum(qtn)`                                | specific enum variant type          | the enum type; variant specificity is dropped before codegen                                                                      |
| `Ty::Class(qtn, args, ...)`                 | `Ty::Class(name, args)`                        | `resume Resume`                     | generated class reference, e.g. `Resume`, `lorem.Resume`, `lorem.Resume$stream`, or `Box<T>`                               |
| `Ty::Enum(qtn, ...)`                        | `Ty::Enum(name)`                               | `sentiment Sentiment`               | generated string enum reference                                                                                                  |
| `Ty::TypeAlias(qtn, ...)`                   | `Ty::TypeAlias(name)`                          | `items StringList`                  | generated alias reference                                                                                                        |
| `Ty::TypeVar(name, ...)`                    | `Ty::TypeVar(name)`                            | generic type parameter `T`          | bare type parameter name (e.g. `T`); serialization and deserialization fail unless the runtime carries concrete type metadata     |
| `Ty::Optional(T, ...)`                      | `Ty::Union([T, Null])`                         | `name string?`                      | `T \| null` (there is no `Ty::Optional` codegen variant; the emitter collapses a null-bearing union). Class fields are emitted as `field: T \| null` unless the stream companion shape marks the field omittable |
| `Ty::List(T, ...)`                          | `Ty::List(T)`                                  | `tags string[]`                     | `T[]`                                                                                                                            |
| `Ty::Map(K, V, ...)`                        | `Ty::Map { key, value }`                       | `metadata map<string, int>`         | `{ [key: string]: V }` (an inline index signature, not `Record<K, V>` — `Record` breaks recursive aliases; enum-keyed maps emit `{ [key in K]?: V }`); BAML maps decode to plain JS objects, not `Map<K, V>` |
| `Ty::Union(types, ...)`                     | `Ty::Union(types)`                             | `result string \| int`              | `string \| number` post-type-simplification                                                                                      |
| `Ty::BuiltinUnknown { ... }`                | `Ty::BuiltinUnknown`                           | `unknown` keyword                   | `unknown`                                                                                                                        |
| `Ty::Function { params, ret, throws, ... }` | `Ty::Callable { params, ret }`                 | callable type                       | `(...args) => ret` only if function values become serializable; otherwise unsupported for serialization/deserialization           |
| `Ty::Void { ... }`                          | `Ty::Unit`                                     | `-> void`                           | `null` (a `-> void` function returns `null` / `Promise<null>`; the emitter maps `Ty::Unit` to `null`, not `void`)                |
| no direct TIR variant                       | `Ty::BamlOptions`                              | generated function options plumbing | `BamlCallOptions`                                                                                                                |
| `Ty::RustType { ... }`                      | `Ty::RustType`                                 | opaque builtin state                | `BamlHandle` or the runtime-specific opaque handle type imported from `@boundaryml/baml-bridge`                                         |
| `Ty::Type { ... }`                          | does not reach Node codegen                    | `type` metatype keyword             | n/a                                                                                                                              |
| `Ty::Never { ... }`                         | does not reach Node codegen as a TS type       | divergent expression / `throws never` | n/a                                                                                                                            |
| `Ty::Future(value, error, ...)`             | does not reach Node codegen                    | `spawn { ... }` before `await`      | n/a                                                                                                                              |
| `Ty::Unknown { ... }`                       | does not reach valid codegen                   | error recovery sentinel             | n/a                                                                                                                              |
| `Ty::Error { ... }`                         | does not reach valid codegen                   | hard error sentinel                 | n/a                                                                                                                              |
| `Ty::EvolvingList(T, ...)`                  | freezes before codegen                         | mutable empty-array literal         | emitted as `Ty::List(T)` if it reaches a value boundary                                                                          |
| `Ty::EvolvingMap(K, V, ...)`                | freezes before codegen                         | mutable empty-map literal           | emitted as `Ty::Map { key, value }` if it reaches a value boundary                                                               |

Notes:
- Generated classes are runtime `class` declarations unless they are stdlib media/IO re-exports. Class fields are public properties and the constructor accepts a field object; codegen does not emit defaults, validators, or aliases.
- Recursive types across namespaces are handled by importing the generated package root as a namespace and referring to symbols through fully-qualified root-relative paths. Recursive type aliases should be emitted in a TypeScript-accepted form.
- Generated function signatures do not encode TypeScript `Result` types. Documented BAML `throws` types may be reflected in generated JSDoc, but the callable return type remains the success type.
- Dropped from the older `engine/` codegen surface: `Checked<T>` / `@check` / `@@check`, `StreamState<T>` / `@stream.state`, and `@@dynamic` class / enum codegen.

## Node error handling

Function calls return a `BamlOutboundResult` envelope. The Node runtime decoder should handle the envelope before returning to generated user code.

- `ok` decodes and returns the value.
- `error` decodes the payload and raises the Node runtime's `BamlError` equivalent. The exception carries the decoded BAML value, trace information, and an optional class name when available. Two special cases on the `error` arm: an error whose `className === 'baml.errors.HostCallable'` whose decoded `_handle` still resolves in the local host-value registry re-throws the **original** JS `Error` object (rehydrated by identity, via `HOST_VALUE_OPAQUE` handles and `host_value_registry.ts`); and `baml.panics.Cancelled` is handled per below. (Unlike Python, Node does **not** map `baml.errors.TypeMismatch` to a native `TypeError` — every other thrown class becomes a `BamlError`.)
- `panic` decodes the payload and raises the Node runtime's `BamlPanic` equivalent, except for process-exit panics where the runtime intentionally exits after flushing telemetry.
- Cancellation: a `baml.panics.Cancelled` (constant `CANCELLED_PANIC_CLASS`) is special-cased on **both** the `error` and `panic` arms and surfaces as a `BamlAbortError` (`.name = 'AbortError'`) whose `reason` is a `BamlCancelledError` (both classes in `errors.ts`).
- Generated TypeScript signatures do not encode thrown BAML types in the return type. If documented thrown types are available, codegen should put them in JSDoc.

## BexExternalValue conversions

The runtime value type returned by `bex_engine` after function execution. Defined in `baml_language/crates/bex_external_types/src/bex_external_value.rs`. Each variant is serialized to a `BamlOutboundValue` protobuf via `external_to_outbound` in `bridge_ctypes/src/value_encode.rs`, then decoded on the Node.js side by the generated/runtime decoder.

The `BamlOutboundValue.value` oneof field column shows which proto field carries each BEX variant on the wire. "(handle table)" means the value is inserted into the per-call handle table and the wire payload is the resulting handle key + `BamlHandleType` discriminator. Rows whose BEX-variant cell is `-` are proto fields with no `external_to_outbound` origin on the Node FFI path; they are listed for decoder completeness.

| `BexExternalValue` variant              | `BamlOutboundValue.value` oneof field                                                                                | Generated Node.js / TypeScript value                                                                                           |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `Null`                                  | `null_value` (`BamlValueNull`)                                                                                       | `null`                                                                                                                         |
| `Int(i64)`                              | `int_value` (`int64`)                                                                                                | `number` if safe; otherwise throw unless bridge-node explicitly chooses a bigint-preserving representation                     |
| `Bigint(BigInt)`                        | `bigint_value` (`string`, base-16)                                                                                   | `bigint`                                                                                                                       |
| `Float(f64)`                            | `float_value` (`double`)                                                                                             | `number`                                                                                                                       |
| `Bool(bool)`                            | `bool_value` (`bool`)                                                                                                | `boolean`                                                                                                                      |
| `String(String)`                        | `string_value` (`string`)                                                                                            | `string`                                                                                                                       |
| `Uint8Array(Vec<u8>)`                   | `uint8array_value` (`bytes`)                                                                                         | `Uint8Array`                                                                                                                   |
| -                                       | `literal_value` (`BamlLiteralValue`: oneof `string_value` / `int_value` / `bool_value` / `bigint_value` / `float_value`) | decoder unwraps to the inner JS primitive; the typed-literal envelope is discarded. No BEX variant should produce this on the FFI path because primitives use `string_value` / `int_value` / `bool_value` directly. |
|                                         |                                                                                                                      |                                                                                                                                |
| `Array { element_type, items }`         | `list_value` (`BamlValueList`)                                                                                       | JS array, typed as `T[]`; elements recursively decoded                                                                         |
| `Map { key_type, value_type, entries }` | `map_value` (`BamlValueMap`)                                                                                         | plain JS object typed as `{ [key: string]: V }`; values recursively decoded                                                    |
| `Instance { class_name, type_args, fields }` | `class_value` (`BamlValueClass`: `name`, `type_args`, `fields`)                                                 | corresponding generated class instance in the generated package, or a runtime class instance for handle-backed stdlib wrappers. Reified generics round-trip: the decoder reads `type_args` and defines a non-enumerable `$types` on the instance. |
| `Variant { enum_name, variant_name }`   | `enum_value` (`BamlValueEnum`: `name`, `value`, `is_dynamic`)                                                        | corresponding generated enum member or string enum value (`is_dynamic` is set `false` on the FFI path and ignored by the decoder) |
|                                         |                                                                                                                      |                                                                                                                                |
| `Union { value, metadata }`             | `union_variant_value` (`BamlValueUnionVariant` - carries `name`, `is_optional`, `is_single_pattern`, `self_type`, `value_option_name`, and inner `value`) | decoder unwraps to the inner JS value; static type is the generated TypeScript union                                           |
|                                         |                                                                                                                      |                                                                                                                                |
| `Handle(Handle)`                        | `handle_value` (`BamlOutboundHandle` - handle table)                                                                 | typed stdlib wrapper for media/IO handles, otherwise bare `BamlHandle`; decoder should reject `HANDLE_UNSPECIFIED`             |
| `FunctionRef { global_index }`          | `handle_value` (handle table; `handle_type = FUNCTION_REF`)                                                          | bare `BamlHandle`; calling it back across the bridge is unsupported today                                                      |
| `Adt(Collector(CollectorRef))`          | `handle_value` (handle table; `handle_type = ADT_COLLECTOR`)                                                         | bare `BamlHandle` unless bridge-node intentionally exposes a typed `Collector` wrapper                                         |
| `Adt(Type(RuntimeTy))`                  | `ty_value` (`BamlTy`), emitted first-class via `runtime_ty_to_proto_ty` (outbound field 21) — **not** a handle          | `null`: the TS `decodeValueHolder` has no `tyValue` branch, so a returned reflected type falls through to `return null`. BAML type-reference values are not yet decoded in Node. |
| `Adt(PromptAst(Arc<PromptAst>))`        | conditional: `prompt_ast_value` (`BamlValuePromptAst`) if encoder option `serialize_prompt_ast=true`; otherwise `handle_value` (handle table; `handle_type = ADT_PROMPT_AST`) | bare `BamlHandle` on the default FFI path; decoder should reject inline prompt AST unless bridge-node opts into it             |
| `Adt(Media(Arc<MediaValue>))`           | conditional: `media_value` (`BamlValueMedia`) if encoder option `serialize_media=true`; otherwise `handle_value` (handle table; `handle_type = ADT_MEDIA_GENERIC` or `ADT_MEDIA_*`) | typed media wrapper via the handle path; decoder should reject inline media unless bridge-node opts into it                    |
| `Adt(TaggedHeapHandle { ty, .. })`      | `handle_value` (handle table) with `BamlOutboundHandle.ty` populated (a full `BamlTy`); `BamlOutboundHandle` has no `name` field | typed wrapper picked by FQN read off `handle.ty.class_ty.name` and resolved through the typemap; the `ty`'s `type_args` parameterize the wrapper when supported |
| `RustData(Arc<dyn Any>)`                | first tries `bex_project::try_convert_rust_data`; if it converts, recurses on the converted `BexExternalValue`; otherwise `handle_value` (handle table; `handle_type = UNTAGGED_RUST_DATA`) | bare `BamlHandle` or an internal opaque runtime value                                                                          |
| `HostValue(HostValueArc)`               | `handle_value` with a host-value handle type                                                                         | host-side handle; callable host values round-trip as registered callables                                                      |
| -                                       | `media_value` (`BamlValueMedia`: `media` enum + optional `mime_type` + oneof `url` / `base64` / `file`)              | reject by default on the Node FFI path; media is expected via `handle_value`                                                   |
| -                                       | `prompt_ast_value` (`BamlValuePromptAst`: nested simple/message/multiple AST)                                        | reject by default on the Node FFI path; prompt AST is expected via `handle_value`                                              |

# Appendix

When generating Node.js TS SDKs for BAML, this is the correct thing to generate for a `make_ipsum` function
```
export const make_ipsum = defineFunction("user.symbol_collisions.lorem.make_ipsum", "sync", ["bar1", "bar2", "bar3"]) as (bar1: symbol_collisions.foo.Bar, bar2: symbol_collisions.fizz.foo.Bar, bar3: symbol_collisions.fizz.buzz.foo.Bar) => Ipsum;
```

This is wrong:
```
export const MakeIpsum = defineFunction("user.symbol_collisions.lorem.MakeIpsum", "sync", [""]) as any;
```


====

When generating Node.js TS SDKs for BAML, this is the correct way to generate cross-namespace symbol references, because it leaves minimal room for name collisions to cause issues:
```
import type * as symbol_collisions from "..";
import { defineFunction } from "@boundaryml/baml-bridge";

export class Ipsum {
    bar1!: symbol_collisions.foo.Bar;
    bar2!: symbol_collisions.fizz.foo.Bar;
    bar3!: symbol_collisions.fizz.buzz.foo.Bar;

    constructor(init: { bar1: symbol_collisions.foo.Bar; bar2: symbol_collisions.fizz.foo.Bar; bar3: symbol_collisions.fizz.buzz.foo.Bar }) { Object.assign(this, init); }
}
```

This is wrong:
```
  import * as symbol_collisions_fizz_buzz_foo from "../fizz/buzz/foo";
  import * as symbol_collisions_fizz_foo from "../fizz/foo";
  import * as symbol_collisions_foo from "../foo";
  import { defineFunction } from "@boundaryml/baml-bridge";

  export class Ipsum {
      bar1!: symbol_collisions_foo.Bar;
      bar2!: symbol_collisions_fizz_foo.Bar;
      bar3!: symbol_collisions_fizz_buzz_foo.Bar;

      constructor(init: { bar1: symbol_collisions_foo.Bar; bar2: symbol_collisions_fizz_foo.Bar; bar3: symbol_collisions_fizz_buzz_foo.Bar }) { Object.assign(this, init); }
  }
```
