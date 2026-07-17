# State of BAML <-> C# completeness

This is the working parity ledger required by `TASK/bridge-csharp.md`. Python
capability identities are preserved even when the idiomatic C# spelling differs.
`supported` is reserved for rows whose matching test passes through
`cargo nextest run -p sdk_test_csharp`.

Status vocabulary: `planned`, `stubbed`, `blocked`, `unsupported`, `supported`.

## Function-call forms

| Python capability identity | Python | C# target | Canonical C# API | Parity test | Phase/dependency | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Free function (sync) | yes | supported | `Ns.Functions.Classify(...)` | `primitive_calls::dotnet` | P0: primitive E2E | Generated `echo(string)` passes over the C ABI under nextest. |
| Free function (async) | yes | supported | `await Ns.Functions.ClassifyAsync(...)` | `primitive_calls::dotnet` | P0: primitive E2E | Async callback path passes under nextest. |
| Static method | yes | supported | `Resume.Parse(...)` | `primitive_calls::dotnet` | P4: nominal types | Sync/async static construction and returned class decoding pass. |
| Instance method | yes | supported | `agent.Reply(...)` | `primitive_calls::dotnet` | P4: class codecs | Sync/async receiver binding under wire key `self`, arguments, and defaults pass. |
| Required args (positional) | yes | supported | `Classify("spam?")` | `primitive_calls::dotnet` | P0 | Primitive binding and original wire key pass over the C ABI. |
| Required args (keyword) | yes | supported | `Classify(text: "spam?")` | `primitive_calls::dotnet` | P0 | Generated C# named argument retains its original wire key. |
| Optional args (omitted -> default) | yes | supported | `Classify("spam?")` with `BamlOptional<T> = default` | `primitive_calls::dotnet` | P1: optional E2E | Generator omits unset entries and the engine evaluates the BAML default. |
| Optional args (supplied) | yes | supported | `Classify("spam?", language: "fr")` | `primitive_calls::dotnet` | P1 | Implicit `T -> BamlOptional<T>` passes over the C ABI. |
| Streaming | yes | supported | `ClassifyStream(...) -> BamlStream<TPartial,TFinal>` | `llm_functions::dotnet` | P8: handles/streams | Replay-backed string and class streams prove pulls, iteration, cancellation, completion, serialization, and disposal. |
| Companion `$build_request` | yes | supported | `ClassifyBuildRequest(...) -> BamlHttpRequest` | `csharp_llm_clients::dotnet` | P8 | Typed client override, provider auth header, and prompt body pass without provider I/O; the LLM fixture also covers sync OpenAI and async Anthropic shapes. |
| Generic function/method (inferred) | yes | supported | `Identity<T>(T value)` | `primitive_calls::dotnet` | P7: typed descriptors | CLR inference plus explicit wire descriptors pass for scalars, lists, nullable values, generated classes, and class methods. |
| Generic function/method (explicit mapping) | yes | supported | ordinary C# generic type arguments | `primitive_calls::dotnet` | P7 | Return-only `GenericTypeName<long>()` proves the runtime receives an explicit binding without argument inference. |
| Generic function/method (subscript) | yes | supported | `Identity<long>(value)` | `primitive_calls::dotnet` | P7 | C# generic application replaces Python subscript syntax. |
| Pass host callback to BAML | yes | supported | `Func`/`Action` or generated optional delegates, plus `ValueTask` overloads | `function_calls::dotnet` | P9: host registry | Required, optional, and generic optional callbacks pass with release ownership, context flow, errors, and cancellation. |

## Runtime behaviors

| Python capability identity | Python | C# target | Canonical C# outcome | Parity test | Phase/dependency | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Normal return | yes | supported | decoded value / completed `Task<T>` | `primitive_calls::dotnet` | P0 | Sync and async result envelopes pass under nextest. |
| BAML error | docs-only typing | supported | throw `BamlError` with value and trace | `primitive_calls::dotnet` | P3: error taxonomy | Dynamic class fields, FQN, and trace survive the native boundary; generated nominal exception-value types remain future work. |
| Call-boundary type mismatch | yes | supported | throw `BamlTypeMismatchException : ArgumentException` | `primitive_calls::dotnet` | P3 | An unbound return-only generic exercises the native binder's `baml.errors.TypeMismatch` envelope; managed tests additionally pin message/value/FQN/trace preservation. |
| BAML panic | yes | supported | throw `BamlPanic` with value and trace | `primitive_calls::dotnet` | P3 | Dynamic panic class fields, FQN, and trace pass in the isolated consumer. |
| Engine-originated cancellation | yes | supported | throw `BamlCancelledException : OperationCanceledException` | `csharp_cancel_token::dotnet` | P3 | An internally canceled BAML spawn propagates the exact `baml.panics.Cancelled` class with dynamic value metadata and no caller-associated token. |
| Host error from callback | yes | supported | rethrow original managed exception object | `function_calls::dotnet` | P9 | Opaque host-value rehydration preserves reference identity and tolerates late completion after cancellation. |
| Cancellation | async only | supported | canceled `Task`; native call cancellation | `primitive_calls::dotnet` | P2: race tests | Pre-cancel, in-flight, concurrent, latency, late-callback, and recovery paths pass; call ID and callback ID are distinct. |
| OS exit | yes | supported | native event flush, then hard `Environment.Exit(code)` | `primitive_calls::dotnet` | P3: child process | ABI v1 exposes `flush_events`; isolated children prove exact codes 0 and 23 without terminating the harness. |

## Value kinds

| Python capability identity | Python | C# target | Canonical C# API/wire shape | Parity test | Phase/dependency | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Primitive: null | yes | supported | nullable projection; absent inbound oneof | `primitive_calls::dotnet` | P0 | Unset and explicit outbound null encodings are equivalent. |
| Primitive: bool | yes | supported | `bool` / `bool_value` | `primitive_calls::dotnet` | P0 | Encoded before numeric fallbacks. |
| Primitive: int | yes | supported | `long` / `int_value` | `primitive_calls::dotnet` | P0 | Full i63 boundary and managed out-of-range rejection pass. |
| Primitive: bigint | yes | supported | `BigInteger` / signed hex | `primitive_calls::dotnet` | P1 | Positive, negative, zero, and high-bit signed-hex vectors pass. |
| Primitive: float | yes | supported | `double` / `float_value` | `primitive_calls::dotnet` | P0 | Negative, zero, and positive values pass. |
| Primitive: string | yes | supported | `string` / `string_value` | `primitive_calls::dotnet` | P0 | Sync and async over-wire round trips pass. |
| Primitive: bytes | yes | supported | `byte[]` / `uint8array_value` | `primitive_calls::dotnet` | P1 | Copy semantics pass for values across the byte range. |
| Container: list | yes | supported | `List<T>` | `primitive_calls::dotnet` | P4: recursive codecs | Empty, nested, nullable, nominal, and generic-element lists pass; cyclic values are rejected. |
| Container: map | yes | supported | `Dictionary<string,V>` | `primitive_calls::dotnet` | P4 | Nested string-keyed maps, duplicate rejection, and exact outbound key metadata pass under the current compiler key contract. |
| Enum | yes | supported | native `enum : long` plus wire codec | `primitive_calls::dotnet` | P5 | Enums round-trip, preserve wire names, reject undefined zero, and use tagged identity-derived discriminants with golden and injected-collision tests. |
| Class | yes | supported | sealed partial class with required init properties | `primitive_calls::dotnet` | P5 | Empty, nested, generic, static/instance-method, and exact outbound metadata cases pass. Decode remains reflection-based. |
| Generic explicitly reified by BAML-known type | yes | supported | generated closed generic model | `primitive_calls::dotnet` | P7 | Explicit descriptors cover primitives, containers, nullable values, media, unions, and generated nominal types. |
| Generic implicitly reified by BAML-known type | yes | supported | inferred closed generic model | `primitive_calls::dotnet` | P7 | Generated CLR inference sends explicit descriptors and outbound class type args are validated exactly. |
| Generic reified by host-only type | no | unsupported | targeted pre-call managed error | `values/generic_host_only_rejected` | P7 | Same semantic non-goal as Python. |
| Union | yes | supported | `BamlUnion<T0,...,TN>` | `primitive_calls::dotnet` | P6: union family | Arity 2/3 wire cases, nullable/literal/list/nominal arms, and metadata validation pass; managed tests pin arities 16 and 32. |
| Recursive type alias | yes | supported | generated nominal wrapper over recursive `BamlUnion` | `csharp_glob::dotnet` | P6 | Nested `int | RecursiveNumbers[]` passes native parity; erased outputs must match one structural arm, ambiguous shapes are rejected, and nullable recursive aliases retain a nominal wrapper. |
| BAML interface | no | unsupported | no public typed projection in v1 | `values/interface_unsupported` | protocol evolution | Explicit non-goal. |
| Media | yes | supported | `BamlImage`, `BamlAudio`, `BamlVideo`, `BamlPdf` | `primitive_calls::dotnet` | P10: handles | All four kinds construct, clone, round-trip sync/async, expose source/MIME data, reject use-after-dispose, and release native ownership. |
| Stream value | yes | supported | `BamlStream<TPartial,TFinal>` | `llm_functions::dotnet` | P8 | Tagged heap-handle type arguments and the terminal sentinel are validated exactly. |
| Host callable | yes | supported | `Func<...>` / `Action<...>` | `function_calls::dotnet` | P9 | Native release callbacks own registry removal after argument transfer. |
| Host callable (async) | yes | supported | `Func<...,ValueTask<T>>` / `Func<...,ValueTask>` | `function_calls::dotnet` | P9 | Dispatch runs off the native thread, restores captured `ExecutionContext`, and awaits without blocking native progress. |
| BAML closure: function-ref wire value | no | unsupported | opaque `BamlHandle` only, not callable | `values/baml_closure_handle` | P10 | Match Python non-goal. |
| BAML closure: VM closure objects | no | unsupported | engine rejection surfaces as `BamlError` | `values/baml_closure_rejected` | P3 | No inbound closure API. |
| BAML type reference value | no | unsupported | no public codec in v1 | `values/type_reference_unsupported` | P11 dynamic values | Wire field alone is not support. |
| BAML type definition value | no | unsupported | no managed spelling | `values/type_definition_unsupported` | n/a | Match Python. |
| `$rust_type` resource | yes | supported | owned typed wrapper or opaque `BamlHandle` | resource-specific C# fixtures | P10 | File/HTTP, glob, cancel-token, task-group, CSV, stream/prompt, and SSE wrapper paths pass focused native or generated-codec tests. Unlisted resources intentionally expose only opaque clone/dispose ownership and no typed operations. |
| Native host exception as opaque value | yes | supported | original managed exception rehydrated | `function_calls::dotnet` | P9 | Host release removes the registry root after the same exception object is captured for rethrow. |
| Untagged BEX heap handle | internal only | unsupported | no normal public call path | `values/bex_heap_unused` | n/a | Do not claim public support. |
| Collector | incomplete | unsupported | no v1 type | `values/collector_unsupported` | out of scope | Python parity target is not supported. |
| PromptAst | incomplete | supported | owned `BamlPromptAst`; `Text`/`Messages` and async variants | `llm_functions::dotnet` | P11 | Generated render companions, clone/dispose, canonical class-envelope encode-back, and use-after-dispose pass. |
| Builtin future value | no | unsupported | boundary error | `values/future_rejected` | n/a | Async call form is separate. |
| Arbitrary unsupported host object | no | supported | targeted `BamlBridgeException` naming CLR type | `primitive_calls::dotnet` | P4 | The generated unknown path rejects an arbitrary `System.Object` before native dispatch. |
| Cyclic/self-referential objects | no | unsupported | targeted rejection before recursion | `primitive_calls::dotnet` | P5 | Reference-identity cycle detection rejects cyclic lists/classes; identity preservation is intentionally unsupported. |

## Compatibility items

| Python capability identity | Python | C# target | Canonical C# API | Parity test | Phase/dependency | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| with_options | unsupported | unsupported | none | `compat/with_options_unsupported` | out of scope | No support planned. |
| AbortSignal / Cancellation | yes | supported | `CancellationToken` on async calls | `primitive_calls::dotnet` | P2 | Awaiting a caller-canceled call throws token-associated `OperationCanceledException`; sync has no cancellation control. |
| Collector | planned | unsupported | none in v1 | `compat/collector_unsupported` | out of scope | Matches task boundary. |
| logging / env vars | planned | unsupported | no C#-specific v1 API | `compat/logging_unsupported` | out of scope | Ambient process configuration is not promoted into an untested generated/runtime compatibility surface. |
| AsyncClient / SyncClient | yes | supported | `Foo` / `FooAsync` methods | `primitive_calls::dotnet` | P0 | Generated sync/async free functions both pass under nextest. |
| TypeBuilder | planned | unsupported | none in v1 | `compat/typebuilder_unsupported` | out of scope | Matches task boundary. |
| ClientRegistry | yes | unsupported | no discovery/accessor API in v1 | `compat/client_registry_unsupported` | out of scope | Typed `BamlClient` values and per-call overrides are supported; enumerating or retrieving declared registry entries is explicitly not. |
| client option | yes | supported | `BamlOptional<BamlClient>` | `csharp_llm_clients::dotnet` | P5 | Omission preserves the declared default; an explicit named client reaches `$build_request` with exact recursive class/enum metadata. |
| OnTick | planned | unsupported | none in v1 | `compat/ontick_unsupported` | out of scope | Matches task boundary. |
| Multimodal | yes | supported | media wrappers | `primitive_calls::dotnet` | P10 | Image/audio/video/PDF values and native handle lifetimes pass in the generated consumer. |
| Errors | incomplete | supported | `BamlError` / `BamlPanic` hierarchy | `primitive_calls::dotnet` | P3 | Error and panic envelopes preserve dynamic values, class FQNs, and traces. |
| BamlValidationError | planned | unsupported | none yet | `compat/validation_error` | future | Do not claim ahead of Python. |
| BamlClientFinishReasonError | planned | unsupported | none yet | `compat/finish_reason_error` | future | Do not claim ahead of Python. |
| BamlAbortError | yes | supported | caller token -> canceled `Task`; engine cancellation -> `BamlCancelledException` | `primitive_calls::dotnet`, `csharp_cancel_token::dotnet` | P2/P3 | Caller-token cancellation stays token-associated; an engine-originated cancellation preserves the BAML panic value in a tokenless `BamlCancelledException`. |

## C#-specific proof obligations

- `BamlOptional<T>` and `BamlNullable<T>` compile/runtime matrices.
- Deterministic typed naming, including case-insensitive file routes.
- Manifest-owned C# regeneration: deterministic manifest bytes, hash-guarded
  overwrite/deletion, unrelated-file preservation, stale cleanup, rollback on
  returned failures, exclusive output ownership, and fail-closed interrupted
  state.
- Versioned API-table layout and all callback/cancellation races.
- Generated source compilation with nullable warnings as errors.
- Clean package consumption with no `Grpc.Tools` flow to consumers.
- Deterministic native-bearing package normalization and ordinary NuGet RID
  resolution; full release proof still requires all eight RID assets.
- Exact release-plan coherence across generated code, managed package/native
  handshake metadata, and the pre-initialization mismatch diagnostic.
- One distinct generated program per runtime-bearing test process.

Current evidence: generated source compiles with nullable warnings as errors;
the runtime unit suite has 86 passing tests in both Debug and Release, and the
generator suite has 18. Safe regeneration has nine filesystem unit tests and
two CLI integration tests, plus an isolated release-binary probe that removed
a namespaced stale leaf, preserved an unrelated file byte-for-byte, and refused
an edited generated file without changing the manifest or sibling output. The
broad non-listener nextest selection passes 9/9,
covering setup/build diagnostics plus primitive and nominal values, callbacks,
typed client overrides, filesystem globs, runtime cancel tokens, and task
groups, plus CSV readers, writers, records, and options. Replay-backed LLM
streams/prompts and loopback file/HTTP resources
passed in earlier focused runs, but the current sandbox prevents rerunning
those two fixtures because it denies their local listener binds. Typed versus
opaque resource coverage is frozen; trimming/AOT are explicit v1 non-goals.
Real eight-host release execution remains. The
atomic NuGet assembler, deterministic package normalization, release-version
coherence, eight-RID content validation, size ceiling, and unsupported-RID
consumer diagnostics pass local structural probes and are wired into the
release build graph.
