# State of BAML <-> C# completeness

This is the canonical working parity ledger for the new C# bridge run. It mirrors
the capability identities in `state-of-python-completeness.md`, then expands the
C#-specific proof obligations required by `design.md`.

The design document, not the previous experimental implementation, defines the
target API and behavior. The dry run demonstrated that many capabilities are
feasible, but none of its claims are automatically `supported` in this ledger.
At the start of the new run:

- `planned` means the completed design requires the capability, but the current
  target branch has not yet passed the named C# parity/proof test.
- `stubbed` means generated or runtime API exists but the capability is not
  complete end to end.
- `blocked` means a named external dependency or unresolved implementation
  fact prevents completion.
- `unsupported` means v1 deliberately rejects or does not expose the shape.
- `supported` is reserved for a matching test that passes through
  `cargo nextest run -p sdk_test_csharp` or, for packaging/publish-only
  obligations, the explicitly named clean-consumer/release fixture.

Update this file continuously. Never bulk-promote rows based on compilation,
unit tests alone, a dry-run result, or a nearby capability.

Current `passed locally` evidence identifies results carried by the local
provenance commit based on
`1ebf901f7896faaec4672fdc4b2f2835db2f1cc0`. It is not reproducible from that
baseline commit alone and cannot authorize implementation entry or external
support claims until the exact provenance SHA is reviewed, pushed, and
recorded.

The “Required parity identity” cells are design routing labels, not test
citations. C4 must replace or augment each implemented row with an exact
current-branch test source and test name before that row can become
`supported`; broad fixture labels alone are insufficient.

## Function-call forms

| Python capability identity | Python | C# target status | Canonical C# API | Required parity identity | Phase/dependency | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Free function (sync) | supported | planned | `Ns.Functions.Classify(..., CancellationToken cancellationToken = default)` | `primitive_calls::dotnet` | P0 narrow E2E | Unsuffixed sync form blocks on the shared async pipeline with `GetAwaiter().GetResult()`. |
| Free function (async) | supported | planned | `await Ns.Functions.ClassifyAsync(..., cancellationToken)` | `primitive_calls::dotnet` | P0 narrow E2E | Returns `Task<T>`, never a parallel `ValueTask<T>` or tokenless overload family. |
| Static method | supported | planned | `Resume.Parse(...)` / `Resume.ParseAsync(...)` | shared static-method fixture | P4 nominal types | Same binding, defaults, generics, and final token rules as free functions. |
| Instance method | supported | planned | `agent.Reply(...)` / `agent.ReplyAsync(...)` | shared instance-method fixture | P4 nominal types | Receiver is encoded under exact BAML wire key `self`; generated calls do not mutate the CLR receiver implicitly. |
| Required args (positional) | supported | planned | normal positional C# argument | `primitive_calls::dotnet` | P0 | Generated identifier and wire key remain separate. |
| Required args (keyword) | supported | planned | normal named C# argument using projected camelCase name | `primitive_calls::dotnet` | P0 + naming | Renaming/reprojection is source-breaking in C# but must never alter the BAML wire key. |
| Optional args (omitted -> default) | supported | planned | `BamlOptional<T> argument = default` omitted | optional/default matrix | P1 optionality | `Unset` omits the wire entry so BAML evaluates its default. |
| Optional args (supplied) | supported | planned | implicit or explicit `BamlOptional<T>.FromValue(value)` | optional/default matrix | P1 optionality | Explicit null remains distinct from omission through composition with C# nullable syntax or `BamlNullable<T>`. |
| Streaming | supported | planned | one cold `FunctionStream(...) -> BamlStream<TPartial,TFinal>` | streaming parity fixture | P8/P9 streams | No `FunctionStreamAsync`, sync enumeration, event, observable, or channel alternate surface. |
| Companion `$build_request` | supported | planned | `FunctionBuildRequest(...)` / `FunctionBuildRequestAsync(...) -> BamlHttpRequest` | request-builder fixture | P8 typed companions | Application owns transport; bridge returns an immutable request snapshot. |
| Generic function/method (inferred) | supported | planned | ordinary native C# inference on `Identity<T>(T value, ...)` | generic inference matrix | P7 generic binder | The binder honors the compiler-selected closed CLR type and rejects noncanonical mappings. |
| Generic function/method (explicit mapping) | supported | planned | explicit native `<T...>` type arguments | result-only generic fixture | P7 generic binder | Required when normal C# inference cannot determine a parameter, including result-only and bare-null cases. |
| Generic function/method (subscript) | supported | planned | native `Identity<long>(value)` | generic syntax compile fixture | P7 generic binder | C# generic application is the idiomatic equivalent of Python subscript syntax. |
| Pass host callback to BAML | supported | planned | `Func<...,CancellationToken,Task<TResult>>` or `Func<...,CancellationToken,Task>` | callback parity fixture | P9 host registry | V1 limit is 15 BAML parameters because the injected token consumes one `Func` input slot. |

## Runtime behaviors

| Python capability identity | Python | C# target status | Canonical C# outcome | Required parity identity | Phase/dependency | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Normal return | supported | planned | decoded value or completed `Task<T>` | `primitive_calls::dotnet` | P0 | Sync and async share one dispatcher and result decoder. |
| BAML error | supported | planned | `BamlErrorException` with exact `BamlValue`, nullable value-derived error name, nullable managed call identity, and rendered-line `BamlTrace` | error-envelope fixture | P3 failures | Never flatten to message-only `BamlException` or invent structured frames absent from the wire. |
| BAML type mismatch | supported | planned | sealed `BamlTypeMismatchException` retaining the ordinary error value/context/trace contract | exact type-mismatch fixture | P3 failures | The current envelope carries no expected/actual/path fields; do not invent them or translate the error to `ArgumentException`. |
| BAML panic | supported | planned | `BamlPanicException` with exact decoded value, `IsExitPanic`, nullable `ExitCode`, and rendered trace | panic-envelope fixture | P3 failures | A catchable non-exit panic remains distinct from recoverable BAML errors; an exit panic terminates instead of constructing this exception. |
| Host error from callback | supported | planned | rethrow the exact original managed exception through `ExceptionDispatchInfo` | B7 managed fixture + callback parity fixture | P9 host registry | B7 proves CLR object/stack identity; product native registry parity remains. |
| Caller cancellation | supported async in Python | planned | canceled operation with `BamlOperationCanceledException`, `Origin=Caller`, winning token | B7 managed fixture + cancellation race fixture | P2/P3 | B7 proves custom canceled-task and sync semantics; product call races remain. |
| Engine cancellation | supported | planned | canceled operation with `BamlOperationCanceledException`, `Origin=Engine` | B7 managed fixture + engine-cancellation fixture | P3 | B7 proves the distinct canceled token/origin model; product envelope parity remains. |
| Stream-disposal cancellation | n/a as a separate Python row | planned | `BamlOperationCanceledException`, `Origin=StreamDisposed` | B7 managed fixture + stream lifecycle fixture | P8/P9 | Final-wait-only cancellation remains ordinary wait cancellation and does not cancel the shared operation. |
| OS exit | supported | planned | bounded event flush, then `Environment.Exit(code)` | B7 isolated child + product exit fixture | P3 | B7 proves exit code/bounded child/no-finally behavior; actual bridge flush path remains. |

## Value kinds

| Python capability identity | Python | C# target status | Canonical C# API/wire shape | Required parity identity | Phase/dependency | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Primitive: null | supported | planned | native `?`, `BamlNullable<T>`, or null-only `BamlValue.Null` by semantic position | nullability matrix | P1/P4 | Standalone normalized `null` is not projected as unconstrained `object?`. |
| Primitive: bool | supported | planned | `bool` | `primitive_calls::dotnet` | P0 | Exact wire kind; no numeric coercion. |
| Primitive: int | supported | planned | `long` with exact BAML range checks | integer-boundary fixture | P0/P4 | Reject input overflow/narrowing before native dispatch and reject oversized inbound results before CLR materialization. |
| Primitive: bigint | supported | planned | `BigInteger` with canonical signed wire encoding | bigint vectors | P1/P4 | Preserve positive, negative, zero, and high-bit cases. |
| Primitive: float | supported | planned | `double` | float vectors | P0 | No implicit `decimal`/`float` alternative mapping. |
| Primitive: string | supported | planned | `string` | `primitive_calls::dotnet` | P0 | UTF-8 pointer/length boundary is part of the interop probe. |
| Primitive: bytes | supported | planned | `ReadOnlyMemory<byte>` with snapshot/owned decode rules | byte ownership fixture | P1/P4 | Generated API does not standardize on mutable `byte[]`. |
| Container: list | supported | planned | `IReadOnlyList<T>`; owned read-only decode snapshot | recursive list fixture | P4 codecs | Concrete `List<T>` is not a canonical generic type binding merely because ordinary arguments may be snapshotted. |
| Container: map | supported | planned | `IReadOnlyDictionary<TKey,TValue>` with only canonical legal BAML keys | map-key fixture | P4 codecs | Dedicated key lowering, duplicate canonical-key detection, deterministic encoding, and nonsemantic output order. |
| Enum | supported | planned | native `enum : long`; deterministic explicit discriminants; exact string wire codec | enum evolution/golden fixture | P5 nominal types | Numeric value is never the wire identity; collision fails generation rather than renumbering. |
| Class | supported | planned | generated sealed partial class with required init-only semantic fields and generated field-by-field codec/factory | class parity fixture | P5 nominal types | No general reflection/`Activator` member discovery on the supported path. |
| Generic explicitly reified by BAML-known type | supported | planned | canonical closed generated/runtime type plus explicit descriptor | generic descriptor fixture | P7 | Includes nested canonical collections, unions, nominal types, and nullable bindings. |
| Generic implicitly reified by BAML-known type | supported | planned | compiler-inferred closed CLR type validated against BAML metadata | generic inference fixture | P7 | The bridge never changes the compiler-selected `T`. |
| Generic reified by host-only type | unsupported | unsupported | targeted `BamlTypeMappingException` before native dispatch | generic host-only rejection | P7 | Match the semantic Python non-goal; no arbitrary serializer/reflection fallback. |
| Union | supported | planned | `BamlUnion<T0,...,TN>` arities 2-32 with explicit case and canonical typed order | union parity + layout fixture | P6 | One-field-per-arm is the current evidence-backed layout; duplicate CLR projections remain distinct by case. |
| Recursive type alias | supported semantically | unsupported | targeted cycle-aware generation diagnostic before output replacement | recursive-alias compiler/generator fixture | generator semantic validation | Current Canary supplies a finite named graph, but Q18's erased CLR projection cannot represent a recursive SCC without a nominal wrapper. V1 deliberately rejects direct, mutual, collection, nullable, and union recursion; no PR #4074 wrapper or dynamic fallback. |
| BAML interface | unsupported in Python | unsupported | targeted unsupported generation/boundary behavior | interface unsupported fixture | compiler policy | Do not invent CLR interfaces or describe an `object?` stub as support. |
| Media | supported | planned | immutable non-disposable `BamlImage`/`BamlAudio`/`BamlVideo`/`BamlPdf` URL-or-owned-bytes values | media restoration fixture | P10 | No persistent public native handle; preserve representation and media type. |
| Stream | supported | planned | `BamlStream<TPartial,TFinal>` with one async enumerator and cached final task | streaming parity fixture | P8/P9 | Cold start, bounded lossless delivery, final-only mode, token roles, and disposal are separate proof rows below. |
| Host callable | supported | planned | canonical Task-returning `Func` with injected linked token | callback parity fixture | P9 | No sync `Action`, tokenless, `ValueTask`, wrapper, or interface variants. |
| Host callable (async) | supported | planned | same canonical Task-returning delegate | callback async fixture | P9 | Restore `ExecutionContext`; do not marshal a `SynchronizationContext`; application code never runs inline on native callback thread. |
| BAML closure: function-ref wire value | unsupported | unsupported | opaque `BamlHandle` only if the protocol exposes a resource; never callable as a CLR delegate | closure-handle rejection | P10 | Do not imply callable round-trip. |
| BAML closure: VM closure objects | unsupported | unsupported | typed boundary/runtime rejection | closure rejection | P3 | No inbound closure API. |
| BAML type reference value | unsupported | unsupported | no public v1 codec | type-reference rejection | P11 | A wire field alone is not support. |
| BAML type definition value | unsupported | unsupported | no managed spelling | type-definition rejection | n/a | Explicit non-goal. |
| `$rust_type` resource | supported selectively | planned | exhaustive known-FQN classification: immutable media/client values where explicitly listed, otherwise owned opaque `BamlHandle` or targeted unsupported | resource-specific fixtures | P10 | No generic typed resource-wrapper convention. Every owned native reference uses `SafeHandle`, clone/lease/release, and exact disposal rules. |
| Native host exception as opaque value | supported | planned | registry identity used only to rehydrate original managed exception | callback exception fixture | P9 | Opaque token is not a general public dynamic value. |
| Untagged BEX heap handle | internal only | unsupported | no normal public call path | internal-handle assertion | n/a | Do not claim public support. |
| Collector | incomplete in Python | unsupported | no public v1 type | collector unsupported fixture | out of scope | Revisit only through a later cross-language decision. |
| PromptAst | incomplete/handle-shaped in Python | planned | opaque owned `BamlHandle` pass-through | prompt companion fixture | P10 | V1 deliberately does not define `BamlPromptAst`, expose prompt-tree accessors, or degrade it to `object?`. |
| Builtin future value | unsupported | unsupported | boundary error; async call form is separate | future rejection | n/a | Do not expose VM future objects. |
| Arbitrary unsupported host object | unsupported | unsupported | targeted `BamlTypeMappingException` naming CLR type and path | arbitrary-object rejection | P4/P11 | `object?`, serializer conventions, tuples, anonymous objects, and arbitrary user classes are not implicit `unknown`. |
| Cyclic/self-referential objects | unsupported | unsupported | deterministic cycle rejection with limits | cycle fixture | P4/P11 | No unbounded recursion or accidental identity preservation. |

## Compatibility items

| Python capability identity | Python | C# target status | Canonical C# API | Required parity identity | Phase/dependency | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| with_options | unsupported | unsupported | none | compatibility rejection | out of scope | No support planned. |
| AbortSignal / Cancellation | supported | planned | final `CancellationToken` and resolved cancellation taxonomy | cancellation matrix | P2/P3 | Do not conflate caller token, BAML cancel-token resource, engine cancellation, or final-wait token. |
| Collector | planned in Python | unsupported | none in v1 | compatibility rejection | out of scope | Explicit rather than an erased placeholder. |
| logging / env vars | planned in Python | unsupported as a C#-specific API | ambient runtime behavior only when supplied below the bridge | compatibility assertion | out of scope | Do not invent managed configuration that diverges from other bridges. |
| AsyncClient / SyncClient | supported | planned | paired unsuffixed sync and `Async` methods | `primitive_calls::dotnet` | P0 | One shared dispatcher. |
| TypeBuilder | planned in Python | unsupported | none in v1 | compatibility rejection | out of scope | Separate future design. |
| ClientRegistry | supported in Python | unsupported as a discovery API | no generated registry enumeration/accessor in v1 | client-registry rejection | out of scope | Typed per-call `BamlClient` overrides do not imply discovery. |
| client option | supported | planned | typed `BamlOptional<BamlClient>` | client override/build-request fixture | P8 | Preserve declared runtime defaults; managed code must not inject provider/environment defaults. |
| OnTick | planned in Python | unsupported | none in v1 | compatibility rejection | out of scope | Explicit non-goal. |
| Multimodal | supported | planned | immutable media values | media fixture | P10 | URL security and byte ownership are documentation obligations. |
| Errors | incomplete in Python | planned | resolved `BamlException` hierarchy plus cancellation outside it | error taxonomy fixture | P3 | Every concrete leaf and catch category needs coverage. |
| BamlValidationError | planned in Python | unsupported | none unless a later protocol/design decision adds it | compatibility rejection | future | Do not claim ahead of the canonical runtime contract. |
| BamlClientFinishReasonError | planned in Python | unsupported | none unless later designed | compatibility rejection | future | Same rule. |
| BamlAbortError | supported | planned | `BamlOperationCanceledException` with origin/token rules | cancellation matrix | P2/P3 | Normal callers may catch `OperationCanceledException`. |

## Standard-library value and resource classification

The Python ledger groups many `$rust_type` values into one row. The dry run
proved that this hides important user-facing choices. These rows are mandatory
classification work, not approval for the experiment's wrapper types.

| BAML identity/category | C# target status | Canonical v1 classification | Required evidence/decision |
| --- | --- | --- | --- |
| `baml.http.Request` | planned | immutable `BamlHttpRequest` managed value | Exact fields/body/header duplication, redaction, and `ToHttpRequestMessage()` tests. |
| `baml.http.Response` | planned | opaque owned `BamlHandle` | Tagged-type validation plus clone/lease/release; no experimental `BamlHttpResponse`. |
| `baml.http.SseStream` | planned | opaque owned `BamlHandle` | Pass-through only; no generated `Next` wrapper or invented disposal surface. |
| `baml.http.Server` | planned | opaque owned `BamlHandle` | Pass-through only; server operations remain inside BAML. |
| `baml.http.TlsConfig` | planned | opaque owned `BamlHandle` | Certificate/private-key Rust state never becomes managed strings/bytes through guessing. |
| `baml.fs.File` | planned | opaque owned `BamlHandle` | Pass-through only; no public typed file wrapper. |
| `baml.glob.Glob` | planned | opaque owned `BamlHandle` | Pass-through only; no experimental typed glob wrapper. |
| `baml.net.TcpStream` | planned | opaque owned `BamlHandle` | Exact tagged FQN and ownership tests. |
| `baml.net.TcpListener` | planned | opaque owned `BamlHandle` | Exact tagged FQN and ownership tests. |
| `baml.net.UdpSocket` | planned | opaque owned `BamlHandle` | Exact tagged FQN and ownership tests. |
| `baml.spawn.TaskGroup` | planned | opaque owned `BamlHandle` | Pass-through only with clone/lease/release; no typed shutdown wrapper. |
| `baml.spawn.CancelToken` | planned | opaque owned `BamlHandle` | Kept distinct from .NET `CancellationToken`; it is a BAML value, not bridge control state. |
| `baml.boundary.LocalId` | planned | opaque owned `BamlHandle` | Pass-through/capture identity only. |
| `baml.csv.CsvRecord` | planned | opaque owned `BamlHandle` | Pass-through only; snapshot semantics remain native. |
| `baml.csv.CsvReader` | planned | opaque owned `BamlHandle` | Pass-through only; no experimental reader wrapper. |
| `baml.csv.CsvRows<T>` | planned | opaque owned `BamlHandle` with exact generic descriptor | Preserve the tagged generic FQN/arguments; do not manufacture a managed iterator. |
| `baml.csv.CsvWriter` | planned | opaque owned `BamlHandle` | Pass-through only; no experimental writer wrapper. |
| `baml.media.Image` | planned | immutable non-disposable `BamlImage` URL-or-owned-bytes value | URL/byte restoration and cleanup for all failure paths. |
| `baml.media.Audio` | planned | immutable non-disposable `BamlAudio` URL-or-owned-bytes value | URL/byte restoration and cleanup for all failure paths. |
| `baml.media.Video` | planned | immutable non-disposable `BamlVideo` URL-or-owned-bytes value | URL/byte restoration and cleanup for all failure paths. |
| `baml.media.Pdf` | planned | immutable non-disposable `BamlPdf` URL-or-owned-bytes value | URL/byte restoration and cleanup for all failure paths. |
| `baml.llm.Stream<TPartial,TFinal>` | planned | `BamlStream<TPartial,TFinal>` managed controller, internally owning the native stream handle | Complete cold-stream lifecycle and actual pull/backpressure proof. |
| `baml.llm.PromptAst` | planned | opaque owned `BamlHandle` | Pass-through only; v1 does not invent `BamlPromptAst` or expose prompt-tree accessors. |
| `baml.llm.OutputFormat` | unsupported | internal prompt-rendering handle; direct public SDK projection is rejected | Exact unsupported diagnostic if a user boundary exposes it. |
| `baml.llm.StreamAccumulator` | unsupported | internal LLM transport handle | Must not appear in a generated user signature. |
| `baml.llm.StreamCache<TStream,TFinal>` | unsupported | internal stream implementation handle | Must not appear in a generated user signature. |
| `baml.llm.PromptMessage` | unsupported | internal prompt-inspection shape because v1 exposes `PromptAst` only opaquely | Exact unsupported diagnostic on a direct user boundary. |
| `baml.llm.Role` | unsupported | internal prompt-template marker | Exact unsupported diagnostic on a direct user boundary. |
| `baml.llm.ContextClient` | unsupported | internal prompt-render context | Exact unsupported diagnostic on a direct user boundary. |
| `baml.llm.Context` | unsupported | internal prompt-render context | Exact unsupported diagnostic on a direct user boundary. |
| `baml.llm.OrchestrationStep` | unsupported | internal orchestration value | No public projection. |
| `baml.llm.ExecutionContext` | unsupported | internal orchestration value, unrelated to CLR `ExecutionContext` | No public projection. |
| `baml.llm.PlannerState` | unsupported | internal orchestration value | No public projection. |
| `baml.llm.Client` | planned | immutable structural `BamlClient` | Exact FQN/field codec, structural equality, recursive sub-client snapshot, shorthand factory, and no registry lookup. |
| `baml.llm.ClientType` | planned | `BamlClientType : long` with explicit stable members | Exact enum wire-name codec; zero/unknown rejected. |
| `baml.llm.RetryPolicy` | planned | immutable structural `BamlRetryPolicy` | Checked integer fields, nullable delay/multiplier fields, structural equality. |
| `baml.llm.PrimitiveClient` | unsupported | internal resolved provider/client state | Never expose credentials/provider state as a managed public object. |
| `baml.llm.MediaUrlHandler` | unsupported | internal provider media policy | No public projection. |
| `baml.llm.PrimitiveClientOptions` | unsupported | internal provider options | No public projection. |
| `baml.llm.AzureOpenAiOptions` | unsupported | internal provider options | No public projection. |
| `baml.llm.AnthropicOptions` | unsupported | internal provider options | No public projection. |
| `baml.llm.GoogleAiOptions` | unsupported | internal provider options | No public projection. |
| `baml.llm.VertexAiOptions` | unsupported | internal provider options | No public projection. |
| `baml.llm.BedrockOptions` | unsupported | internal provider options | No public projection. |
| `baml.errors.HostCallable._handle` | planned | internal host-exception registry identity | Rehydrate the original managed exception; never surface the raw handle as a public error property. |
| `CodegenTy::Resource` | planned | opaque owned `BamlHandle` with generated expected descriptor | Exact handle tag/FQN validation and ownership tests. |
| Raw `CodegenTy::RustType` in an unclassified public shape | unsupported | targeted generator diagnostic | The generator uses an exhaustive FQN allowlist above; no automatic wrapper or `object?` fallback. |
| Plain standard-library option/result shapes (`baml.glob.ScanOptions`, `baml.fs.DirEntry`/`MkdirOptions`, `baml.net.Datagram`, CSV option/error/position shapes) | planned selectively | ordinary generated immutable nominal class/enum rules when a supported public signature references them | No native ownership; field-by-field codecs and the normal public-type audit apply. |

## C#-specific interop, lifecycle, and deployment proof obligations

These rows intentionally split broad capabilities that the Python ledger groups
together. The completed design says a single “interop,” “streaming,” or
“publishing” row is insufficient.

| Obligation | Target status | Acceptance test/evidence | Phase/blocker |
| --- | --- | --- | --- |
| Source-generated `baml_get_api_v1` import and typed V1 table match the actual C ABI | passed locally | A2 plus clean packaged Linux x64 table/lifetime evidence in `TASK/abi-lifetime-evidence.md`; cross-RID remains | P0; Q1 amended |
| Default NuGet/RID native resolution | passed locally | isolated exact-package `linux-x64` asset selected into and executed from an `ubuntu.26.04-x64` publish outside the repository | P0/P2; cross-RID remains |
| One assembly-owned resolver and absolute maintainer-only override | passed locally | clean package-default publish plus valid and fail-closed `BAML_BRIDGE_CSHARP_NATIVE_LIBRARY` diagnostic processes in `TASK/abi-lifetime-evidence.md`; product runtime integration and cross-RID remain | P0 |
| No cwd/Cargo/source-tree production probing or public `Init(path)` | passed locally | published consumer executes from `/tmp`; resolver delegates only to normal .NET probing or one frozen absolute environment override | P0; final runtime API audit remains |
| UTF-8/binary pointer-length boundaries and exactly-once buffer release | passed locally | actual call/media slice covers non-ASCII/NUL/control bytes and 15 same-table releases across success/error/empty/invalid paths | P0/P3; final registry races remain |
| Literal-union producer and decoder metadata agree exactly | passed locally | A7 exact-literal Rust producer/validator tests, exact 40-byte envelope decode, and contradictory C# metadata rejection | P4; A7 |
| BAML int uses checked `[-2^62,2^62-1]` semantics in both directions | passed locally | A8 C# min/max/interior/min-1/max+1/`long`-extreme/literal/nested-list vectors | P0/P4; A8 |
| Static unmanaged callback exception containment | passed locally | actual static result callbacks plus synthetic unmanaged duplicate/invalid callback return without crossing an exception | P2/P9; product user-callback integration remains |
| Managed exception/cancellation identity and hard-exit semantics | passed locally | warning-free `TASK/failure-cancellation-evidence.md`: hierarchy/redaction, custom canceled tasks, sync direct rethrow, exact callback EDI identity, token classification, terminal races, child exit | P3/P9; actual product parity remains |
| Atomic call IDs, wrap/exhaustion, unknown/late/duplicate completion | passed locally | Rust exhaustion/concurrency tests plus actual callback copy/late/duplicate containment in `TASK/abi-lifetime-evidence.md`; final managed registry race suite remains | P2 |
| Native handle clone/lease/release and process-lifetime library ownership | passed locally | actual native clone independence and invalid/double-release behavior; final `SafeHandle` lease/dispose/finalizer stress remains | P9/P10 |
| One distinct BAML program per process; same-fingerprint reuse | passed locally | B13 actual-native 128-way lazy singleton, same-fingerprint object reuse, and pre-native conflict in `TASK/program-bootstrap-deployment-evidence.md` | P0/P2; product integration remains |
| Exact generated/managed/native version equality | passed locally | bridge registration conflict, clean packaged product-version mismatch, and A3's exact generated-contract/runtime/required-bridge checks | P2/release; product integration remains |
| Generated byte array byte-for-byte and SHA-256 integrity | passed locally | 683,918 canonical compiler bytes and a deterministic compiled/executed 16 MiB lower-bound fixture each equal one generated private hexadecimal array; SHA-256, alternate-carrier rejection, edited-byte pre-native failure, and actual-native corrupt-byte cached failure pass in `TASK/program-bootstrap-deployment-evidence.md` | P0/P2; 16 MiB is not a ceiling and product generator/runtime integration remains |
| One generated byte array/bootstrap across a multi-file program | passed locally | 128 callers through two generated namespace surfaces share one `Lazy` program and one actual native initialization for the six-file canonical fixture | P0; product emitter integration remains |
| Deterministic whole-directory generation transaction and manifest | planned | repeat/stale/failure/interruption/path-collision suite | generator P1 |
| Generator-owned directory rejects handwritten ownership assumptions | planned | user code outside directory preserved; output boundary exact | generator P1 |
| Typed naming independent of discovery order | planned | 100 permutations + hash-prefix collision fixture | generator P0 |
| Case-insensitive paths and Windows device names | planned | cross-platform routing fixtures | generator P0 |
| Internal Protobuf generation is deterministic and private | passed locally | two isolated byte-identical local generations, no-op/import graph, internal/path inspection in `TASK/protocol-generation-evidence.md`; atomic attempt 29785957216 proved four generated sources byte-identical across Linux x64, macOS ARM64, and Windows x64 | packaging P1; complete atomic fan-in remains |
| `Grpc.Tools` and schemas do not flow to consumers | passed locally | isolated exact-package cache/asset/public-surface inspection | packaging P1 |
| Frozen `Grpc.Tools` / `Google.Protobuf` compatibility | passed locally | exact 2.82.0/libprotoc 35.0 with tested 3.35.1 and range `[3.35.1,4.0.0)` plus envelope vectors | packaging P1; build-host/trim matrix remains |
| One atomic eight-RID `baml-bridge` package | blocked | Atomic attempt 29785957216 produced the exact 68,548,097-byte normalized package (`9195e1dd…`), with all eight shipping/diagnostic producers and deterministic archive/package assembly passing. Four consumers passed; both Windows consumers executed successfully before an escaped-checksum parser mismatch, and both musl jobs stopped before restore because one required environment value was not forwarded into Docker. Protocol and semantic/deployment lanes passed; completeness skipped. `TASK/package-feasibility-evidence.md` and `TASK/csharp-entry-gates-handoff.md` record exact measurements and the focused repairs. | release; Q10 feasibility |
| Only selected RID native asset reaches RID-specific publish | passed locally | sole canonical `linux-x64` package asset reaches concrete Ubuntu x64 publish byte-identically | release; other RIDs remain |
| Unsupported RID diagnostics | passed locally | exact evidence package rejects explicit `linux-s390x` with bounded `BAML0010`; warning-free eight-way runtime policy throws `PlatformNotSupportedException` with detected facts/list and never substitutes architecture/libc | release; final product target/loader and native matrix remain |
| Union one-field-per-arm binary layout | passed locally | current .NET 10 probe retains arity 2/8/16/32 sizes, copy cost, construction/match allocations, duplicate cases, invalid default, source hashes, and exact output | P6; `TASK/union-layout-evidence.md` |
| Cold stream starts exactly once | planned | enumeration-first/final-first/concurrent-start suite | P8/P9 |
| Single partial consumer and multiple cached final waiters | planned | stream lifecycle/race suite | P8/P9 |
| Bounded, ordered, lossless stream delivery | evidence passed locally; product not started | `TASK/stream-media-abi-evidence.md` executes the exact one-pull/one-completion slow-consumer mode with no queue, permits incidental initial-prefix/chunk-boundary variation while requiring strict later extensions, reconstructs the exact 789-byte final payload and SHA-256 from normalized deltas, and retains zero unsolicited idle completions, bounded callback state, cached/final-only identity, and exact cancellation/release behavior | P8/P9; implement the public stream lifecycle and pass the committed-source external package/trim reproduction plus product parity races |
| Stream factory/enumerator/final-wait token domains | planned | token/race matrix | P8/P9 |
| Early break/disposal cancels and releases exactly once | planned | stream disposal/late-result suite | P8/P9 |
| Host callback linked token classification | passed locally | B7 proves matching canceled linked-token acknowledgment versus exact unrelated-token fault without task reclassification | P9; native dispatch integration remains |
| Callback registry generation/reuse and shutdown leak diagnostics | planned | high-contention registry/GC suite | P9 |
| Optional host-callable named omission | passed locally | A5 consumes five Prost payloads emitted through Rust's production `build_to_host_call` encoder, passes every omission/null/supply case, and executes six missing/contradictory/unknown/duplicate/type/nullability rejection branches | P9; product callback lane remains |
| `BamlHandle` wrapper-identity equality and no raw native handles | passed locally | B1 actual native clone/release plus B5 SafeHandle wrapper-identity/clone/lease/idempotent-dispose/race audit with no raw public property | P10; product implementation/finalizer stress remains |
| Media URL/bytes restoration without persistent native handle | passed locally | 17-call actual-envelope probe covers URL/base64/file for image/audio/PDF/video, nominal `_data` handle unwrapping, eager ownership, MIME/representation preservation, and failure cleanup in `TASK/stream-media-abi-evidence.md` | P10; product/public-type and cross-RID parity remain |
| `BamlValue` complete kind/descriptor/limit/equality matrix | passed locally | exact 14 payload kinds plus the separate 15-case `BamlTypeDescriptorKind` with descriptor-only `Unknown`; structural descriptors/equality, public enum/class/zero-based-union inspection, owned dynamic/typed collections, strict null mapping, nominal/generic/union/media/handle identity, duplicates, cycles, and exact limits in `TASK/managed-contract-evidence.md` | P11; product wire/parity remains |
| Canonical generic closure matrix and pre-dispatch diagnostics | passed locally | positive/negative warning-as-error compile fixtures plus fail-closed runtime binder paths/replacements in `TASK/managed-contract-evidence.md` | P7/P11; generated product calls remain |
| Semantic partial transforms | passed locally | compiled default/done/not_null/with_state generated shapes and zero-default `Pending` state in `TASK/managed-contract-evidence.md` | P8/P11; compiler-emitter/parity remains |
| Normal JIT publish | passed locally | B13 clean framework-dependent publish plus self-contained single-file executions from `/tmp` in `TASK/program-bootstrap-deployment-evidence.md` | release; exact final product/cross-RID consumers remain |
| Trimmed JIT publish | evidence passed locally; product not started | exact-package `linux-x64` trimmed ABI, Protobuf/media/pull-stream, managed contract, reflection, and RID-policy fixtures publish warning-free and execute in package-default mode | release; committed-source external reproduction and final product consumers remain |
| Single-file JIT with native sidecar | passed locally | B13 self-contained single-file sidecar executes from `/tmp` with exact native bytes and no loose program assets | release; trimmed/cross-RID product remains |
| Single-file JIT with native self-extraction | passed locally | B13 one-file self-extract execution from `/tmp` restores the exact native SHA and has no sidecar/path assumption | release; trimmed/cross-RID product remains |
| Trimmed single-file JIT | evidence passed locally; product not started | fresh current-source exact-package trimmed sidecar and self-extract forms execute with exact allowed inventories and embedded generated carrier; both untrimmed forms also pass, and all four sidecar/extracted native copies are byte-identical to the package asset | release; committed-source external reproduction and final product consumers remain |
| Consumer-owned roots for reflection-only access | passed locally | trimmed rooted execution preserves the public constructor/properties under `DynamicallyAccessedMembers`, while the deliberate unrooted execution proves removal | release/docs; document the root contract and verify final product consumers |
| NativeAOT | unsupported | local direct and exact-package `buildTransitive` negative builds stop with exact `BAML0019`; no supported binary is produced | final product package target remains implementation |
| Canonical C# user documentation | planned | all credential-free examples compile/run with warnings as errors | final phase |

## Working evidence rule

The dry run is summarized in `dry-run-findings.md`. Its measurements can seed a
probe and its test cases can be ported, but the new run must record the target
commit, exact command, fresh result, and current artifact digest before using
that evidence to promote a row. A local source path remains provisional for
external promotion; reproducible evidence additionally requires that exact
path at the recorded committed SHA. If a probe disproves the completed design,
amend `design.md` explicitly; do not silently make the implementation or this
ledger the new design authority.
