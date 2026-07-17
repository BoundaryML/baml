# State of the BAML/Swift bridge

This document is an overview of the current state of the BAML/Swift bridge and what is supported / partially supported / not supported.

- Function-call forms (how a BAML callable is invoked from Swift)
- Runtime-behavior forms (what a call *does* at the boundary, beyond returning a value)
- Value kinds supported across the BAML/Swift bridge
- Compatibility with `engine`
- Wishlist

# Function-call forms (how a BAML callable is invoked from Swift)

Swift inverts Python's `define_function` model: instead of binding structural callables at import time, sdkgen emits real typed `func` bodies that call the generic `BamlRuntime.shared.callSync<R>` / `call<R>` entry points. The typed surface and the runtime binding are the same artifact — there is no `.pyi`/`.py` split.

| Call form | Supported? | BAML shape | Swift call form |
| --- | --- | --- | --- |
| Free function (sync) | ✅ | `function classify(...) -> T` | `try Baml.ns.classify(...)` |
| Free function (async) | ✅ | same | `try await Baml.ns.classify_async(...)` |
| Static method | ✅ | `class Resume { function parse(...) }` | `Resume.parse(...)` (`static func`) |
| Instance method | ✅ | `class Agent { function reply(self, ...) }` | `agent.reply(...)`; `self` is encoded as required-param 0 from the receiver |
| Required args (positional) | n/a | `function classify(text: string) -> T` | Swift arguments are always labeled: `classify(text: "spam?")` — there is no unlabeled positional form |
| Required args (keyword) | ✅ | same | `classify(text: "spam?")` |
| Optional args (omitted → default) | ✅ | `function classify(text: string, lang: string = "en")` | `classify(text: "spam?")` — the parameter is `BamlOptional<T> = .unset`; `.unset` omits the kwarg and the engine fills the default (literal or engine-eval alike) |
| Optional args (supplied) | ✅ | same | `classify(text: "spam?", lang: "fr")` (via `BamlOptional`'s literal conformances) |
| Streaming | ✅ | `classify$stream(...)` | `classify_stream` / `classify_stream_async` → `BamlStream<Partial, Final>` (runtime class over a tagged handle) |
| companion functions, e.g. `$build_request` | ✅ | `classify$build_request(...)` | `classify_build_request` / `_async` (`$` → `_` wholesale) |
| Generic function / method (inferred) | ✅ | `function classify<T>(...)` | `classify(...)` — swiftc solves `T` at compile time; the wire carries **no** `type_args`, the engine re-infers from values |
| Generic function / method (explicit binding) | ✅ | `function classify<T>(...)` | `classify(x: w as Wrapper<Int>)` or an annotated `let` — explicit binding is the call site's types; no `_types=`-style kwarg exists or is needed for value-position TypeVars |
| Generic function (return-only TypeVar) | ❌ | `function parse_as<T>(s: string) -> T` | not emitted — no value carries `T`, and the wire type-hint hook (built from Swift's statically-known `T`) does not exist yet |
| Pass a Swift closure to baml | ✅ | `function run_agent(query: string, tool: (string) -> string) -> T` | `run_agent(query: "...", tool: { s in ... })` where the param is `@escaping @Sendable (String) async throws -> String`; passed as `HOST_VALUE_CALLABLE` |

# Runtime-behavior forms (what a call *does* at the boundary, beyond returning a value)

`unwrapEnvelope` decodes the `BamlOutboundResult` envelope and dispatches its `ok` / `error` / `panic` arm. These rows are the control-flow outcomes of a call, orthogonal to the value it carries — a thrown value is still decoded through the value table below; what differs is how it surfaces in Swift.

| Runtime behavior | Supported? | Trigger (BAML side / caller) | Swift outcome |
| --- | --- | --- | --- |
| Normal return | ✅ | `ok` arm | decoded value (see value table) |
| BAML error | ✅ (docs-only) | `error` arm; `throws E` written or inferred | `throw BamlError` carrying `payload: BamlOutboundValue?`; typed access via `error.value(as: MyError.self)`; thrown types appear as `- Throws:` doc lines only, never in the signature. **No** `baml.errors.TypeMismatch` → native-error special case yet (Python maps it to `TypeError`; deferred here) |
| BAML panic | ✅ | `panic` arm (non-exit) | `throw BamlPanic(message, className, bamlTrace, payload)` |
| Swift error (host callback) | ✅ | native Swift error thrown inside a passed-in host callable, surfaced back through the engine | error-path rehydration: the envelope decoder looks up the original error object in `HostCallableRegistry` (FQN `baml.errors.HostCallable`) and re-throws **that** object |
| Cancellation | ✅ (async only) | caller cancels the `Task`, or engine returns `baml.panics.Cancelled` | async: `withTaskCancellationHandler` onCancel → `cancel_function_call(call_id)` (reserve-based, race-safe); engine `baml.panics.Cancelled` → `CancellationError`. Sync calls have **no** cancellation path. `BamlCallContext` (imperative abort) is unported — structured concurrency owns cancellation |
| OS exit | ✅ | `panic` with `is_exit_panic` (`baml.sys.exit`) | `exit(code)` — a hard process exit, **not** a catchable `BamlPanic` |

# Value kinds supported across the BAML/Swift bridge

Directional shorthand: **in** = Swift→BAML encode (`BamlEncodable._bamlEncode()` → `value_decode.rs`), **out** = BAML→Swift decode (`value_encode.rs` → `BamlDecodable._bamlDecode()`).

The **Value kind** column groups rows by category. `n/a` in the Swift-value or inbound column means the value has no Swift-side spelling / no inbound path — typically because the VM→FFI conversion refuses to produce it (`CannotConvert`), so it only ever exists engine-side.

## Value table

| Value kind | Supported? | bex_vm `Object::` | BEV type (`BexExternalValue`) | Swift value type | Swift → BAML (in) | BAML → Swift (out) |
| --- | --- | --- | --- | --- | --- | --- |
| Primitive | ✅ | — (unboxed `Value::Null`) | `Null` | `nil` / `BamlNull` | absent oneof | `nil` |
| Primitive | ✅ | — (unboxed `Value::Bool`) | `Bool` | `Bool` | `bool_value` | `Bool` |
| Primitive | ✅ | — (unboxed `Value::Int`) | `Int` | `Int` | `int_value` | `Int` |
| Primitive | 🚧 | `Bigint(Arc<BigInt>)` | `Bigint` | — (no `BamlBigInt` type yet) | no encode path | decodes only when the value fits `Int`; larger values fail decode |
| Primitive | ✅ | `Float(f64)` | `Float` | `Double` | `float_value` | `Double` |
| Primitive | ✅ | `String(BexStr)` | `String` | `String` | `string_value` | `String` |
| Primitive | ✅ | `Uint8Array(..)` | `Uint8Array` | `Foundation.Data` | `uint8array_value` | `Data` |
| Container | ✅ | `Array(..)` | `Array{element_type,items}` | `Array` | `list_value` (empty → oneof presence still set) | `Array` (`item_type` ignored) |
| Container | ✅ | `Map(..)` | `Map{key_type,value_type,entries}` | `Dictionary` | `map_value`; `String` keys (BAML map keys are string-only at runtime) | `[String: V]` |
| Enum | ✅ | `Variant(..)` | `Variant{enum_name,variant_name}` | `enum: String` (`Baml.ns.Sentiment`) | `enum_value` (FQN emitted per type) | enum case; decode throws if variant absent |
| Class | ✅ | `Instance(..)` | `Instance{class_name,type_args:[],fields}` | `Equatable`/`Sendable` struct (`Baml.ns.Foo`); recursion via `@BamlIndirect` boxing | `class_value`, `class_ty.name`=base FQN; **all declared fields present** (optional = present-as-null, never omitted) | field-by-field `_bamlDecode` |
| Generic explicitly reified by BAML-known type | ✅ | `Instance{class_type_args}` | `Instance{class_name,type_args:[],fields}` | `Wrapper<Int>(value: 5)` | **no** `type_args` ever sent — swiftc bound `T` at compile time, engine re-infers from values | static return type drives decode; the compile-time binding IS the reification |
| Generic implicitly reified (inferred) | ✅ | `Instance{class_type_args}` | `Instance{class_name,type_args:[],fields}` | `Wrapper(value: 5)` (inference) | same — empty `type_args` | same |
| Generic reified by Swift-only type | n/a | — | — | — | unrepresentable: a generic argument must be `BamlCodableValue`, which only BAML-expressible types conform to — the analog of Python's `Box[threading.Lock]` is a **compile error** | — |
| Union | ✅ (**union metadata used**) | (inner value's Object) | `Union{value,metadata}` | `BamlUnion2<A,B>`…`BamlUnion8` (positional cases, type-directed layer, `match`) | encoded as the inner value (no union wrapper inbound) | metadata-first: `value_option_name` vs `_bamlArmIdentity`, then class FQN, then structural try-order — the wire metadata Python discards selects the arm here |
| BAML interface | ❌ | VM representation of interfaces (under active evolution) | — | — | — | symbols referencing interfaces are skipped by the coverage fixpoint |
| Media | ✅ | `RustData(Arc<MediaValue>)` | `Adt(Media)` | `Baml.baml.media.Image`/`Audio`/`Video`/`Pdf` (struct + `_data: BamlHandle?`) | `class_value` w/ stdlib FQN + `_data` handle; construction via `BamlMedia.fromBase64` (canonical `BamlCffiMediaKind` values) | `handle_value` `ADT_MEDIA_*` → handle into the generated struct |
| Stream | ✅ | `Instance` (FQN `baml.llm.Stream`, special-cased) | `Adt(TaggedHeapHandle{ty,heap_handle})` | `BamlStream<Partial, Final>` | `handle` (bare inner handle) | `ADT_TAGGED_HEAP_HANDLE` → `BamlStream`; `next()` → `BamlStreamNext` (`.finished` via `baml.stream.StreamFinished` sentinel) |
| Host callable | ✅ | `HostClosure(..)` | `HostValue{Callable}` | closure / `func` reference | `handle` `HOST_VALUE_CALLABLE`, registered in `HostCallableRegistry` | ok-path → bare `BamlHandle` (identity lost); error-path → rehydrate original via registry |
| Host callable (async) | ✅ | `HostClosure(..)` | `HostValue{Callable}` | `async` closure — the generated param type is `async throws`, so sync and async are the same surface | same encode; dispatched on a detached `Task` (never blocks the engine thread); `complete_host_call` exactly once | same as above |
| BAML closure | ❌ | — | `FunctionRef{global_index}` (SysOp-minted) | `BamlHandle` (bare, not callable) | n/a (no inbound path to pass a fn ref) | `handle_value` `FUNCTION_REF` → bare `BamlHandle`, **not callable back** |
| BAML closure | ❌ | `Closure` / `BoundMethod` / `GenericFunction` / `Function` | — | — | n/a — `CannotConvert` | engine-rejected before the wire → `BamlError` |
| BAML type reference values | ❌ | `Type(Box<RuntimeTy>)` | `Adt(Type(RuntimeTy))` | — | no encoder | no decode arm |
| BAML type definition values | ❌ | `Interface` / `ImplRule` / `Class` / `Enum` / `Package` | — | — | — | engine-rejected → `BamlError` |
| BAML `$rust_type` values: `baml.io.File`, `baml.net.UdpSocket`, etc | ✅ | `RustData(..)` | `RustData(Arc<dyn Any>)` | `BamlHandle` (final class; deinit releases, encode clones the key for the wire) | round-trips as `handle` in the generated struct's `_handle` field | `UNTAGGED_RUST_DATA` → `BamlHandle` into the struct field |
| native Swift error thrown by a host callback | ✅ | `RustData(Arc<HostValueArc>)` | `HostValue{Opaque}` | `BamlHandle` | n/a — minted engine-side only (envelope built by the bridge with `traceback` present-as-null) | `HOST_VALUE_OPAQUE` → bare handle; original error rehydrated by key on the error-path (registry, not `HANDLE_TABLE`) |
| n/a - unused in swift sdk | 🚧 | heap handle (`UNTAGGED_BEX_HEAP`) | `Handle(Handle)` | — (the SDK always uses `copy_objects=true`) | `handle` (preserves `handle_type` tag) | bare `BamlHandle` |
| n/a - unused in swift sdk | 🚧 | `Collector(CollectorRef)` | `Adt(Collector)` | — (`baml.llm.Collector`) | n/a | bare `BamlHandle` (catch-all) |
| n/a - unused in swift sdk | 🚧 | `RustData(Arc<PromptAst>)` | `Adt(PromptAst)` | — (`baml.llm.render_prompt` output) | n/a | bare `BamlHandle` (catch-all) |
| BAML builtin type | ❌ | `Future` / `UnscheduledFuture` | — | — | — | engine-rejected → `BamlError` |
| arbitrary unsupported Swift object | n/a | — | — | — | **statically unrepresentable**: parameters are typed, and only `BamlEncodable` conformers can appear — Python's whole mis-encode row (`set`, `datetime`, numpy, silent `TypedDict`/`NamedTuple` coercions, stringified dict keys) is a compile error in Swift | n/a |
| cyclic / self-referential objects | n/a | — | — | — | unconstructible: generated models are value-semantic structs (`@BamlIndirect` boxes recursion in the *type*, but a value graph cannot contain a reference cycle), so the unbounded-recursion failure mode does not exist | n/a |

### Notes

- **Type-erased BAML types** with no distinct *value* (interfaces as types, associated projections, `never`, `type` metatype) do not appear as value rows: unlike Python's `typing.Any` collapse, the Swift emitter's coverage fixpoint **skips** any symbol whose signature it cannot express, so unsupported types produce absent API rather than untyped API. `never` return types render as void functions (the call only returns by throwing).
- **Literal-only unions** (`"a" | "b"`) collapse to the base type (`String`); the engine validates values. Mixed literal unions translate their arms to base types first (`42 | "x"` → `BamlUnion2<Int, String>`).
- **Recursive union aliases** (`json`, `RecList`) emit a nominal indirect enum under the user's name with the exact `BamlUnionN` surface; nullable ones keep non-null arms in the enum and `?` at every reference site.

### Opaque handles aka BamlHandle

Three wire tags decode to a bare `BamlHandle`; only the first two occur through the normal Swift SDK. `BamlHandle` owns its key: `deinit` releases it (except host-value keyspaces, which the engine's release callback owns), and encoding clones a fresh key for the wire so the Swift instance stays independently droppable.

- **`UNTAGGED_RUST_DATA`** — a BAML stdlib resource class with an opaque `_handle $rust_type` field: `File`, `TcpStream`/`TcpListener`/`UdpSocket`, `Glob`, http `Response`, csv readers, `spawn` join handles. Rides out as the `_handle` field of the generated struct; passing the struct back hands the same Arc to the resource op. Media (`_data`) and `baml.errors.HostCallable` use `$rust_type` too but are peeled off to `ADT_MEDIA_*` / `HOST_VALUE_OPAQUE` and never reach this tag.
- **`HOST_VALUE_OPAQUE`** — a Swift closure passed into a BAML function threw a native error. The bridge registers the error object by key (in `HostCallableRegistry`, **not** `HANDLE_TABLE`) and wraps it in `baml.errors.HostCallable`; on the error-path the *original* error object is rehydrated by key and re-thrown. Lifetime is `HostValueArc::drop` → deferred `HostReleaseFn`. Never originates inbound.
- **`UNTAGGED_BEX_HEAP`** (and `ADT_COLLECTOR` / `ADT_PROMPT_AST`) — a live BAML heap object handed to the host without deep-copying (`copy_objects=false`). The SDK always calls with `copy_objects=true`, so these never surface through normal calls; they decode to bare handles via the catch-all.

# Compatibility with `engine`

If you're migrating to BAML v1.0 from BAML v0.2xx.y: there was no Swift SDK in the `engine` era, so every row is "new capability" rather than "migration" — the table below maps the `engine` feature set onto what the Swift bridge offers today.

| engine feature | BAML v1 Swift support? |
| --- | --- |
| with_options | ❌ - no support is planned |
| AbortSignal / Cancellation | ✅ - `Task.cancel()` cancels an in-flight async call (structured concurrency); `BamlCallContext.abort()` is **not** ported — no imperative sync abort |
| Collector | ❌ - work is planned |
| logging / env vars | ❌ - work is planned for `BAML_LOG`, `BAML_LOG_JSON`, `BAML_LOG_MAX_MESSAGE_LENGTH`, `BAML_LOG_COLOR_MODE` |
| AsyncClient / SyncClient | ✅ - BAML 1.0 generates `classify_sentiment()` and `classify_sentiment_async()` |
| TypeBuilder | ❌ - work is planned |
| ClientRegistry | ✅ - LLM functions have a `client:` optional arg; `Baml.baml.llm.Client` is a generated struct |
| client Option | ✅ - same as above |
| OnTick | ❌ - work is planned |
| Multimodal (Image, Audio, Video, Pdf) | ✅ - `Baml.baml.media.Image`, `.Audio`, `.Video`, `.Pdf` + `BamlMedia.fromBase64` |
| Errors | 🚧 - `BamlError` / `BamlPanic` with typed `value(as:)` payloads; the richer per-type error taxonomy is engine-side |
| Errors: BamlValidationError | ❌ - work is planned |
| Errors: BamlClientFinishReasonError | ❌ - work is planned |
| Errors: BamlAbortError | ✅ - engine `baml.panics.Cancelled` surfaces as Swift's native `CancellationError` |

# Wishlist

- `BamlBigInt` — a Swift arbitrary-precision type so bigints outside `Int` range decode instead of failing
- wire type-hint hook for return-only TypeVars (`parse_as<T>`) — Swift statically knows `T` at every call site; it just needs a way to tell the engine
- `baml.errors.TypeMismatch` → native error mapping (Python's `TypeError` analog)
- `Codable` conformance on generated models (deliberately deferred: BAML serde semantics vs `JSONEncoder` semantics need a decision first)
- serde delegation to BAML, i.e. `Foo.from_json()` / `foo.to_json()` delegating to BAML function calls (pairs with the `Codable` decision)
- interfaces
    - interfaces as args
    - interfaces as return values
- typed-error smoke test on a physical iOS device (simulator + macOS verified; device unwinding unverified)
- function visibility (a mechanism for hiding baml std functions from the generated Swift surface)
- mutability across the boundary (lower stakes than Python: value-semantic structs make shared mutation inexpressible today, which is arguably the right default for Swift)
