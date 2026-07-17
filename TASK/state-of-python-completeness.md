# State of BAML↔Python completeness

This document is an overview of the current state of the BAML/Python bridge and what is supported / partially supported / not supported.

- [Function-call forms (how a BAML callable is invoked from Python)](https://app.notion.com/p/State-of-BAML-Python-completeness-397bb2d2621680cc81dae8affc310c87?pvs=21)
- [Runtime-behavior forms (what a call *does* at the boundary, beyond returning a value)](https://app.notion.com/p/State-of-BAML-Python-completeness-397bb2d2621680cc81dae8affc310c87?pvs=21)
- [Value kinds supported across the BAML/Python bridge](https://app.notion.com/p/State-of-BAML-Python-completeness-397bb2d2621680cc81dae8affc310c87?pvs=21)
- [Compatibility with `engine`](https://app.notion.com/p/State-of-BAML-Python-completeness-397bb2d2621680cc81dae8affc310c87?pvs=21)
- [Wishlist](https://app.notion.com/p/State-of-BAML-Python-completeness-397bb2d2621680cc81dae8affc310c87?pvs=21)

# Function-call forms (how a BAML callable is invoked from Python)

Each generated callable is bound with `baml_core.define_function(fqn, "sync"|"async", required_names, optional_names?)`. The `.pyi` carries the typed surface; the runtime `.py` binds a structural callable.

| Call form | Supported? | BAML shape | Python call form |
| --- | --- | --- | --- |
| Free function (sync) | ✅ | `function classify(...) -> T` | `b.ns.classify(...)` |
| Free function (async) | ✅ | same | `await b.ns.classify_async(...)` |
| Static method | ✅ | `class Resume { function parse(...) }` | `Resume.parse(...)` (`staticmethod`) |
| Instance method | ✅ | `class Agent { function reply(self, ...) }` | `agent.reply(...)`; `"self"` is required-param 0 via descriptor protocol |
| Required args (positional) | ✅ | `function classify(text: string) -> T` | `classify("spam?")` (zipped against `required_param_names`) |
| Required args (keyword) | ✅ | same | `classify(text="spam?")` |
| Optional args (omitted → default) | ✅ | `function classify(text: string, lang: string = "en")` | `classify("spam?")` → engine fills `lang="en"` (literal default inline; engine-eval default → `UNSET`) |
| Optional args (supplied) | ✅ | same | `classify("spam?", lang="fr")` (keyword-only in `.pyi`) |
| Streaming | ✅ | `classify$stream(...)` | `classify_stream` / `classify_stream_async` → `BamlStream` (pure-Python `_stream.py`, wraps a handle) |
| companion functions, e.g. `$build_request` | ✅ | `classify$build_request(...)` | `classify__build_request` / `_async` |
| Generic function / method (inferred) | ✅ | `function classify<T>(...)` | `classify(...)` (type args inferred engine-side) |
| Generic function / method (`_types=` kwarg) | ✅ | `function classify<T>(...)` | `classify(..., _types={T: int})` (explicit type binding via kwarg) |
| Generic function / method (subscript) | ✅ | `function classify<T>(...)` | `classify[int](...)` (explicit type binding via subscript) |
| Pass a python callback to baml | ✅ | `function run_agent(query: string, tool: (string) -> string) -> T` | `run_agent("...", tool=my_tool)` where `my_tool: typing.Callable[[str], str]`; passed as `HOST_VALUE_CALLABLE` |

# Runtime-behavior forms (what a call *does* at the boundary, beyond returning a value)

`decode_call_result` decodes the `BamlOutboundResult` envelope and dispatches its `ok` / `error` / `panic` arm. These rows are the control-flow outcomes of a call, orthogonal to the value it carries — a thrown value is still decoded through the value table above; what differs is how it surfaces in Python.

| Runtime behavior | Supported? | Trigger (BAML side / caller) | Python outcome |
| --- | --- | --- | --- |
| Normal return | ✅ | `ok` arm | decoded value (see value table) |
| BAML error | ✅ (docs-only) | `error` arm; `throws E` written or inferred | `raise BamlError(value, baml_trace, class_name)`; thrown value decoded via the outbound value path; thrown types appear in the `Raises:` docstring only, never the signature. Special case: FQN `baml.errors.TypeMismatch` → native `TypeError` |
| BAML panic | ✅ | `panic` arm (non-exit) | `raise BamlPanic(value, baml_trace, class_name)` |
| Python error (host callback) | ✅ | native Python exception thrown inside a passed-in host callable, surfaced back through the engine | error-path rehydration: `decode_call_result` looks up the original exception object in the host-value registry (FQN `baml.errors.HostCallable`, `_try_rehydrate_host_value`) and re-raises **that** object |
| Cancellation | ✅ (async only) | caller cancels the awaitable, or engine returns `baml.panics.Cancelled` | async: `asyncio.CancelledError` caught → `cancel_function_call(call_id)` → re-raise; engine `baml.panics.Cancelled` → `BamlCancelledError(reason)`. Sync calls have **no** cancellation path |
| OS exit | ✅ | `panic` with `is_exit_panic` (`baml.sys.exit`) | flush telemetry + `os._exit(exit_code)` — a hard process exit, **not** a catchable `SystemExit` or `BamlPanic` |

# Value kinds supported across the BAML/Python bridge

Directional shorthand: **in** = Python→BAML encode (`proto.py::_set_inbound_value` → `value_decode.rs`), **out** = BAML→Python decode (`value_encode.rs` → `proto.py::decode_value`).

The **Value kind** column groups rows by category. `n/a` in the Python-value or inbound column means the value has no Python-side spelling / no inbound path — typically because the VM→FFI conversion refuses to produce it (`CannotConvert`), so it only ever exists engine-side.

## Value table

| Value kind | Supported? | bex_vm `Object::` | BEV type (`BexExternalValue`) | Python value type | Python → BAML (in) | BAML → Python (out) |
| --- | --- | --- | --- | --- | --- | --- |
| Primitive | ✅ | — (unboxed `Value::Null`) | `Null` | `None` | absent oneof | `None` |
| Primitive | ✅ | — (unboxed `Value::Bool`) | `Bool` | `bool` | `bool_value` (checked before `int`) | `bool` |
| Primitive | ✅ | — (unboxed `Value::Int`) | `Int` | `int` (fits i64) | `int_value` | `int` |
| Primitive | ✅ | `Bigint(Arc<BigInt>)` | `Bigint` | `int` (outside i64) | `bigint_value` (hex) | `int` (`_parse_hex_bigint`, capped) |
| Primitive | ✅ | `Float(f64)` | `Float` | `float` | `float_value` | `float` |
| Primitive | ✅ | `String(BexStr)` | `String` | `str` | `string_value` | `str` |
| Primitive | ✅ | `Uint8Array(..)` | `Uint8Array` | `bytes` / `bytearray` | `uint8array_value` (bytearray copied) | `bytes` |
| Container | ✅ | `Array(..)` | `Array{element_type,items}` | `list` / `tuple` | `list_value` (empty→`SetInParent`) | `list` (`item_type` ignored) |
| Container | ✅ | `Map(..)` | `Map{key_type,value_type,entries}` | `dict` | `map_value`; str/int/bool/enum keys, all stringified engine-side | `dict[str,Any]` (key/value types ignored) |
| Enum | ✅ | `Variant(..)` | `Variant{enum_name,variant_name}` | `enum.Enum` (`baml_sdk.Sentiment`) | `enum_value` (FQN via reverse typemap) | enum member; raises if variant absent |
| Class | ✅ | `Instance(..)` | `Instance{class_name,type_args:[],fields}` | Pydantic model (`baml_sdk.Foo`) | `class_value`, `class_ty.name`=base FQN; walks `dict(value)`+private handle attrs | `model_validate(fields)` via typemap |
| Generic explicitly reified by BAML-known type | ✅ | `Instance{class_type_args}` | `Instance{class_name,type_args:[Int],fields}` | Generic model (`Box[int]`) | `class_ty.type_args` via `pydantic_instance_type_args` | `type_args`→`_parameterize(cls,…)` then validate |
| Generic implicitly reified by BAML-known type (`type_args` inferred at runtime) | ✅ | `Instance{class_type_args}` | `Instance{class_name,type_args:[],fields}` | Generic model (`Box.of(5)`, does not explicitly bind `T`) | `class_ty.type_args` filled via generic inference | `type_args`→`_parameterize(cls,…)` then validate |
| Generic reified by Python-only type | ❌ (need more rules for serializing Python-only types) | — | — | Generic model (`Box[frozenset(int)]`, `Box[threading.Lock]`) | rejected | — |
| Union | ✅ (union metadata dropped) | (inner value's Object) | `Union{value,metadata}` | union-typed value (`int \| Foo`) | encoded as the inner value (no union wrapper inbound) | `union_variant_value` → inner value; **metadata discarded** |
| BAML interface | ❌ | VM representation of interfaces (under active evolution) | — | — | — | Python codegen is `typing.Any` |
| Media | ✅ | `RustData(Arc<MediaValue>)` | `Adt(Media)` | `BamlImage`/`Audio`/`Video`/`Pdf` | `class_value` w/ stdlib FQN + `_data` handle | `handle_value` `ADT_MEDIA_*` → `_from_pyhandle` |
| Stream | ✅ | `Instance` (FQN `baml.llm.Stream`, special-cased) | `Adt(TaggedHeapHandle{ty,heap_handle})` | `BamlStream` | `handle` (inner `BamlPyHandle`) | `ADT_TAGGED_HEAP_HANDLE`, class FQN off `ty` → `Stream` wrapper |
| Host callable | ✅ | `HostClosure(..)` | `HostValue{Callable}` | `def` / lambda / `Callable` | `handle` `HOST_VALUE_CALLABLE`, registered in host-value registry | ok-path → bare `BamlPyHandle` (identity lost); error-path → rehydrate original via `_try_rehydrate_host_value` |
| Host callable (async) | ✅ (works, but returned coroutine is driven to completion on new asyncio event loop) | `HostClosure(..)` | `HostValue{Callable}` | `async def` / coroutine-returning | same encode; async-ness detected at *invoke* via `asyncio.iscoroutine` on the return | invoked on a **fresh** `new_event_loop` on the dispatch thread; `Future`/non-coroutine awaitables not recognized |
| BAML closure | ❌ | — | `FunctionRef{global_index}` (SysOp-minted) | `BamlPyHandle` (bare, not callable) | n/a (no inbound path to pass a fn ref) | `handle_value` `FUNCTION_REF` → bare `BamlPyHandle`, **not callable back** |
| BAML closure | ❌ | `Closure` / `BoundMethod` / `GenericFunction` / `Function` | — | — | n/a — `CannotConvert` | engine-rejected before the wire → `BamlError` |
| BAML type reference values | ❌ | `Type(Box<RuntimeTy>)` | `Adt(Type(RuntimeTy))` | — | `ty_value` field exists but **no encoder branch** | `ty_value` on wire but **no `decode_value` arm** → `None` |
| BAML type definition values | ❌ | `Interface` / `ImplRule` / `Class` / `Enum` / `Package` | — | — | — | engine-rejected → `BamlError` |
| BAML `$rust_type` values: `baml.io.File`, `baml.net.UdpSocket`, etc | ✅ | `RustData(..)` | `RustData(Arc<dyn Any>)` | `BamlPyHandle` | round-trips as `handle` in a model's private attr; `try_convert_rust_data` can't peel it | `UNTAGGED_RUST_DATA` → bare `BamlPyHandle` into `__pydantic_private__` (the model's `_handle` attr) |
| native Python exception thrown by a host callback | ✅ | `RustData(Arc<HostValueArc>)` | `HostValue{Opaque}` | `BamlPyHandle` | n/a — minted engine-side only | `HOST_VALUE_OPAQUE` → bare handle; original exception rehydrated by key on error-path (not in `HANDLE_TABLE`) |
| n/a - unused in python sdk | 🚧 | heap handle (`UNTAGGED_BEX_HEAP`) | `Handle(Handle)` | — (only via `copy_objects=false`; the SDK always uses `copy_objects=true`) | `handle` (preserves `handle_type` tag) | `handle_value` `UNTAGGED_BEX_HEAP` → bare handle |
| n/a - unused in python sdk | 🚧 | `Collector(CollectorRef)` | `Adt(Collector)` | — (`baml.llm.Collector`) | n/a | `handle_value` `ADT_COLLECTOR` → bare `BamlPyHandle` (catch-all) |
| n/a - unused in python sdk | 🚧 | `RustData(Arc<PromptAst>)` | `Adt(PromptAst)` | — (`baml.llm.render_prompt` output) | n/a | `handle_value` `ADT_PROMPT_AST` → bare `BamlPyHandle` (catch-all) |
| BAML builtin type | ❌ | `Future` / `UnscheduledFuture` | — | — | — | engine-rejected → `BamlError` |
| arbitrary unsupported Python object | ❌ | — | — | `set`/`frozenset`, non-pydantic `dataclass`, `datetime`/`date`/`Decimal`/`UUID`/`pathlib.Path`, numpy scalars & arrays, generators/iterators, Python class objects themselves | `TypeError` naming the kwarg. **Silent mis-encode (no error):** `TypedDict`→`map_value`, `NamedTuple`/`namedtuple`→`list_value` (field names lost), non-str/int/bool/enum `dict` key (tuple/float/bytes/`None`)→`str(key)` | n/a |
| cyclic / self-referential objects | ❌ | — | — | — | unbounded recursion in `_set_inbound_value` | unbounded recursion |

### Notes

- **Type-erased BAML types** with no distinct *value* (interfaces as types, associated projections, `never`, `type` metatype, optional-arg callable types) are covered by 01a Table 8; they collapse to `typing.Any`/`None` at codegen and so do not appear as value rows here.

### Opaque handles aka BamlPyHandle

Three wire tags decode to a bare `BamlPyHandle`; only the first two occur through the normal Python SDK.

- **`UNTAGGED_RUST_DATA`** — a BAML stdlib resource class with an opaque `_handle $rust_type` field: `File`, `TcpStream`/`TcpListener`/`UdpSocket`, `Glob`, http `Response`, csv readers, `spawn` join handles. The engine holds it as `Object::RustData(Arc<resource>)`; on the way out `try_convert_rust_data` can't peel it, so it boxes into `HANDLE_TABLE` and rides out as the `_handle` private attr of the generated Pydantic model. Passing the model back (`drain` + `alloc_rust_data`) hands the same Arc to the resource op. Media (`_data $rust_type`) and `baml.errors.HostCallable` also use `$rust_type` but are peeled off to `ADT_MEDIA_*` / `HOST_VALUE_OPAQUE` and never reach this tag.
- **`HOST_VALUE_OPAQUE`** — a Python callback passed into a BAML function raised a native exception. The bridge registers the exception object by key (in its own registry, **not** `HANDLE_TABLE`) and wraps it in `baml.errors.HostCallable`; on the error-path the *original* exception object is rehydrated by key and re-raised. Lifetime is `HostValueArc::drop` → deferred `HostReleaseFn`. Never originates inbound.
- **`UNTAGGED_BEX_HEAP`** (and `ADT_COLLECTOR` / `ADT_PROMPT_AST`) — a live BAML heap object handed to the host without deep-copying (`copy_objects=false`). The public SDK always calls with `copy_objects=true` ("fully owned value, no Handle variants"), so these never surface through normal calls — they exist for internal paths only (GC-finalizer callbacks, lazy-handle embedders) and decode to bare handles via the catch-all.

# Compatibility with `engine`

If you're migrating to BAML v1.0 from BAML v0.2xx.y, then you'll need to change a lot.

| engine feature | BAML v1 support? |
| --- | --- |
| with_options | ❌ - no support is planned |
| AbortSignal / Cancellation | ✅ - BAML 1.0 supports `task.cancel()` and `BamlCallContext.abort()` |
| Collector | ❌ - work is planned |
| logging / env vars | ❌ - work is planned for `BAML_LOG`, `BAML_LOG_JSON`, `BAML_LOG_MAX_MESSAGE_LENGTH`, `BAML_LOG_COLOR_MODE` |
| AsyncClient / SyncClient | ✅ - BAML 1.0 generates `classify_sentiment()` and `classify_sentiment_async()` |
| TypeBuilder | ❌ - work is planned |
| ClientRegistry | ✅ - LLM functions in v1 have a `client=` default arg; `baml_sdk.baml.llm.Client` is now a Pydantic model |
| client Option | ✅ - LLM functions in v1 have a `client=` default arg; `baml_sdk.baml.llm.Client` is now a Pydantic model |
| OnTick | ❌ - work is planned |
| Multimodal (Image, Audio, Video, Pdf) | ✅ - BAML 1.0 has `baml_sdk.baml.media.Image`, `baml_sdk.baml.media.Audio`, etc. |
| Errors | 🚧 - BAML 1.0 has many new error types (deriving from `BamlError` and `BamlPanic`) |
| Errors: BamlValidationError | ❌ - work is planned |
| Errors: BamlClientFinishReasonError | ❌ - work is planned |
| Errors: BamlAbortError | ✅ - BAML 1.0 has `baml_sdk.baml.panics.Cancelled` |

# Wishlist

- mutability across the boundary (prerequisite: cyclic/self-referential objects preserved correctly)
- interfaces
    - interfaces as args
    - interfaces as return values
- function visibility (e.g. do we have a mechanism for hiding baml std functions from python?)
- serde delegation to BAML, i.e. `bamlfoo.from_json()` and `bamlfoo.to_json()` delegate to the BAML function calls