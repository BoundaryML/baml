# C# bridge implementation notes

Target checkout: `408b2be28afbf9005e7b50d1f5bd4621036ab1c9` on
`paulo/csharp` (observed 2026-07-15).

## 2026-07-15 proof-of-concept slice

Implemented source so far:

- `baml_language/sdks/csharp/bridge_csharp`: `net10.0` host assembly named
  `Baml.Bridge`, package ID `baml-bridge`, public namespace `Baml`.
- Pinned build-time protobuf pair: `Grpc.Tools 2.82.0` and
  `Google.Protobuf 3.35.1`. The four canonical schemas are inputs through
  `ProtoRoot`; generated C# stays in `obj/` and requests internal access.
- Dynamic binding to the checkout's versioned `baml_get_api_v1` table. The
  bridge validates ABI/table size, registers language ID 5 and SDK version
  `0.15.0`, initializes bytecode, and registers a static unmanaged callback.
- Callback IDs are managed correlation IDs (`uint32`). Runtime call IDs come
  independently from `new_function_call` (`uint64`), are encoded in
  `CallFunctionArgs.call_id`, and are used for cancellation.
- Borrowed callback payloads are copied before returning. Owned native
  `Buffer` results are copied and released exactly once in a `finally` block.
- Codecs cover null, bool, BAML's signed i63 `int`, bigint hex, float, string,
  bytes, literals, recursive lists/string-keyed maps, generated classes/enums,
  closed generic classes, structural unions, dynamic unknown values, all four
  media kinds, and opaque native handles. Result envelopes dispatch
  ok/error/panic, including dynamic class fields, FQNs, traces, and handle
  cleanup on every result branch.
  The generated C# projection uses `long`, so inbound encoding rejects values
  outside `[-2^62, 2^62 - 1]` before calling native code and points callers to
  `BigInteger` plus a BAML `bigint` parameter for larger values.
- Resolved public `BamlOptional<T>` and `BamlNullable<T>` shapes plus initial
  xUnit v3 tests. Codec tests construct and parse real generated protobuf
  messages for primitives, literals, errors, traces, and expected-type
  contradictions.
- `sdkgen_csharp` workspace crate and `output_type = "csharp"` CLI dispatch.
  It emits sync/async free functions and class methods, generic CLR methods
  with explicit `BamlTy` bindings, defaulted argument filtering, classes,
  enums, unions, flattened non-recursive aliases, media/handle types,
  idiomatic names, and a
  SHA-256-fingerprinted bytecode carrier.
- Consumer fixture: `/root/dev/baml-csharp-poc`, with sync/async calls plus a
  primitive, nullable, literal, and defaulted-argument stress matrix.
- Shared harness crate: `sdk_test_csharp`, currently opting in the
  `primitive_calls`, `function_calls`, and `llm_functions` fixtures. Its nextest setup builds `bridge_cffi`, exports
  the exact native-library path through `$NEXTEST_ENV`, and runs each generated
  .NET consumer in a separate process. The C# dotnet fixture group is serialized
  because both project references write the same `Baml.Bridge` MSBuild output.

## 2026-07-16 host-callable slice

- The managed API table now has typed host-dispatch, host-release, and
  host-completion pointers and registers static unmanaged trampolines during
  bridge initialization.
- Delegates encode as `HOST_VALUE_CALLABLE` handles in a process-wide managed
  registry. Argument-encoding rollback removes entries that were never handed
  to native; after transfer, the native last-Arc release callback owns removal.
- Dispatch copies the borrowed `BamlToHostCall` bytes, returns promptly, and
  runs the delegate on the thread pool. Required positional arguments decode to
  the delegate's CLR parameter types. The registration-time `ExecutionContext`
  is restored, while callback awaits use `ConfigureAwait(false)`.
- Generated required-argument callable types support up to 16 parameters and
  emit `Func`/`Action` plus `ValueTask` callback overloads. Generated sync BAML
  entry points reject host callables before native dispatch. Callable types
  containing optional parameters remain explicit stubs pending a C# convention
  for omitted-by-name arguments.
- Callback results use the normal inbound codec. `Task`, `ValueTask`, and their
  generic forms are awaited by the dispatcher. A thrown `BamlError` with a
  generated value is unwrapped for typed BAML catches; other managed exceptions
  become `baml.errors.HostCallable` values with opaque registry handles.
- Before returning from the native top-level callback, C# inspects an outbound
  HostCallable error and captures the original exception reference. The caller
  then rethrows it through `ExceptionDispatchInfo`, preserving object identity
  even though native release removes the registry root immediately afterward.
- The `function_calls` C# consumer proves sync and async delegates, two
  arguments, primitive and generated-class argument decoding, repeated calls,
  captured `AsyncLocal` state, BAML-side catch, typed BAML throws, original
  managed exception identity, in-flight cancellation, benign late completion,
  and post-cancel recovery.

## 2026-07-16 streaming slice

- Generated `$stream` companions now project the canonical native
  `baml.llm.Stream<TPartial, TFinal>` tagged handle as the owned managed type
  `BamlStream<TPartial, TFinal>`. The bridge invokes `Stream.next` and
  `Stream.final` through the same call ABI as ordinary functions; no new
  unmanaged streaming callback was required.
- The wrapper offers sync/async `Next` and `Final`, implements one
  `IAsyncEnumerable<TPartial>` enumeration, and supports synchronous and
  asynchronous disposal. `Next` returns
  `BamlUnion<TPartial, BamlStreamFinished>`, preserving null partials as values
  distinct from the terminal sentinel.
- A per-stream semaphore serializes pulls. Early async-enumerator disposal
  disposes the stream, normal terminal enumeration leaves it available for its
  final value, repeated final calls are supported, and use after explicit or
  early disposal throws. A pre-canceled pull never dispatches and therefore
  does not consume a partial.
- Tagged-handle decode validates stream class identity and both generic type
  arguments. The next-result decoder validates the terminal class shape and
  handles nullable partial unions without relying on erased C# nullable
  annotations. Encoding clones a temporary wire handle so disposal racing an
  in-flight call cannot invalidate native ownership.
- The opted-in `llm_functions` consumer uses checked-in replay recordings in
  isolated server/client processes. It proves sync and async string streams,
  serialized concurrent pulls, cancellation, manual pulls, async enumeration,
  completion, repeated final access, second-enumeration rejection, early
  disposal, and generated-class partial/final decoding through the native
  runtime.

## 2026-07-16 prompt-AST slice

- Generated `$render_prompt` companions now project `baml.llm.PromptAst` as
  the owned managed `BamlPromptAst` resource. It supports clone/dispose,
  sync/async readable text, structured `BamlPromptMessage` role/content
  access, and a readable `ToString()` while the handle is live.
- Prompt values cross the inbound ABI as the canonical
  `baml.llm.PromptAst { _data: handle }` class envelope. The bridge validates
  the class identity, exact `_data` field, and prompt handle tag, and clones a
  temporary reference for every accessor call.
- Native inbound conversion now restores the PromptAst ADT from that class
  envelope, matching the existing media ownership path. A focused native
  regression passes a round-tripped prompt into its standard-library `text`
  accessor so a bare opaque object cannot silently replace it.
- The replay-backed `llm_functions` C# consumer proves generated companion
  typing, clone independence, original-handle disposal, text/message rendering,
  readable `ToString()`, and use-after-dispose failure through the native
  runtime.

## 2026-07-16 HTTP request and resource slice

- Generated `$build_request` companions now return immutable
  `BamlHttpRequest` values instead of unsupported `object?` stubs. The bridge
  validates the exact `baml.http.Request` method/url/headers/body class shape
  in both directions and emits its nominal descriptor for generic and union
  positions.
- The `llm_functions` consumer builds sync OpenAI and async Anthropic requests
  in a credential-injected child process, proving provider auth headers and
  prompt body content without network I/O.
- `baml.http.Response` and `baml.fs.File` project as owned
  `BamlHttpResponse` and `BamlFile` wrappers. Their hidden `$rust_type` fields
  must carry `UNTAGGED_RUST_DATA`; encoding clones the native reference into
  the canonical class envelope, while decoding validates every public and
  private field before taking ownership.
- A dedicated `csharp_resources` fixture uses a temp file and a loopback HTTP
  server. It proves file cursor state across sync/async calls, seek/read/text,
  native close, clone independence, HTTP metadata/body access, and
  file/response use-after-dispose behavior through the native runtime.

## 2026-07-16 typed LLM-client slice

- Every generated LLM function and companion now takes
  `BamlOptional<BamlClient>` instead of `BamlOptional<object?>`. The immutable
  client model represents the canonical recursive name/type/sub-clients/retry/
  counter shape; `BamlRetryPolicy` and `BamlClientType` retain their exact
  standard-library wire identities.
- `BamlClient.FromShorthand` covers provider/model overrides, while the public
  constructor covers named primitive, fallback, and round-robin clients.
  Encoding and decoding validate all class fields, enum variants, optional
  retry fields, recursion depth, and nominal descriptors.
- The isolated `csharp_llm_clients` fixture passes an explicit named client to
  a generated `$build_request` companion and verifies its authorization header
  and prompt body without credentials, sockets, or provider I/O. The existing
  LLM fixture separately passes an explicit shorthand client in its
  build-request child.
- Generated symbols for enumerating or retrieving declared client-registry
  entries are an explicit v1 non-goal. Typed per-call client overrides are the
  supported client-selection API and do not imply registry discovery.

## 2026-07-16 SSE and engine-cancellation slice

- Generated `$parse_stream` companions now accept the owned
  `BamlSseStream` projection of `baml.http.SseStream`. The wrapper supports
  clone/dispose, sync/async `Next`, and sync/async `Close`, and validates its
  exact URL plus untagged native-handle envelope in both codec directions.
- Managed descriptor and malformed-handle tests cover the SSE transport shape,
  and the no-network LLM-client fixture proves the generated parse-stream
  signatures compile. A native SSE pull remains unverified because the current
  sandbox denies the loopback listener needed to supply an event stream.
- An outbound panic whose class is exactly `baml.panics.Cancelled` now becomes
  `BamlCancelledException`, an `OperationCanceledException` subtype preserving
  the decoded value, class FQN, and BAML trace. Other panic classes remain
  `BamlPanic`. This engine-originated path has no caller token; explicit
  caller-token cancellation continues to produce a token-associated canceled
  `Task` before any late native result can win.
- An outbound error whose class is exactly `baml.errors.TypeMismatch` becomes
  `BamlTypeMismatchException : ArgumentException`, matching the existing host
  contract that call-boundary type failures use the language's native argument
  error taxonomy. The exception retains the decoded class value, exact FQN,
  and trace, and uses its `message` field as the managed exception message.

## 2026-07-16 event-flush ABI slice

- ABI v1 now appends `flush_events` after all existing fields, preserving every
  established field offset while making the design's complete production CFFI
  surface available through the versioned table.
- C# validates the pointer during native initialization and exposes
  `BamlBridge.FlushEvents()`. Exit panics flush immediately before
  `Environment.Exit(code)`; the native sink is currently an intentional no-op
  because tracing/event production has been removed.
- Both native API layout tests pass, and the primitive generated consumer's
  isolated exit cases still produce exact codes 0 and 23 through the appended
  table entry.

## 2026-07-16 glob-resource slice

- `baml.glob.Glob` now projects as the owned `BamlGlob` wrapper with clone,
  dispose, sync/async `Matches`, and root-string or structured-options `Scan`
  overloads. `BamlGlobScanOptions` immutably represents all six canonical
  `baml.glob.ScanOptions` fields.
- Both resource classes have exact class descriptors and fail-closed codecs;
  the glob handle must be `UNTAGGED_RUST_DATA`, while options reject missing,
  extra, or mistyped fields.
- The socket-free `csharp_glob` generated consumer proves native glob creation,
  clone independence, use-after-dispose rejection, matching, default relative
  scans, configured dotfile/absolute/directory scans, and options round trips.

## 2026-07-16 runtime cancel-token resource slice

- `baml.spawn.CancelToken` now projects as the owned `BamlCancelToken` wrapper
  with clone/dispose and sync/async `Cancel` and `IsCancelled` operations. Its
  exact one-field class envelope requires an `UNTAGGED_RUST_DATA` handle.
- This is deliberately separate from the generated async method's final .NET
  `CancellationToken`: a BAML token is a shareable runtime value, while the .NET
  token controls one bridge call and carries managed task-cancellation identity.
- The socket-free `csharp_cancel_token` consumer proves fresh state, clone and
  encode-back state sharing, one-shot cancellation counts, use-after-dispose
  rejection, and `baml.spawn.CancelToken.any` propagation.

## 2026-07-16 optional host-callable slice

- Host callable types with optional parameters now generate stable
  program-specific sync and `ValueTask` delegate contracts. Optional delegate
  parameters are `BamlOptional<T>`, preserving omitted, supplied, and supplied
  null as distinct states. Callback-local generic delegate parameters close
  over the containing function or method's CLR type arguments.
- Every generated delegate parameter carries `BamlWireNameAttribute`; dispatch
  reflects the delegate type's public `Invoke` contract rather than the
  compiler-generated lambda method. Native named optional arguments therefore
  map correctly even when only a later parameter such as `z` is present.
- Missing optional parameters receive `default(BamlOptional<T>)`; supplied
  values are decoded into `BamlOptional<T>.FromValue`. Required parameters
  still fail closed when absent, and synchronous outer BAML calls continue to
  reject host callbacks before dispatch.
- The shared `function_calls` consumer passes all-unset, `y`-only, `z`-only,
  all-set, async, and generic optional-callback cases alongside the existing
  callback ownership, exception, context, and cancellation matrix.

## 2026-07-16 recursive type-alias slice

- Recursive BAML aliases now generate nominal wrapper classes whose typed
  `Value` is the recursive underlying `BamlUnion`. `BamlTypeAliasAttribute`
  preserves the exact FQN, and `IBamlTypeAliasValue` gives the codec a narrow
  unwrapping contract without representing the alias as a BAML class.
- `ProtoTypeCodec` emits `BamlTyTypeAlias`. Inbound values unwrap to their
  underlying wire value; native outbound values erase the alias/union wrapper,
  so reconstruction selects exactly one CLR arm by decoded wire shape.
  Ambiguous erased shapes fail closed instead of using declaration order.
- The `csharp_glob` native consumer round-trips a nested
  `int | RecursiveNumbers[]` alias, and managed tests pin descriptor identity,
  inbound erasure, nested outbound reconstruction, and nominal wrapping for a
  null value admitted by a recursive alias.

## 2026-07-16 task-group resource slice

- `baml.spawn.TaskGroup` now projects as owned `BamlTaskGroup`, with
  clone/dispose and sync/async access to cancellation, limit mutation/read,
  optional name, active count, and queued count. Its exact class envelope
  requires one `UNTAGGED_RUST_DATA` handle.
- The socket-free `csharp_task_group` consumer proves initial state, shared
  mutation across clone/encode-back ownership, zero-member cancellation with
  selector arguments, and use-after-dispose rejection.

## 2026-07-16 CSV resource slice

- `baml.csv.CsvWriter`, `CsvReader`, and `CsvRecord` now project as owned
  cloneable/disposable wrappers. The reader and writer preserve their nested
  file/callback fields and shared native state when cloned or encoded back;
  `BamlCsvPosition` and `BamlIteratorDone` preserve their exact class shapes.
- Writer raw/generic rows, headers, counts, text, flush, close, and immutable
  `BamlCsvWriterOptions` are typed. Reader headers, raw iteration, skipped
  diagnostics, position, close, and record generic cell/decode/map accessors
  are typed. Immutable `BamlCsvReaderOptions` covers all 18 standard-library
  fields, including explicit headers, trimming, ragged-row policy, null values,
  encoding/BOM policy, skip/error policy, and row limits. The concrete iterator
  implementation entry point is
  `baml.csv.CsvReader.root.iter.Iterator.next`, reflecting the compiler's
  interface-scoped method identity.
- A managed `onSkip` delegate can be encoded when constructing a reader, but a
  non-null callback returned inside `ReaderOptions` cannot be reconstructed as
  a CLR delegate from its opaque outbound handle and therefore fails closed.
- The native round trip for a literal union can report the first literal in
  `selected_option` while carrying a different literal value: writer option
  `terminator = "crlf"` returned selected metadata for `"lf"`. The C# options
  decoder therefore reconstructs erased string-literal unions from the actual
  string only when it matches exactly one declared literal arm. This is a
  protocol/runtime finding worth fixing independently; trusting the selected
  metadata here would silently change valid configuration.

## 2026-07-16 atomic NuGet packaging slice

- C# now participates in the frozen release version surfaces. The release-plan
  stamper updates the NuGet project, generated-code attributes read
  `baml_version::CANONICAL_VERSION`, and the managed assembly carries the exact
  package version used by the native SDK-registration handshake. Generated
  bootstrap registration supplies the same marker and fails before native
  initialization with `BamlSdkVersionMismatchException` when the runtime
  package differs.
- `pack-all-native.sh` requires exactly one file for each of the eight supported
  RIDs and packs them into one `baml-bridge` package. Both `.nupkg` and `.snupkg`
  are normalized into deterministic OPC/ZIP form. Package inspection rejects
  missing, extra, or misplaced native entries.
- `buildTransitive/baml-bridge.targets` validates both `RuntimeIdentifier` and
  `RuntimeIdentifiers`. A clean local-package consumer accepted supported RIDs
  and produced a BAML-specific diagnostic for `freebsd-x64`, including when it
  appeared inside a multi-RID list.
- Runtime detection normalizes distro-specific .NET identifiers such as
  `ubuntu.26.04-x64` to the portable packaged RID `linux-x64`; musl remains a
  distinct portable RID. Unsupported architectures, OSes, and Android/Bionic
  fail with `PlatformNotSupportedException` and the supported-RID list.
- The release graph now waits for the shared eight-target `bridge_cffi` build,
  verifies every digest, stages standard NuGet native paths, assembles the
  managed package once, enforces a 200,000,000-byte ceiling, records provenance,
  and uploads immutable package artifacts. Registry publication remains gated
  on the external `baml-bridge` ownership/trusted-publisher setup described in
  the design.
- A synthetic structure/determinism probe used the current Linux x64 release
  library in all eight named slots. It produced a 59,346,002-byte `.nupkg`
  (`ceac31082de515fdf95dca743113ae86483814a587b1dbc7a6a967f66c789791`)
  and a 128,309-byte `.snupkg`
  (`9437c8b22941e1976c6de190ed760085b904ca3e8662f452a9b3755869efd043`);
  two runs were byte-identical. This validates the assembler, not native
  architecture labels. The release matrix remains responsible for supplying
  and executing the real per-target bytes.

## 2026-07-16 generated-name collision slice

- Emitted user classes, enums, and recursive aliases now share one
  deterministic type-name allocation domain per generated leaf. Normalization
  collisions such as `foo_bar`/`fooBar` receive identity-derived suffixes, and
  every callable/property type reference uses the same allocated name.
- The synthetic `Functions` holder is reserved before user types are allocated,
  preventing a class named `Functions` from producing duplicate declarations.
  Generated paths rewrite Windows device-name namespace segments such as `CON`
  and `LPT1` while retaining legal idiomatic namespace spelling.
- Generator tests pin the colliding declarations/references and device-name
  paths. Colliding namespace identities receive distinct namespaces and
  case-insensitive routes; 100 declaration-order permutations are
  byte-identical. Contextual keywords remain legal, reserved keywords escape,
  and injected full-hash collision tests fail rather than falling back to
  discovery order.

## Verification state

The primitive free-function proof passed end to end on 2026-07-15. The sandbox
denies `/dev/null` and `/dev/urandom`, so validation commands were run with a
temporary `/tmp`-only `LD_PRELOAD` shim that implements null-device semantics
and serves .NET entropy reads from the working Linux `getrandom` syscall. The
shim is not part of the repository or product.

Verified results:

- `cargo metadata --locked --no-deps`: passed with `sdkgen_csharp` in the
  workspace and lockfile.
- `cargo check --locked -p sdkgen_csharp`: passed.
- `cargo test --locked -p sdkgen_csharp`: 18 passed.
- `cargo build --locked --release -p baml_cli -p bridge_cffi`: passed. The
  resulting native library exports `baml_get_api_v1`.
- Warning-free Debug and Release builds plus the direct in-process xUnit v3
  runner: 86 passed in each configuration. The current sandbox blocks the local
  IPC socket used by the outer `dotnet test` command, so the test assembly was
  executed directly after each build.
- The broad non-listener nextest selection passed 9/9: setup guard, build
  diagnostics, `primitive_calls`, `function_calls`, `csharp_llm_clients`,
  `csharp_glob`, `csharp_cancel_token`, and `csharp_task_group`. It deliberately
  includes `csharp_csv` reader/writer/resource and option coverage and
  skips `llm_functions` and `csharp_resources` because this sandbox denies the
  loopback binds used by those fixtures. Both listener-backed fixtures passed
  earlier when the environment allowed their local servers, and their managed
  paths remain covered by unit and focused generated-consumer tests.
- The primitive consumer covers cancellation races, nested containers,
  generated and generic classes, generic free/static/instance methods,
  nullable generic composition, structural unions, media ownership, dynamic
  values, fail-closed objects/cycles, BAML errors, and BAML panics inside its
  isolated .NET process. Child-process cases verify exact `baml.sys.exit` codes
  0 and 23 without terminating the harness.
- `function_calls` also guards the standard-library recursive JSON alias: it
  projects to `object?` because vendor symbols are not emitted, while
  user-defined recursive aliases retain nominal generated wrappers.
- Two clean direct protobuf generations remained byte-for-byte identical.
- Clean/imported-schema/direct-schema incremental generation and managed
  package inspection are recorded in `TASK/codex/protocol-package-probe.md`.
- The release CLI generated `Functions.g.cs` and
  `BamlGeneratedProgram.g.cs` for `/root/dev/baml-csharp-poc`.
- The generated consumer built with 0 warnings and 0 errors.
- With `BAML_BRIDGE_LIBRARY` pointing at the release `libbridge_cffi.so`, the
  consumer completed sync/async strings, bool, i63 boundaries, bigint, float,
  bytes, null/nullable values, defaults, and literals.
- A normalized Linux x64 native-bearing `baml-bridge 0.15.0` package restored
  through a fresh NuGet cache and ran sync/async calls without
  `BAML_BRIDGE_LIBRARY`. A RID-specific publish copied only its selected native
  asset. Two independent `tools/pack-native.sh` runs produced byte-identical
  packages; the final normalized package is 7,579,481 bytes with SHA-256
  `6a25b5624af50a1899bfca727f97a98fde560ba9d296b100b3d5b3402a92c67e`.
  The normalizer removes the random OPC identifiers emitted by raw `dotnet
  pack`.

Canonical commands outside this sandbox are:

```bash
cd /root/dev/baml/baml_language
RUSTC_WRAPPER= cargo test --locked -p sdkgen_csharp
RUSTC_WRAPPER= cargo build --locked --release -p baml_cli -p bridge_cffi

cd /root/dev/baml/baml_language/sdks/csharp/bridge_csharp
dotnet test --solution Baml.Bridge.slnx --configuration Release

cd /root/dev/baml-csharp-poc
/root/dev/baml/baml_language/target/release/baml-cli generate --from .
dotnet build --configuration Release
BAML_BRIDGE_LIBRARY=/root/dev/baml/baml_language/target/release/libbridge_cffi.so \
  dotnet run --configuration Release --no-build
```

Expected consumer output:

```text
sync=hello from C#
async=hello from C#
primitive/default matrix=passed
```

Protocol-generation evidence:

- The `Grpc.Tools 2.82.0` package bundles `libprotoc 35.0` for Linux x64.
- Its MSBuild targets map `<Protobuf Access="Internal">` to the
  `internal_access` C# option.
- Two clean direct generations from all four canonical schemas compared
  byte-for-byte equal.
- Generated transport classes including `InboundValue`, `CallFunctionArgs`,
  and `BamlOutboundResult` are `internal sealed partial` and use namespace
  `BamlBridge.Cffi.V1`, matching the handwritten adapters.
- Generated-source SHA-256 values from that probe were:
  `BamlHandle.cs=3469310d8103bd55a7d7747f970ce1eb053316d2e2eb63b32681100afcfb3188`,
  `BamlInbound.cs=6e948f31a1a0f1def4b1035073db632d2a810b77dde5df0a9eff2d4edcb08fa4`,
  `BamlOutbound.cs=11af1adb6d02b8b1137d2d66c1c580069a0b5eb632ee7a4b11caa42ae78a9988`,
  and `BamlType.cs=1692c078e763ce7a1ae3c8eec48469bd97326c39337ba74e960f334feca3952b`.

This is still not the full question-9 probe. The complete supported-host
matrix, trimming/NativeAOT, and version-skew coverage remain outstanding.
Deterministic generation, direct/imported-schema invalidation, managed
compilation, package contents/dependencies, generated-source compilation,
primitive runtime round trips, and a clean package consumer now pass on Linux
x64.

## 2026-07-16 safe C# regeneration slice

- C# generation now owns output through deterministic schema-1
  `.baml-generated-files.json` entries containing sorted portable paths and
  lowercase SHA-256 digests. The manifest is the commit point and is not
  included in the reported generated-file count.
- Regeneration preflights all current ownership before staging. It refuses a
  modified owned file, corrupt or wrong-generator manifest, user-owned path
  collision, symlink ancestor, unsafe/non-portable path, case-insensitive
  prefix collision, or duplicate canonical output directory without changing
  existing output.
- New files are staged beneath the output root, all affected files and the old
  manifest are renamed to an internal backup, staged output is installed, and
  the next manifest is installed last. Returned commit failures restore the
  backup. A successful commit removes stale owned files and empty
  generated-only parents while leaving unrelated files byte-for-byte intact.
- `.baml-generation-lock` serializes writers. A hard kill or abandoned
  `.baml-generation-*` directory fails closed on the next run. Automatic crash
  recovery was deliberately rejected for this slice because inferring commit
  versus rollback could overwrite a post-crash user edit; an operator must
  inspect and remove interrupted state.
- Manifest ownership is currently C#-only. Enabling it for Python or
  TypeScript requires a migration rule for existing bannerless artifacts such
  as `py.typed`; silently claiming equal or empty files would make later stale
  deletion unsafe.
- `cargo test --locked -p baml_cli generated_output_tests` passes 9/9.
  Focused `exit_code_e2e` runs pass the C# stale/remove/edit workflow and
  duplicate-output refusal. A release CLI probe in an isolated project also
  generated five files, removed the two-file `Stale/` leaf after its source was
  deleted, preserved `User.cs` at SHA-256
  `468a662d7c914f1ec0984eb8cf0d7b7bc2f3aa0982166164f4ffda771372b985`,
  and returned exit code 4 after `Functions.g.cs` was edited while leaving the
  manifest and sibling hashes unchanged.
- Strict dependency-inclusive Clippy passes for both generator and CLI:
  `cargo clippy --locked -p sdkgen_csharp --all-targets -- -D warnings` and
  `cargo clippy --locked -p baml_cli --lib --tests -- -D warnings`.

## 2026-07-16 native mismatch and cancellation taxonomy

- `primitive_calls::dotnet` now forces the engine's generic Gate-A failure by
  invoking the return-only `generic_type_name<T>` through the hidden generated
  dispatch seam without a type binding. The native
  `baml.errors.TypeMismatch` envelope decodes as
  `BamlTypeMismatchException : ArgumentException` with its class name and
  message value intact.
- Missing required named arguments are classified earlier by `BexArgs` as
  `baml.errors.InvalidArgument`, not `TypeMismatch`. A non-generic `int`
  parameter supplied a string also remains permissive in the current engine
  path and fails later if generated C# expects an `Int64` result. Tests must
  not use either case as indirect evidence for the generic type-mismatch arm.
- `csharp_cancel_token::dotnet` now creates a long-running BAML spawn with an
  internal `baml.spawn.CancelToken`, cancels it inside BAML, and awaits it
  without any caller `CancellationToken`. The resulting native
  `baml.panics.Cancelled` value decodes as tokenless
  `BamlCancelledException : OperationCanceledException`; this distinguishes
  engine cancellation from the caller-associated canceled-task path.
- Focused nextest runs pass both fixtures. These close the ledger's two
  managed-only runtime-error mappings.

## 2026-07-16 bounded bytecode carrier

- Generation emits one centralized `BamlGeneratedProgram.g.cs`, splits base64
  into 12,000-character constants, and rejects raw bytecode above 8 MiB. The
  runtime independently enforces the same decoded limit and decodes segments
  directly into one preallocated byte array without `String.Concat` or a
  second full-size encoded string.
- The representative fixture carries 633,774 raw bytes as 845,032 base64
  characters. Its program source is 849,548 bytes and its Release consumer
  assembly is 3,433,472 bytes. An exact 8 MiB carrier also compiles: 933
  constants, 11,237,286 source bytes, 3.50 seconds of build time, and 216,676
  KiB peak RSS.
- Runtime tests reject missing, malformed, internally padded, oversized, and
  fingerprint-corrupt carriers with stable `BamlBridgeException` diagnostics.
  Generator tests reject 8 MiB plus one byte before output is written.
- Project-reference consumers pass under nextest. A clean local-feed consumer
  compiled the same generated source against the normalized package and ran
  sync and async calls through its Linux native asset. A non-trimmed
  framework-dependent single-file publish ran successfully and was
  byte-identical across two builds: 4,848,261 bytes, SHA-256
  `352a2098ded6915a8a2b597e1d9cabfaba6bb5682100d63141d4e4bb9eda826f`.
- Embedded resource and binary/content carriers were rejected for the selected
  source-in-project model because they require generator-owned MSBuild
  integration. Trimming and NativeAOT remain explicit v1 non-goals. Full
  commands and measurements are in `TASK/codex/bytecode-carrier-probe.md`.

## 2026-07-16 deterministic enum and union binary contracts

- Enum discriminants now hash an exact tagged typed identity under
  `baml-csharp-enum-discriminant-v1`: tags 0/1/3/4/5 carry length-prefixed
  domain/package/namespace/symbol/member bytes and tag 2 carries the u32
  big-endian namespace count. SHA-256's first eight big-endian bytes are
  sign-masked and zero is rejected.
- Golden values are
  `user.sentiment.Label::positive = 3_684_794_946_289_716_079` and
  `negative = 4_780_179_345_900_423_946`. An injected digest collision fails
  generation with both identities rather than probing or renumbering.
- `tools/Baml.Union.LayoutProbe` compares typed fields with an object payload
  at arities 2, 8, 16, and 32 and is included in `Baml.Bridge.slnx`. It checks
  duplicate closed arm types and invalid default behavior.
- The object payload stays 16 bytes and copies faster, but boxes value arms:
  24 bytes per `long`/enum construction and 32 bytes per `BigInteger`
  construction. Typed fields allocate zero for construction and matching in
  every measured scenario, selecting the current one-field-per-arm v1 binary
  layout. Full results are in `TASK/codex/union-layout-probe.md`.

## Explicit v1 limits

- Generator emits targeted `NotSupportedException` bodies for unsupported
  callable shapes.
  Structurally ambiguous erased recursive aliases are rejected. Resource-
  specific wrappers beyond prompts, BAML/SSE streams, files, HTTP responses,
  globs, runtime cancel tokens, task groups, and CSV readers/writers/records are
  explicit typed v1 non-goals. An opaque cloneable/disposable `BamlHandle`
  exists for other `$rust_type` values.
- `BamlSdk` and the application-owned source-file artifact are the fixed v1
  generated identity. Generated code uses the public `BamlProgram.Call*`
  tuple/object seam; advanced callers may use it dynamically, but the generated
  surface remains the canonical typed API.
- Typed container, nominal, nullable-generic, and union reconstruction uses CLR
  generic-type inspection, cached reflection, and `Activator.CreateInstance`.
  This is the selected non-trimmed v1 model; trimming and NativeAOT are explicit
  non-goals.
- `BamlUnion<T0,...,TN>` is mechanically generated with a one-field-per-arm
  layout for arities 2 through 32. Source regeneration is deterministic and
  managed behavior is pinned at arities 2, 3, 16, and 32; the measured layout
  contract is frozen above.
- Hard exit flushes the native event sink and calls `Environment.Exit`.
  Isolated child-process tests prove exact exit codes and harness containment.
- The shared harness currently opts in `primitive_calls`, `function_calls`,
  `llm_functions`, and the C#-specific `csharp_resources`,
  `csharp_llm_clients`, `csharp_glob`, `csharp_cancel_token`, and
  `csharp_task_group` and `csharp_csv` fixtures; other shared fixtures remain
  excluded until their required generated types/codecs exist.
