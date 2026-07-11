---
date: 2026-07-08
repository: baml4
---
These are the rules that dictate how Python SDK generation works for BAML.

# Example generated code
Given this BAML code
```
// user's package, in namespace `lorem`
// fully qualified BAML symbol: user.lorem.Resume (root is a reserved pkg name in BAML)
class Resume
class Resume$stream  // bamlc-generated companion type; field shapes are produced by PPIR stream expansion inside the compiler. Host-language codegen consumes this as a regular TIR class — it does NOT re-derive a Partial<T> / Optional[T] transform at codegen time.
function extract_resume() -> Resume
function extract_resume$build_request() -> baml.http.Request  // bamlc-generated companion, `extract_resume` is an LLM-backed function

// user's package, in namespace `ipsum`
// fully qualified BAML symbol: user.ipsum.Sentiment
enum Sentiment
function classify_sentiment() -> Sentiment
function classify_sentiment$build_request() -> baml.http.Request  // bamlc-generated companion, `ClassifySentiment` is an LLM-backed function

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

We'll generate these Python symbols
```
                          // types and functions

                          // user code
                    class baml_sdk.lorem.Resume
                      def baml_sdk.lorem.extract_resume()
                async def baml_sdk.lorem.extract_resume_async()
                
                      def baml_sdk.lorem.extract_resume_stream()
                async def baml_sdk.lorem.extract_resume_stream_async()
                
                      def baml_sdk.lorem.extract_resume__build_request()
                async def baml_sdk.lorem.extract_resume__build_request_async()

                     enum baml_sdk.ipsum.Sentiment
                      def baml_sdk.ipsum.classify_sentiment()
                async def baml_sdk.ipsum.classify_sentiment_async()
                      def baml_sdk.ipsum.classify_sentiment__build_request()
                async def baml_sdk.ipsum.classify_sentiment__build_request_async()

                          // other package
                    class baml_sdk.vendor.aws.s3.Bucket
                      def baml_sdk.vendor.aws.s3.create_bucket()
                async def baml_sdk.vendor.aws.s3.create_bucket_async()

                          // standard library, static functions, instance functions
                    class baml_sdk.baml.http.Response
                      def baml_sdk.baml.http.fetch()
                async def baml_sdk.baml.http.fetch_async()

                    class baml_sdk.baml.media.Pdf
        @staticmethod def baml_sdk.baml.media.Pdf.from_url()
  @staticmethod async def baml_sdk.baml.media.Pdf.from_url_async()

                    class baml_sdk.baml.io.File
        @staticmethod def baml_sdk.baml.io.File.open()
  @staticmethod async def baml_sdk.baml.io.File.open_async()
                      def baml_sdk.baml.io.File.close(self, ...)
                async def baml_sdk.baml.io.File.close_async(self, ...)

						  // companion types
						  //   companion types have no static functions, no instance functions
					class baml_sdk.stream_types.lorem.Resume
					class baml_sdk.stream_types.vendor.aws.s3.Bucket
					class baml_sdk.stream_types.baml.io.File
					class baml_sdk.stream_types.baml.http.Response
					class baml_sdk.stream_types.baml.media.Pdf
```


## Exhaustive Ty conversions

Python SDK codegen consumes `baml_codegen_types::Ty`, not raw `baml_compiler2_tir::ty::Ty`. The first column below names the upstream TIR shape when there is one; the second column names the codegen-facing variant that the Python emitter actually matches. For `Ty::Class` / `Ty::Enum` / `Ty::TypeAlias`, references route to the generated `baml_sdk/` leaf. `$stream` class references route under `baml_sdk.stream_types`; `$stream` functions route beside their parent function.

| tir-ty                                      | codegen-ty                                     | Example BAML                          | Generated Python symbol                                                                              |
| ------------------------------------------- | ---------------------------------------------- | ------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `Ty::Primitive(Int)`                        | `Ty::Int`                                      | `age int`                             | `int`                                                                                                |
| `Ty::Primitive(Bigint)`                     | `Ty::Bigint`                                   | `value bigint`                        | `int`                                                                                                |
| `Ty::Primitive(Float)`                      | `Ty::Float`                                    | `score float`                         | `float`                                                                                              |
| `Ty::Primitive(String)`                     | `Ty::String`                                   | `name string`                         | `str`                                                                                                |
| `Ty::Primitive(Bool)`                       | `Ty::Bool`                                     | `active bool`                         | `bool`                                                                                               |
| `Ty::Primitive(Null)`                       | `Ty::Null`                                     | `null` in a union                     | `None`                                                                                               |
| `Ty::Primitive(Uint8Array)`                 | `Ty::Uint8Array`                               | `data uint8array`                     | `bytes`                                                                                              |
| `Ty::Primitive(Image)`                      | `Ty::Media(Image)`                             | `photo image`                         | `baml_sdk.baml.media.Image`                                                                          |
| `Ty::Primitive(Audio)`                      | `Ty::Media(Audio)`                             | `clip audio`                          | `baml_sdk.baml.media.Audio`                                                                          |
| `Ty::Primitive(Video)`                      | `Ty::Media(Video)`                             | `clip video`                          | `baml_sdk.baml.media.Video`                                                                          |
| `Ty::Primitive(Pdf)`                        | `Ty::Media(Pdf)`                               | `doc pdf`                             | `baml_sdk.baml.media.Pdf`                                                                            |
| generic media source type                   | `Ty::Media(Generic)`                           | `media` / any media shape             | `typing.Any`                                                                                         |
| `Ty::Literal(Int(v), ...)`                  | `Ty::Literal(Int(v))`                          | `answer 42`                           | `typing.Literal[42]`                                                                                 |
| `Ty::Literal(Bigint(v), ...)`               | `Ty::Literal(Bigint(v))`                       | `answer 42n`                          | `typing.Literal[42]`                                                                                 |
| `Ty::Literal(Float(_), ...)`                | `Ty::Literal(Float(_))`                        | float literal type                    | `typing.Any` because Python `typing.Literal` does not accept floats                                  |
| `Ty::Literal(String(v), ...)`               | `Ty::Literal(String(v))`                       | `status "draft"`                      | `typing.Literal["draft"]`                                                                            |
| `Ty::Literal(Bool(v), ...)`                 | `Ty::Literal(Bool(v))`                         | `flag true`                           | `typing.Literal[True]` / `typing.Literal[False]`                                                     |
| `Ty::EnumVariant(qtn, variant, ...)`        | `Ty::Enum(qtn)`                                | specific enum variant type            | the enum class; variant specificity is dropped before Python codegen                                 |
| `Ty::Class(qtn, args, ...)`                 | `Ty::Class(name, args)`                        | `resume Resume`                       | generated class reference, e.g. `Resume`, `lorem.Resume`, `stream_types.lorem.Resume`, or `Box[int]` |
| `Ty::Enum(qtn, ...)`                        | `Ty::Enum(name)`                               | `sentiment Sentiment`                 | generated `str, enum.Enum` class                                                                     |
| `Ty::TypeAlias(qtn, ...)`                   | `Ty::TypeAlias(name)`                          | `items StringList`                    | generated alias reference                                                                            |
| `Ty::TypeVar(name, ...)`                    | `Ty::TypeVar(name)`                            | generic type parameter `T`            | bare `T`; leaf emits `T = typing.TypeVar("T")`                                                       |
| `Ty::Optional(T, ...)`                      | `Ty::Union([T, Null])`                         | `name string?`                        | `typing.Optional[T]` (codegen has no `Ty::Optional`; the emitter collapses a null-bearing union)     |
| `Ty::List(T, ...)`                          | `Ty::List(T)`                                  | `tags string[]`                       | `typing.List[T]`                                                                                     |
| `Ty::Map(K, V, ...)`                        | `Ty::Map { key, value }`                       | `metadata map<string, int>`           | `typing.Dict[K, V]`                                                                                  |
| `Ty::Union(types, ...)`                     | `Ty::Union(types)`                             | `result string \| int`                | `typing.Union[...]` after upstream simplification                                                    |
| `Ty::BuiltinUnknown { ... }`                | `Ty::BuiltinUnknown`                           | `unknown` keyword                     | `typing.Any`                                                                                         |
| `Ty::Function { params, ret, throws, ... }` | `Ty::Callable { params, ret }`                 | callable type                         | `typing.Callable[[...], ret]`; if any parameter is optional, widens to `typing.Callable[..., ret]`   |
| `Ty::Void { ... }`                          | `Ty::Unit`                                     | `-> void`                             | `None`                                                                                               |
| no direct TIR variant                       | `Ty::BamlOptions`                              | generated function options plumbing   | `baml.Options`                                                                                       |
| `Ty::RustType { ... }`                      | `Ty::RustType`                                 | opaque builtin state                  | `_BamlPyHandle`                                                                                      |
| `Ty::Type { ... }`                          | does not reach Python codegen                  | `type` metatype keyword               | n/a                                                                                                  |
| `Ty::Never { ... }`                         | does not reach Python codegen as a Python type | divergent expression / `throws never` | n/a                                                                                                  |
| `Ty::Future(value, error, ...)`             | does not reach Python codegen                  | `spawn { ... }` before `await`        | n/a                                                                                                  |
| `Ty::Unknown { ... }`                       | does not reach valid codegen                   | error recovery sentinel               | n/a                                                                                                  |
| `Ty::Error { ... }`                         | does not reach valid codegen                   | hard error sentinel                   | n/a                                                                                                  |
| `Ty::EvolvingList(T, ...)`                  | freezes before codegen                         | mutable empty-array literal           | emitted as `Ty::List(T)` if it reaches a value boundary                                              |
| `Ty::EvolvingMap(K, V, ...)`                | freezes before codegen                         | mutable empty-map literal             | emitted as `Ty::Map { key, value }` if it reaches a value boundary                                   |

Notes:
- Generated classes are Pydantic v2 models unless they are media re-exports. The class body emits `model_config = pydantic.ConfigDict(extra="forbid")`.
- Class fields are plain `(name, type)` pairs. Codegen does not emit `pydantic.Field(...)`, aliases, defaults, constraints, or validators. An optional field renders as `field: typing.Optional[T]`, not `field: typing.Optional[T] = None`, so Pydantic treats it as required-but-nullable.
- Recursive class annotations rely on `from __future__ import annotations` and normal module imports. Recursive type aliases use `typing_extensions.TypeAliasType(...)` with quoted forward references in the alias body.
- Dropped from the older `engine/` codegen surface: `Checked<T>` / `@check` / `@@check`, `StreamState<T>` / `@stream.state`, and `@@dynamic` class / enum codegen.

## Python-specific codegen notes

- Every generated SDK directory gets both runtime `.py` files and `.pyi` stubs. Runtime files contain factory-bound values; `.pyi` files contain the typed `def` / `async def` signatures and class fields for type checkers.
- The SDK root also emits `_inlinedbaml.py`, `_typemap.py`, and `py.typed`. `_inlinedbaml.py` stores the user BAML source text. `_typemap.py` builds a lazy `BamlTypeMap` from FQN to `(module_path, attr_name)` entries for classes, enums, and aliases. `py.typed` marks the package as PEP 561 typed.
- Runtime package `__init__.py` files use PEP 562 `__getattr__` lazy child imports. This keeps `import baml_sdk` from eagerly importing every generated namespace. `.pyi` package files use explicit child re-exports because type checkers do not execute `__getattr__`.
- Free functions, companion functions, static methods, and instance methods are assigned from `baml_bridge.define_function(...)` in `.py`. The typed callable surface lives in the `.pyi`. Sync and async siblings are emitted for every callable; `$stream` becomes `_stream`, and other suffixes such as `$build_request` become `__build_request`.
- Cross-leaf references from class fields and type aliases are unconditional relative imports in `.py` because annotations / alias RHS values must resolve at module load or Pydantic schema-build time. Signature-only references are guarded by `if typing.TYPE_CHECKING:` in runtime `.py` files.

## Python error handling

Function calls return a `BamlOutboundResult` envelope. `decode_call_result` handles the envelope rather than returning raw decoded values directly.

- `ok` decodes and returns the value.
- `error` decodes the payload and raises `baml_bridge.BamlError`. The exception carries `.value`, `.baml_trace`, and an optional class name for readable messages. Special case: an `error` whose value FQN is `baml.errors.TypeMismatch` is re-surfaced as a native Python `TypeError` (message pulled from the decoded value, `baml_trace` spliced in), not a `BamlError`.
- `error` whose payload is a rehydratable host-callable exception (`baml.errors.HostCallable`) re-raises the *original* native Python exception object, looked up in the host-value registry by `_try_rehydrate_host_value`, rather than wrapping it.
- `panic` decodes the payload and raises `baml_bridge.BamlPanic`, except for exit panics: `baml.sys.exit` flushes telemetry and calls `os._exit(exit_code)`.
- Cancellation (async only): an engine `baml.panics.Cancelled` panic is wrapped as `BamlCancelledError(reason)` (a subclass of `BamlError`) and re-raised as `asyncio.CancelledError`; a caller-side `asyncio.CancelledError` triggers `cancel_function_call(call_id)` then re-raises. This wrapping happens in `baml_bridge/__init__.py::_decode_call_result_async`, not in `proto.decode_call_result`. Sync calls have no cancellation path.
- Generated `.pyi` signatures do not encode Python `Result` types. For documented thrown BAML types, codegen collects thrown type names and emits them in `Raises:` docstrings for functions / methods.

## BexExternalValue conversions

The runtime value type returned by `bex_engine` after function execution. Defined in `baml_language/crates/bex_external_types/src/bex_external_value.rs`. Each variant is serialized to a `BamlOutboundValue` protobuf via `external_to_outbound` in `bridge_ctypes/src/value_encode.rs`, then decoded on the Python side by `decode_value` / `decode_call_result` in `baml_bridge/proto.py`.

The `BamlOutboundValue.value` oneof field column shows which proto field carries each BEX variant on the wire. "(handle table)" means the value is inserted into the per-call handle table and the wire payload is the resulting handle key + `BamlHandleType` discriminator. Rows whose BEX-variant cell is `—` are proto fields with no `external_to_outbound` origin on the Python FFI path; they are listed for decoder completeness.

| `BexExternalValue` variant              | `BamlOutboundValue.value` oneof field                                                                                | Generated python symbol                                                                                                                                                |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Null`                                  | `null_value` (`BamlValueNull`)                                                                                       | `None`                                                                                                                                                                 |
| `Int(i64)`                              | `int_value` (`int64`)                                                                                                | `int`                                                                                                                                                                  |
| `Bigint(BigInt)`                        | `bigint_value` (`string`, base-16)                                                                                   | `int`                                                                                                                                                                  |
| `Float(f64)`                            | `float_value` (`double`)                                                                                             | `float`                                                                                                                                                                |
| `Bool(bool)`                            | `bool_value` (`bool`)                                                                                                | `bool`                                                                                                                                                                 |
| `String(String)`                        | `string_value` (`string`)                                                                                            | `str`                                                                                                                                                                  |
| `Uint8Array(Vec<u8>)`                   | `uint8array_value` (`bytes`)                                                                                         | `bytes`                                                                                                                                                                |
| —                                       | `literal_value` (`BamlLiteralValue`: oneof `string_literal` / `int_literal` / `bool_literal`)                        | `_decode_literal` unwraps to the inner Python value (`str` / `int` / `bool`); the typed-literal envelope is discarded. No BEX variant produces this on the FFI path — primitives go through `string_value` / `int_value` / `bool_value` directly. |
|                                         |                                                                                                                      |                                                                                                                                                                        |
| `Array { element_type, items }`         | `list_value` (`BamlValueList`)                                                                                       | `list` (elements recursively decoded)                                                                                                                                  |
| `Map { key_type, value_type, entries }` | `map_value` (`BamlValueMap`)                                                                                         | `dict` (values recursively decoded)                                                                                                                                    |
| `Instance { class_name, fields, type_args }` | `class_value` (`BamlValueClass`: `name`, `fields`, `type_args`)                                                 | corresponding class in `baml_sdk/` (or `baml_sdk.stream_types` for `$stream` companions), resolved via the lazy `BamlTypeMap` registry. `type_args` carries reified generics, so `Box[int]` / `Box.of(5)` return parameterized models. |
| `Variant { enum_name, variant_name }`   | `enum_value` (`BamlValueEnum`: `name`, `value`, `is_dynamic`)                                                        | corresponding enum in `baml_sdk/`, resolved via the lazy `BamlTypeMap` registry                                                                                        |
|                                         |                                                                                                                      |                                                                                                                                                                        |
| `Union { value, metadata }`             | `union_variant_value` (`BamlValueUnionVariant` — carries `name`, `is_optional`, `is_single_pattern`, `self_type`, `value_option_name`, inner `value`) | In Python, decoder discards the metadata and unwraps to the inner value (Python is duck-typed).<br>In Go, translated to the corresponding type generated for the union. |
|                                         |                                                                                                                      |                                                                                                                                                                        |
| `Handle(Handle)`                        | `handle_value` (`BamlOutboundHandle` — handle table)                                                                 | For media handle types: corresponding stdlib class wrapping the handle, e.g. `baml_sdk.baml.media.Image(__handle = BamlHandle())`, `…Audio`, `…Video`, `…Pdf`. For other handle kinds: bare `BamlPyHandle`. `_decode_handle` raises on `HANDLE_UNSPECIFIED`. |
| `FunctionRef { global_index }`          | `handle_value` (handle table; `handle_type = FUNCTION_REF`)                                                          | bare `BamlPyHandle` — no codegen'd wrapper; calling it back across the bridge is unsupported today                                                                     |
| `Adt(Collector(CollectorRef))`          | `handle_value` (handle table; `handle_type = ADT_COLLECTOR`)                                                         | bare `BamlPyHandle` — superseded by `BamlHandle`; not currently surfaced as a typed wrapper                                                                            |
| `Adt(Type(RuntimeTy))`                  | `ty_value` (`BamlTy`), emitted first-class via `runtime_ty_to_proto_ty` — **not** a handle                          | `None`: `decode_value` has no `ty_value` arm, so it falls through to the terminal `return None`. BAML type-reference values do not round-trip into Python.             |
| `Adt(PromptAst(Arc<PromptAst>))`        | conditional: `prompt_ast_value` (`BamlValuePromptAst`) if encoder option `serialize_prompt_ast=true`; otherwise `handle_value` (handle table; `handle_type = ADT_PROMPT_AST`). On the Python FFI path, `serialize_prompt_ast=false` — always handle. | bare `BamlPyHandle`; the Python decoder raises `BamlError` if it ever sees `prompt_ast_value` (handle path only)                                                       |
| `Adt(Media(Arc<MediaValue>))`           | conditional: `media_value` (`BamlValueMedia`) if encoder option `serialize_media=true`; otherwise `handle_value` (handle table; `handle_type = ADT_MEDIA_GENERIC` or `ADT_MEDIA_*`). On the Python FFI path, `serialize_media=false` — always handle. | superseded by the typed media stdlib class via the `Handle(...)` row; the Python decoder raises `BamlError` if it ever sees `media_value` inline                       |
| `Adt(TaggedHeapHandle { ty, .. })`      | `handle_value` (handle table) with `BamlOutboundHandle.ty` populated (a full `BamlTy`); `BamlOutboundHandle` has no `name` field | typed wrapper picked by FQN read off `handle.ty.class_ty.name`; the `ty`'s `type_args` parameterize the wrapper                                                        |
| `RustData(Arc<dyn Any>)`                | first tries `bex_project::try_convert_rust_data` — if it converts, recurses on the converted `BexExternalValue` and emits whatever that produces. Otherwise `handle_value` (handle table; `handle_type = UNTAGGED_RUST_DATA`). | bare `BamlPyHandle`, injected into the containing class's `__pydantic_private__` by `_decode_class`. |
| `HostValue(HostValueArc)`               | `handle_value` with a host-value handle type                                                                         | host-side handle; callable host values round-trip as registered callables                                                                                              |
| —                                       | `media_value` (`BamlValueMedia`: `media` enum + optional `mime_type` + oneof `url` / `base64` / `file`)              | raises `BamlError("BEX emitted 'media_value' on the FFI path — media/prompt AST are expected via handle_value, not inline")`. Media on this path always rides via `handle_value`. |
| —                                       | `prompt_ast_value` (`BamlValuePromptAst`: nested simple/message/multiple AST)                                        | raises `BamlError("BEX emitted 'prompt_ast_value' on the FFI path …")`. Same rationale as `media_value`.                                                               |
