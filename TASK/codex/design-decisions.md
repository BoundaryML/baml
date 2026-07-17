# C# bridge design decisions

## Question 1: consume the versioned API table

Resolution: load the native library with `NativeLibrary`, resolve only
`baml_get_api_v1`, validate `abi_version` and `struct_size`, then invoke typed
unmanaged function pointers from that table.

This supersedes the design's static-per-symbol `LibraryImport` recommendation.
The target checkout now explicitly describes `baml_get_api_v1` as the sole
symbol a manually loaded bridge needs to resolve, and its table includes
capabilities missing from the older Go symbol list: native call-ID allocation,
bridge/version registration, media operations, and the bytecode initializer.
Binding individual legacy exports would duplicate the ABI inventory and could
silently miss appended production capabilities.

Invariants exercised by the implementation:

- One native library handle is retained for process lifetime.
- The full consumed v1 table layout is represented, so a truncated table is
  rejected.
- Static `[UnmanagedCallersOnly]` callbacks are rooted by code lifetime and do
  not let exceptions unwind through C.
- Callback correlation IDs and runtime cancellation IDs are distinct because
  the actual ABI gives them different widths and purposes.
- Package probing tries the normal `bridge_cffi` name first. Explicit
  `BAML_BRIDGE_LIBRARY` and source-tree probing are development paths.

The compiled bridge and native fixtures pass bytecode initialization, sync/async callback
completion, borrowed-buffer copying, cancellation races, host dispatch and
release callbacks, handle clone/release, and all four media
constructors/accessors. Managed tests pin version, table-size, missing-pointer,
null-pointer/length, duplicate/late-completion, and callback exception behavior.
Cross-RID execution remains a release-matrix obligation, not an open interop
design question.

## Question 9: frozen protobuf tool/runtime pair

Use `Grpc.Tools 2.82.0` with `Google.Protobuf 3.35.1`. Clean and
incremental deterministic generation, direct/imported schema invalidation,
managed compilation, package inspection, envelope round trips, and a clean
consumer now pass. The bridge project requests internal generated types and
does not expose `Grpc.Tools` transitively. The complete supported build-host
matrix remains a release gate.

## Question 10: one atomic native-bearing package

The implemented release topology is the resolved one-package design. One
`baml-bridge` `.nupkg` contains `Baml.Bridge.dll` plus exactly one native asset
under each of the eight standard RID paths. The release assembler consumes the
shared `bridge_cffi` artifacts and verifies their sidecar digests; it does not
rebuild, rewrite, or independently version native inputs.

Both primary and symbol packages are normalized deterministically before they
become release artifacts. Explicit unsupported `RuntimeIdentifier` and
`RuntimeIdentifiers` values fail through a bounded `buildTransitive` target,
while ordinary RID asset selection remains owned by NuGet/.NET. The frozen
release version stamps the package, managed handshake metadata, generated-code
marker, and native version from the same plan.

Runtime diagnostics normalize .NET's distro-specific reported identifiers to
the portable packaged contract by OS, architecture, and libc. This accepts
values such as `ubuntu.26.04-x64` as `linux-x64` without weakening the supported
set; unsupported OSes/architectures and Android/Bionic fail before native
initialization with `PlatformNotSupportedException`.

A synthetic eight-slot assembly probe is byte-for-byte reproducible and below
the design's 200,000,000-byte safety ceiling, but it deliberately duplicates
the current Linux x64 library to exercise packaging mechanics. It is not the
required real architecture/host feasibility proof. The release matrix must
still supply, inspect, and execute the eight correctly labeled binaries before
publishing can be enabled. NuGet organization ownership and trusted-publisher
configuration remain external release prerequisites.

## Question 14: application-owned source artifact and regeneration

`baml generate` writes normal C# source beneath the configured output,
conventionally `baml_sdk/`:

```text
baml_sdk/
  BamlGeneratedProgram.g.cs
  [NamespaceSegments/]
    Functions.g.cs
```

The fixed namespace root is `BamlSdk`; BAML namespace segments follow it
and use idiomatic PascalCase. Free functions live in the resolved `Functions`
holder. The consuming project compiles these sources directly and references
the exact same canonical version of `baml-bridge` as the generating CLI. The
generated version marker is checked before native initialization.

Within one generated leaf, emitted classes, enums, and recursive aliases now
use a shared deterministic allocator. It reserves the synthetic `Functions`
holder, suffixes normalization collisions by typed identity, and routes every
translated reference through the allocation. Windows device-name namespace
segments are rewritten only in filesystem paths. Namespace identities that
normalize alike receive distinct projected namespaces and case-insensitive
paths. One hundred declaration-order permutations produce byte-identical
output; contextual keywords and injected full-hash collisions are pinned.

The C# regeneration ownership contract is now concrete:

- `.baml-generated-files.json` schema 1 contains the generator identity and a
  sorted list of normalized relative paths with lowercase SHA-256 digests.
- Only manifest-owned files whose current digest matches may be overwritten or
  deleted. Unrelated files are preserved. A pre-manifest C# file may be adopted
  only when it starts with BAML's generated banner.
- All prospective paths are validated before staging. Absolute, parent,
  non-normalized, non-UTF-8, control-character, Windows-invalid/device, reserved,
  symlink-traversing, and case-insensitively ambiguous routes fail without
  modifying output.
- A generation stages the complete next file set inside the canonical output
  directory, backs up every affected file, installs staged files, and commits
  the manifest last. Returned transaction failures restore the prior files and
  manifest. Stale owned files and their empty generated-only directories are
  removed after a successful commit.
- An exclusive `.baml-generation-lock` prevents concurrent writers. A hard
  kill intentionally leaves the lock and possibly `.baml-generation-*` state;
  later runs fail closed for operator inspection instead of inferring recovery.
- Generator blocks must resolve to distinct canonical output directories. The
  manifest writer is enabled for C# only; applying it to existing Python or
  TypeScript output requires a separate safe migration for bannerless artifacts.

Nine filesystem unit tests and two real CLI integration tests cover stable
manifest bytes, repeat generation, stale deletion, unrelated-file
preservation, modified-file refusal, corrupt/wrong-owner manifests, legacy
adoption, unsafe/colliding paths, symlinks, locks/interrupted state, and
duplicate output directories.

The source-in-project choice and `BamlSdk` root are the v1 compatibility
contract. The generator emits no project, assembly, MSBuild resource target,
or official program-specific package. The consuming assembly owns the compiled
source identity; an application may package it only under an application-owned
identity.

The container/model codec probe adds a constraint to that decision. With
source compiled directly into an arbitrary consumer assembly, generated models
cannot implement a runtime-`internal` codec interface: C# accessibility stops
the consumer assembly from naming it. The viable choices are:

- generated call sites fully encode/decode through public `BamlProgram`
  operations without a runtime-owned generated-code interface;
- a public, hidden-but-compatible codec/registration contract between the
  consumer and `Baml.Bridge`;
- a generated project with a fixed assembly identity that receives friend
  access to runtime internals; or
- controlled reflection over generated wire attributes.

V1 selects the last option deliberately. Cached reflection covers generated
nominal member discovery, closed generic classes, recursive aliases, and union
factories and validates exact BAML FQNs, fields, generic type arguments, enum
wire names, and union metadata before construction. Question 19 makes trimming
and NativeAOT explicit non-goals, so this is the final non-trimmed v1 codec
architecture rather than temporary trim-safe work.

## Question 7: stable native-enum discriminants

Generated enums are `enum : long` with explicit positive nonzero values. The
wire continues to use exact enum and variant names; CLR numbers never become
wire identities.

The SHA-256 input is byte-stable. Each ordinary component is tag byte, u32
big-endian byte length, then UTF-8 bytes: tag 0 is domain
`baml-csharp-enum-discriminant-v1`, tag 1 package, tag 3 each namespace
segment, tag 4 enum symbol, and tag 5 original member. Tag 2 is followed
directly by the u32 big-endian namespace count. The first eight digest bytes
are read big-endian, the sign bit is cleared, and zero is rejected. Golden
values are:

- `user.sentiment.Label::positive = 3_684_794_946_289_716_079`
- `user.sentiment.Label::negative = 4_780_179_345_900_423_946`

Generation detects a duplicate result and reports both identities; it never
probes or renumbers. Tests pin both goldens, stable allocation independent of
declaration order, and an injected digest collision.

## Questions 8 and 15: structural unions and generic descriptors

`BamlUnion<T0,...,TN>` is mechanically generated for arities 2 through 32 with
an explicit one-based case tag and one private field per arm. The default value
has no active case. Implicit conversions are convenience only; `FromTn` is the
authoritative construction API for overlapping arms. Source regeneration is
byte-for-byte deterministic, and runtime tests cover wire metadata plus managed
arities 2, 3, 16, and 32. The maintained comparative probe selects the current
one-field-per-arm binary layout: it avoids the compact object's 24-byte
`long`/enum and 32-byte `BigInteger` boxing allocations. Exact arity 2/8/16/32
size, copy, construction, and match results are recorded in
`TASK/codex/union-layout-probe.md`.

Generated generic callables use ordinary CLR type parameters and pass an
internal `(BAML type-variable name, System.Type)` vector to `BamlProgram`.
`ProtoTypeCodec` deterministically encodes primitives, unknown, nullable,
lists, maps, unions, media, opaque handles, and generated enum/class identity.
Unsupported CLR types fail before native dispatch. Closed generic classes carry
type arguments in both inbound class values and outbound validation. Class-level
bindings precede method-level bindings, matching BAML declaration scope.

Every supported free, static, and instance callable has sync and `Async`
forms. Instance calls prepend exact wire key `self`; only async forms take a
final optional `CancellationToken`. Sync uses `GetAwaiter().GetResult()` over
the same dispatcher, and bridge awaits use `ConfigureAwait(false)`. A
return-only generic requires explicit `<T>`. Build-request, stream, prompt,
and parse-stream compiler companions use the same allocator, optional binding,
generic descriptors, and typed results. Entry points containing host callables
must use the async form; shapes outside the resolved translation limits fail
explicitly instead of making an erased native call.

Non-recursive BAML aliases flatten to the resolved underlying CLR projection.
C# source aliases are not assembly metadata and therefore cannot be the public
contract for a generated project reference. Recursive aliases use generated
nominal wrapper classes around their typed recursive `BamlUnion` value. Their
exact alias FQN is descriptor metadata, not a class identity. Because native
outbound values erase the alias/union wrapper, decoding requires exactly one
structural CLR arm to match; ambiguous shapes fail closed rather than depending
on declaration order.

The standard-library `baml.llm.Client` family is runtime-owned rather than
generated into each consumer. LLM callables use `BamlOptional<BamlClient>`;
the immutable client object recursively carries `BamlClientType`, sub-clients,
optional `BamlRetryPolicy`, and its round-robin counter. `FromShorthand` is a
convenience constructor, not a separate wire type. All three nominal identities
and every field remain canonical on the ABI. This closes typed per-call client
overrides. Generated discovery/accessors for declared client-registry entries
are an explicit v1 non-goal; callers pass declared/shorthand client values
through the typed option surface.

## Question 17: native handle ownership

Every inbound native handle is owned by a `SafeHandle`. Encoding clones a
temporary wire reference and keeps it alive through native argument decoding;
result conversion transfers ownership exactly once into `BamlMedia` or
`BamlHandle`. Recursive cleanup runs for successful, error, panic, and failed
conversion paths. Public wrappers are thread-safe for clone/use versus dispose
through `DangerousAddRef`; use after disposal throws `ObjectDisposedException`.

Media has concrete `BamlImage`, `BamlAudio`, `BamlVideo`, and `BamlPdf`
wrappers with URL/file/base64 construction and MIME/source access. The opaque
`BamlHandle` is cloneable and disposable but intentionally not callable.

Required-argument host callables project to `Func`/`Action`, with
additional `ValueTask`-returning overloads on generated async BAML entry points.
The generated sync BAML entry point rejects callable arguments. Registry keys
root delegates until the native last-Arc release callback fires; encoding
failures roll back keys that native never received. Dispatch is fire-and-return,
copies borrowed bytes, restores registration-time `ExecutionContext`, and runs
off the native thread. An arbitrary managed exception is rethrown by reference
when it returns to the same process; `BamlError` with a generated value keeps
typed BAML throw identity. Cancellation tolerates late callback completion.

Optional host-callable parameters project as `BamlOptional<T>` on deterministic
generated delegate types rather than CLR optional/default parameters. This
preserves BAML omission independently of a C# default value and supports
partial named calls. A parameter-level wire-name attribute is authoritative;
lambda implementation parameter names are not. Generic optional-callable
delegates close over their containing function or method's type parameters.
Generator and native consumer tests cover all-unset, partially named, all-set,
async, and generic callback dispatch.

Streaming companions use the existing ordinary call surface rather than a
new unmanaged event callback. A companion returns the native tagged handle
`baml.llm.Stream<TPartial, TFinal>`; managed code owns it through
`BamlStream<TPartial, TFinal>`, and invokes the canonical
`baml.llm.Stream.next` and `baml.llm.Stream.final` methods with the handle as
`self`. This amends the earlier one-type `BamlStream<T>` inventory: partial and
final types can differ and both are part of the validated public signature.

The bridge serializes pulls per stream and exposes sync/async `Next` and `Final`
plus one `IAsyncEnumerable<TPartial>` enumeration. `Next` returns
`BamlUnion<TPartial, BamlStreamFinished>` so a null partial cannot be confused
with completion. Early enumerator disposal disposes the owned stream; normal
terminal enumeration keeps it usable for `Final`. Repeated final calls are
allowed, pre-canceled pulls do not consume input, and all use after stream
disposal fails with `ObjectDisposedException`. Replay-backed primitive and
generated-class fixtures exercise these rules through the native runtime.

Prompt-rendering companions own the tagged `baml.llm.PromptAst` handle through
`BamlPromptAst`. The wrapper exposes sync/async `text` and `messages` accessors,
structural `BamlPromptMessage` results, explicit cloning, and deterministic
disposal. Encoding preserves the canonical `baml.llm.PromptAst` class envelope
with its sole `_data` handle field; native inbound conversion restores the
PromptAst ADT before standard-library accessors run. A cloned wrapper remains
valid after the original is disposed, and all access through a disposed wrapper
throws `ObjectDisposedException`.

`baml.http.Request` is structural rather than handle-backed. Generated
`$build_request` companions return immutable `BamlHttpRequest` values whose
method, URL, headers, and body are copied from an exact four-field class
envelope. `baml.http.Response` and `baml.fs.File` instead own their hidden
`UNTAGGED_RUST_DATA` body/file reference through `BamlHttpResponse` and
`BamlFile`. Encoding clones that reference into the canonical class field;
decoding validates the nominal class, all public fields, the sole private
handle field, and its tag before taking ownership. Wrapper clones have
independent lifetimes but share the underlying response/file state; file
`Close` therefore affects every clone, while wrapper `Dispose` releases only
that clone's native reference.

This is the final v1 lifecycle contract. Synchronization-context capture is
deliberately avoided while `ExecutionContext` is restored; caller cancellation
controls the outer async call and is not injected into callback signatures.
Typed wrappers additionally cover SSE streams, globs, runtime cancel tokens,
task groups, and CSV readers/writers/records. Other standard-library resources
use opaque `BamlHandle` or have no typed v1 projection. Trimming/NativeAOT are
unsupported by question 19, and cross-RID execution remains a release gate.

## Question 16: cancellation, thrown values, and hard exit

Caller-token cancellation completes the generated async call as a normal
token-associated canceled `Task`, so awaiting it throws
`OperationCanceledException`/`TaskCanceledException`. The managed callback is
removed before `cancel_function_call(nativeCallId)` and late native completion
is ignored. Pre-cancel, in-flight cancel, eight concurrent calls, a 500 ms
latency bound, and post-cancel recovery pass through `sdk_test_csharp`.

Engine-originated cancellation remains an outbound panic at the wire layer and
is identified by the exact class FQN `baml.panics.Cancelled`. C# maps it to
`BamlCancelledException : OperationCanceledException`, with the decoded
`Value`, `ClassName`, and `BamlTrace` attached. It carries no cancelable token,
which distinguishes it from caller-token cancellation. All other panic classes
remain `BamlPanic`; matching only the exact FQN avoids treating user classes or
future panic variants as cancellation.

The standard-library `baml.spawn.CancelToken` is separately projected as an
owned `BamlCancelToken` resource. Clones share its one-shot runtime state and
can be passed back into BAML or composed with `CancelToken.any`; it never
supplies token identity to a managed canceled `Task`. This prevents the two
cancellation mechanisms from being silently conflated.

BAML error/panic class values use the resolved dynamic vocabulary, commonly an
ordinal `Dictionary<string, object?>` for unprojected classes.
`BamlException.Value` preserves that value, `ClassName` preserves the wire FQN,
and `BamlTrace` preserves the trace. This dynamic thrown-value contract is the
v1 API; it does not fabricate a generated class from an unexpected error type.

The exact error FQN `baml.errors.TypeMismatch` is a call-boundary contract
failure and maps to `BamlTypeMismatchException : ArgumentException`, not
`BamlError`. It preserves the same decoded value, FQN, and trace metadata and
uses the standard-library `message` field as `Exception.Message`. Other
`baml.errors.*` values remain `BamlError`.

Hard BAML exit is deliberately process-terminating rather than catchable. The
C# decoder calls the ABI-v1 `flush_events` table entry and then
`Environment.Exit(code)`. The table field is append-only so existing ABI-v1
offsets remain stable; startup rejects a table too old to contain the required
entry. Isolated consumer children pin exit codes 0 and 23.

## Question 19: reflection and NativeAOT are explicit v1 non-goals

The v1 typed generated paths are not trim- or NativeAOT-supported. Nominal,
recursive-alias, structural-union, generic, callback, and dynamic decoding use
reflection and `Activator.CreateInstance`. The bridge fails closed for shapes
it cannot reconstruct, but no trim annotations or generated factory registry
are claimed. README and completeness documentation state this limitation.
Supporting trimming or NativeAOT later requires a separately approved codec
design and compiled coverage; v1 CI intentionally does not advertise or test
those deployment modes.

## Question 20: bounded base64 source carrier

The v1 source-in-project generator emits exactly one
`BamlGeneratedProgram.g.cs`. It splits standard base64 into 12,000-character
constants and lazily calls `BamlBridge.RegisterEncodedProgram` on first use.
The generator and runtime both enforce an 8 MiB raw-bytecode ceiling. The
runtime decodes segments directly into one preallocated raw array without a
concatenated encoded-string allocation, then verifies the generated SHA-256
fingerprint before native initialization.

The representative 633,774-byte program, the compiled 8 MiB boundary, clean
project- and package-reference consumers, and a deterministic non-trimmed
single-file publish all pass. Missing/malformed/oversized/fingerprint-corrupt
carriers have targeted diagnostics. The generated-file transaction owns the
single carrier source, so it cannot be duplicated across partial leaves and
uses the same safe regeneration cleanup as other output.

Embedded-resource and binary/content alternatives were rejected for this
artifact model because they require generator-owned MSBuild integration.
Trimming and NativeAOT remain explicit bridge-level v1 non-goals, not carrier
promises. Exact measurements are in `TASK/codex/bytecode-carrier-probe.md`.
