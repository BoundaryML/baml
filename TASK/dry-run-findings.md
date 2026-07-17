# Prior C# bridge experiment: evidence and lessons

## Authority and provenance

This document distills the useful evidence produced by the earlier experimental
C# bridge run. It is not an alternate design.

- Normative target: `design.md` in this folder.
- Experiment: BoundaryML/baml PR #4074, branch reported as `paulo/csharp`.
- Dry-run checkout reported by its notes:
  `408b2be28afbf9005e7b50d1f5bd4621036ab1c9`.
- Observation window: 2026-07-15 through 2026-07-17.
- Main reported .NET environment for the union probe: SDK 10.0.109,
  runtime 10.0.9, Linux x64, AMD EPYC-Milan.

The results below were read from the dry-run task documents and logs. They were
not independently rerun while preparing this seed. A new implementation may
reuse test ideas and historical measurements, but it must revalidate them
against its recorded target SHA before promoting a capability to `supported`.

When this document and the completed design disagree, the completed design
wins. If current compiled evidence disproves the design, amend the design
explicitly; do not silently adopt the experiment's behavior.

## Evidence worth preserving

| Area | Dry-run observation | What it supports | What remains before the new run may rely on it |
| --- | --- | --- | --- |
| Union storage | A maintained .NET 10 probe compared one typed field per arm with `object` payload plus tag at arities 2, 8, 16, and 32. Typed fields allocated zero during construction/matching; payload/tag boxed `long` and enum at 24 B and `BigInteger` at 32 B. Duplicate closed types and invalid default were tested. | The canonical Q8 choice of one-field-per-arm storage is evidence-backed, not aesthetic guesswork. | Port/rerun the probe from the final implementation, retain exact source/command/output, and verify canonical duplicate-arm semantics. |
| Protobuf tool pair | `Grpc.Tools 2.82.0`, bundled `libprotoc 35.0`, and `Google.Protobuf 3.35.1` built on Linux x64. Two clean generations were byte-identical, incremental invalidation was selective, transport types were internal, and a clean consumer did not receive `Grpc.Tools` or schemas. | Q9's internal-generation model and candidate frozen pair. | Complete required build-host matrix, representative envelope vectors, package/version-skew coverage, and freeze the supported runtime range in the canonical evidence record. |
| Raw NuGet nondeterminism | Two `dotnet pack --no-build` outputs differed because NuGet generated random OPC core-properties names and relationship IDs. A pre-signing normalizer produced byte-identical packages. | Exact package provenance cannot assume raw `dotnet pack` is reproducible. | Define the release order for pack, normalize, inspect, sign if applicable, verify, and publish. Never normalize a signed package. |
| Linux native package | A clean isolated consumer restored a normalized one-RID package, compiled generated source with nullable warnings as errors, ran sync/async calls with the development override unset, and published exactly the Linux native asset. | Normal NuGet RID resolution and source-in-project consumption are viable. | Redo with canonical `[LibraryImport]`, canonical generated API, exact current package, and all supported deployment modes. |
| Eight-slot package mechanics | A synthetic package copied one Linux binary into all eight named RID slots. It was deterministic, had the expected paths, and its unsupported-RID target rejected `freebsd-x64`. | The atomic package assembler and diagnostics can be exercised before all native artifacts exist. | This proves no architecture, dependency, loadability, aggregate real size, or cross-RID execution. Real binaries/runners remain mandatory. |
| Naming | Generator tests reportedly covered 100 shuffled declaration orders, `Functions` reservation, normalization collisions, case-insensitive routes, Windows device names, and injected full-hash collisions. | The typed allocation and route invariants in the completed design are practical and should be tested early. | Recreate against compiler-owned names and canonical C# identity types; do not copy free-form identity strings. |
| Callback failures | The experiment copied borrowed bytes before returning, completed work off the unmanaged callback thread, flowed `ExecutionContext`, avoided `SynchronizationContext`, and rethrew the exact managed exception via `ExceptionDispatchInfo`. | Q16-Q17's callback containment, ambient context, and exception-identity contracts. | Rebuild on the actual direct-export ABI and canonical Task-only delegate surface. |
| Cancellation/exit fixtures | Pre-cancel, in-flight cancel, concurrent calls, late completion, recovery, and hard exits 0/23 in child processes were reported. Missing generic binding reliably produced `baml.errors.TypeMismatch`; missing required arguments produced `InvalidArgument`. | Useful deterministic stimuli and fixture topology for Q16. | Verify the canonical `BamlOperationCanceledException` token/origin/task-state behavior and exact current wire classes. |
| Pull-style stream operations | The experiment invoked ordinary BAML stream `next` and `final` operations on a native stream handle instead of accepting unbounded pushed partials. | This may provide the bounded backpressure mechanism required by Q17 while retaining the canonical cold `IAsyncEnumerable` public API. | Prove the current ABI supports one-demand/one-partial pull, cancellation, terminal result, and cleanup. Do not assume the older wrapper's sync `Next`/`Final` API. |
| Generated output safety | The experiment exercised deterministic manifests, stale removal, collision/path validation, failed-write rollback, and concurrent-writer protection. | Atomic, deterministic regeneration deserves first-class tests. | Implement the completed design's wholly owned `baml_client/` directory transaction. Do not import mixed ownership, legacy adoption, or preservation of handwritten files inside it. |
| Untrimmed single-file | The experimental package and Base64 carrier ran in one untrimmed framework-dependent single-file configuration. | Single-file native resolution is feasible in principle. | It does not prove the canonical hex `byte[]`, sidecar and self-extraction variants, trimming, or canonical resolver behavior. |

### Historical union layout measurements

The experimental command was:

```bash
dotnet run \
  --project tools/Baml.Union.LayoutProbe \
  --configuration Release
```

The decision-driving results were:

| Arity/payload | Typed fields size | Payload/tag size | Typed copy ns/op | Payload/tag copy ns/op | Typed construct B/op | Payload/tag construct B/op |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 2 / `long` | 24 B | 16 B | 5.67 | 1.55 | 0 | 24 |
| 2 / `BigInteger` | 40 B | 16 B | 10.94 | 1.43 | 0 | 32 |
| 8 / `long` | 72 B | 16 B | 5.61 | 1.40 | 0 | 24 |
| 8 / `BigInteger` | 136 B | 16 B | 15.95 | 1.48 | 0 | 32 |
| 16 / `long` | 136 B | 16 B | 11.89 | 1.45 | 0 | 24 |
| 16 / `BigInteger` | 264 B | 16 B | 19.77 | 1.39 | 0 | 32 |
| 32 / `long` | 264 B | 16 B | 15.36 | 1.40 | 0 | 24 |
| 32 / `BigInteger` | 520 B | 16 B | 27.28 | 1.50 | 0 | 32 |

Reference, enum, generated-class-shaped, and mixed closures were also measured.
The typed layout traded larger structs and slower copying for zero per-value
boxing allocations. Keep the full new-run output as the actual binary-contract
evidence; these numbers are historical comparison points.

### Historical package/protocol measurements

These values are useful baselines, not current release artifacts:

| Artifact | Reported size | Reported SHA-256 / scope |
| --- | ---: | --- |
| Real stripped Linux x64 `libbridge_cffi.so` | 19,729,160 B | `4fd82a5d676728c74424d19d92dbc90be97d44136a28f390c9b39cb98d822d31` |
| Experimental `Baml.Bridge.dll` | 723,968 B | `d5dffd5030f25baee8336658e1a378ffe8fd2c70f6c0609c3a8c70d2bc273170` |
| Normalized one-RID package | 7,579,481 B | `6a25b5624af50a1899bfca727f97a98fde560ba9d296b100b3d5b3402a92c67e` |
| Synthetic eight-slot `.nupkg` | 59,346,002 B | `ceac31082de515fdf95dca743113ae86483814a587b1dbc7a6a967f66c789791` |
| Synthetic eight-slot `.snupkg` | 128,309 B | `9437c8b22941e1976c6de190ed760085b904ca3e8662f452a9b3755869efd043` |

The synthetic package duplicated Linux bytes under other RID names. It must
never be cited as a real multi-platform package.

The dry-run protocol-source hashes were:

```text
3469310d8103bd55a7d7747f970ce1eb053316d2e2eb63b32681100afcfb3188  BamlHandle.g.cs
6e948f31a1a0f1def4b1035073db632d2a810b77dde5df0a9eff2d4edcb08fa4  BamlInbound.g.cs
11af1adb6d02b8b1137d2d66c1c580069a0b5eb632ee7a4b11caa42ae78a9988  BamlOutbound.g.cs
1692c078e763ce7a1ae3c8eec48469bd97326c39337ba74e960f334feca3952b  BamlType.g.cs
```

Only freshly generated current-checkout digests may become canonical.

## Correctness footguns exposed by the experiment

### Native buffer presence and ownership

The native ABI—not pointer intuition—defines both ownership and optional
presence:

- If an owned native buffer contract requires its free function even for
  `{ptr = null, len = 0}`, call it exactly once. Do not guard release only on a
  non-null pointer.
- If an optional value uses `len == 0` as absence, do not substitute
  `ptr == null`. An empty Rust-owned slice may have a non-null pointer.
- Copy borrowed callback memory before returning to native code.
- Copy owned output before freeing it, and free it on decode/type/cancellation
  failure as well as success.

The actual-ABI probe must pin these rules to the current header/exports and add
allocation counters or another objective exactly-once oracle.

### Callback IDs are not native call IDs

The experiment found distinct widths and purposes for callback correlation and
native function-call cancellation IDs. Never reuse one as the other or derive
one by truncation. The canonical source-generated getter, typed API table, and
managed registries must use the actual ABI types and independently test
wrap/exhaustion.

### Literal-union metadata may contradict the payload — resolved

A dry-run CSV case reportedly returned payload `"crlf"` while
`selected_option` identified literal arm `"lf"`. The experiment worked around
this by choosing from the payload. That workaround is forbidden by the
completed design: union metadata and payload must agree, and contradiction is a
protocol/type failure.

The new run reproduced and fixed exact arm selection in the shared producer,
recorded the Rust CFFI `"crlf"` envelope, and proved that the pinned C#
Protobuf decoder rejects selected `"lf"` with payload `"crlf"`. A7 records
the exact evidence. The permanent rule remains: no first-compatible-arm
guessing in C#.

### Do not use indirect stimuli for type-mismatch tests

The dry run reported:

- missing required arguments were classified as `baml.errors.InvalidArgument`;
- supplying a string where an `int` was expected was permissive along one
  current path and did not reliably trigger the desired boundary error;
- omitting a required generic binding reliably reached
  `baml.errors.TypeMismatch`.

Tests must trigger the exact outcome they claim to cover and assert the wire
class/FQN, not merely any thrown managed exception.

### Shared fixture projects need isolated build output

Two project references writing the same `Baml.Bridge` MSBuild output forced the
dry run to serialize a fixture group. Prefer isolated intermediate/output paths
per consumer so parallel test execution remains safe and does not hide race
bugs behind global serialization.

### Provider defaults belong below the managed bridge

The experiment confirmed that per-call typed client overrides can be encoded
without reconstructing provider/environment defaults in C#. Preserve the
runtime as the owner of declared defaults. Managed convenience constructors
must not create a second provider-default policy.

## Current-Canary integration findings to revalidate

The dry-run week review compared its branch with a 2026-07-17 Canary snapshot.
These are drift-sensitive audit leads, not confirmed current facts:

### Compiler-owned codegen types and names

The review reported that Canary had replaced a parallel generator type algebra
with compiler-owned `CodegenTy` and qualified-name APIs. A semantic rebase
therefore needs to:

- consume canonical compiler types rather than reintroducing a C#-local mirror;
- classify function, void, never, interface, enum-variant, future, type,
  resource, prompt-AST, literal, alias, and Rust-backed variants explicitly;
- avoid wildcard match arms so a newly added compiler type forces a C# policy
  decision;
- use current qualified-name accessors for package/namespace/symbol/stream
  identity rather than reading obsolete fields;
- keep exact canonical wire identity independent of projected names;
- let the compiler own cross-language normalization/alias validity while the
  C# layer performs only CLR-specific lowering.

Verify the actual current API before writing adapters; the names above may have
continued to evolve after the dry-run snapshot.

### Literal unions and map keys are different lowering problems

Ordinary unions must retain duplicate CLR projections and explicit case
identity. A map key has a narrower canonical contract. If the compiler accepts
a string literal or alias/literal union as a map key, C# should expose the key
as `string` and validate its exact literal/descriptor rather than emit
`IReadOnlyDictionary<BamlUnion<string,string>,...>`.

Regression coverage should include string-literal unions, aliased string keys,
enum keys, duplicate canonical wire keys, invalid numeric/bool/object keys, and
literal values whose metadata contradicts their payload.

### ABI/header authority

The week review reported a generated public C header with layout and calling
convention checks. The final C# design selects one source-generated
`baml_get_api_v1` getter import followed by exact typed calls through the
validated API table; those managed declarations must be tested against the
current authoritative native contract:

- fixed-width integer and enum widths;
- struct size/alignment/field offsets;
- buffer ownership and optional-value presence;
- callback calling conventions and lifetimes;
- the exact required export list.

Do not append an experimental table field such as `flush_events` or hand-copy a
stale ABI inventory merely because the previous branch did so.

### Platform and release contract

The review reported a repository-owned platform contract and warned that the
experiment duplicated the RID list across scripts, project files, targets, and
runtime code. The new implementation must derive or validate one C# mapping
from the current repository contract and decide explicitly whether upstream
“experimental” targets are claimed/release-blocking for NuGet.

The release flow should:

- build immutable native inputs once;
- assemble and normalize the one NuGet package once;
- run clean package consumers on claimed hosts/RIDs;
- publish those exact verified bytes from a separate non-compiling trusted job;
- make release completion depend on NuGet publication;
- record package/version/digest in release metadata;
- run a public-registry post-publish smoke.

### Shared parity accounting

The dry branch opted into only fixture directories with C# overlays. That
provided strong bespoke coverage but obscured missing shared capabilities. The
new harness should reuse shared test identities/source wherever possible or
enforce an explicit Now/Later ledger. C#-specific resource, ABI, layout,
package, trim, and hard-exit probes remain additional; they do not replace
shared parity.

## Reconciliations the new run must resolve before depending on them

These are written down here so they do not survive only as chat context. They
are not permission to override the completed design.

### 1. Canonical API-table feasibility against the actual ABI — resolved

The experiment reported that its native library exported
`baml_get_api_v1` and that the table exposed operations not present in an older
individual-export list. The current-Canary audit proved that the public header
guarantees only that getter and that required `register_bridge` is available
only in the table. Q1 is now explicitly amended to import
`baml_get_api_v1` through source-generated `[LibraryImport]`, validate the
typed append-only table, and call its exact unmanaged function pointers.

The repository-owned .NET 10 probe and current artifact evidence are recorded
under A2 in `verification-gates.md`. PR #4074's manual
`NativeLibrary.GetExport` loader remains rejected; the validated table
contract, not that loader implementation, was confirmed.

### 2. Cross-assembly generated-code registration — resolved

Generated `.g.cs` compiles into an arbitrary consumer assembly. It cannot name
or implement an `internal` codec/registration contract from `Baml.Bridge`, and
`InternalsVisibleTo` cannot name every consumer assembly. The experiment used
reflection as an escape hatch; that conflicts with supported trimming.

The exact-package probe now proves the versioned
`Baml.Generated.V1` public-but-editor-hidden contract from an arbitrary
consumer assembly. Generated static code supplies field-by-field codecs and
explicit typed registration; the runtime never scans arbitrary members. A3
records the package digest and compile/run evidence. Do not fall back to
`Activator`, member reflection, or `InternalsVisibleTo`.

### 3. Recursive aliases are not representable by simple CLR erasure — resolved

The completed design says aliases project to their underlying CLR type. A
recursive alias such as conceptually `int | RecursiveNumbers[]` has no finite
C# source alias or closed nested generic spelling. The experiment introduced a
nominal wrapper, which contradicts ordinary alias erasure.

The current compiler probe now covers direct, mutual, collection, nullable,
and union recursion as finite named codegen graphs. Q18 explicitly chooses a
targeted C# v1 generator diagnostic for every recursive alias SCC before output
replacement. The generator must not recurse indefinitely, emit `BamlValue`,
or import the dry-run wrapper; the exact decision/evidence is recorded under
A4 in `verification-gates.md`.

### 4. Optional parameters inside host callables — resolved

Plain `Func` parameter names are not BAML wire identity, and BAML may invoke a
callback with only a later optional named argument. The dry run generated
custom delegate types and wire-name attributes, conflicting with the canonical
Task-only `Func` family.

The proved canonical form is:

```text
Func<Required1,
     BamlOptional<Optional1>,
     BamlOptional<Optional2>,
     CancellationToken,
     Task<TResult>>
```

The generator-registered callback descriptor—not reflected CLR parameter
names—maps exact BAML wire names into declaration-order slots and fills omitted
slots with `BamlOptional<T>.Unset`. All-unset, first-only, later-only,
explicit-null, and all-set actual-wire cases pass in the A5 C# probe. Never
trust lambda implementation parameter names.

### 5. Standard-library resource coverage needs an explicit ledger — resolved

The experiment created typed wrappers for prompts, files, HTTP responses, SSE,
globs, BAML cancel tokens, task groups, CSV resources, and LLM clients. The
completed public inventory deliberately does not authorize those dry-run types:
opaque protocol resources use `BamlHandle`, while media and
`BamlHttpRequest` have separate value contracts.

The completeness ledger now classifies every current rust-backed
standard-library class plus current resource/client/prompt helpers as a
canonical structural/immutable value, opaque `BamlHandle`, ordinary nominal
value, internal identity, or explicit unsupported v1 shape. A6 records the
exhaustive-source evidence. There is no public `BamlFile`, `BamlPromptAst`, or
similar experimental wrapper.

### 6. Pull streams satisfy canonical bounded backpressure — resolved

The B8 actual-table fixture validates the pull-style native
`Stream.next`/`Stream.final` path: a cold controller starts once, issues one
native next operation per demand, preserves 20 ordered partials with zero
unsolicited idle completions, requires each partial to strictly extend the
previous partial and the final to extend the twentieth, and caches/drains the final operation. The
replay server/runtime and consumer use separate processes so the endpoint is
present before consumer runtime initialization. No pushed callback queue or
new acknowledgment field is required.

### 7. Validate the exact BAML integer domain — resolved

Current compiler/runtime authority confirms `[-2^62, 2^62 - 1]` while the
public CLR/protobuf carrier is `long`/`int64`. The pinned C# probe checks both
directions and scalar/literal/container paths, including `long` extremes; A8
records the vectors. Not every `long` is a valid BAML int.

### 8. The canonical hex carrier needs canonical tests

The experiment used Base64 and invented an 8 MiB ceiling. Both are rejected by
Q20. Its measurements only show that large generated source affects compile
resources. Test realistic and boundary-sized canonical hexadecimal arrays for
correct generation, compilation, publish, and integrity, but do not reopen the
chosen representation or inherit the 8 MiB limit without an explicit design
amendment.

### 9. Normalization and signing order

If package signing is retained, define and test an order equivalent to:

```text
pack -> normalize -> inspect -> sign -> verify signed artifact -> publish
```

The exact bytes installed by clean consumers must be the bytes ultimately
published. Do not repack or renormalize after consumer verification, and do not
claim a pre-signing digest is the final registry digest.

## Do not copy these experimental decisions

| Dry-run choice | Completed canonical requirement |
| --- | --- |
| Dynamic `baml_get_api_v1` table loaded through manual `NativeLibrary.GetExport` | One source-generated `[LibraryImport]` for `baml_get_api_v1`, one assembly-owned resolver, and exact typed function pointers from the validated canonical table |
| `BAML_BRIDGE_LIBRARY` and source/Cargo tree probing | Normal .NET/NuGet production probing; one fail-closed absolute maintainer/test override before first use |
| Cached reflection, arbitrary member discovery, and `Activator.CreateInstance` | Generated field-by-field codecs/factories and explicit static registration compatible with trimming |
| Trimming and NativeAOT both unsupported | Trimmed JIT and trimmed single-file supported; NativeAOT alone rejected with `BAML0019` |
| `BamlSdk` root and `baml_sdk/` output | Canonical allocated namespaces and wholly owned `baml_client/` output |
| Mixed per-file ownership, adoption, and preservation inside generated output | Whole generator-owned directory staged and replaced atomically; user code lives outside |
| Base64 segments, `RegisterEncodedProgram`, and an 8 MiB limit | One private hexadecimal `byte[]`, SHA-256 fingerprint, no invented ceiling without an amendment |
| `List<T>`, `Dictionary<string,V>`, and public `byte[]` | `IReadOnlyList<T>`, `IReadOnlyDictionary<TKey,TValue>`, and `ReadOnlyMemory<byte>` |
| `unknown` or thrown dynamic values as `object?`/`Dictionary<string,object?>` | Immutable typed `BamlValue` plus `BamlTypeDescriptor` |
| Reflection-decoded classes and generic instances | Generated trim-safe codecs/factories |
| Nominal recursive-alias wrappers as an assumed general policy | Ordinary aliases erase; recursive representability is an explicit reconciliation above |
| Collapse duplicate CLR union arms | Preserve typed BAML arms and explicit active case even when CLR projections match |
| Handle-backed disposable media | Immutable non-disposable URL-or-owned-bytes media values |
| `BamlError`, `BamlPanic`, `BamlCancelledException`, and type mismatch as `ArgumentException` | Resolved exception-suffixed hierarchy and `BamlOperationCanceledException` |
| Tokens only on async calls | One final `CancellationToken` on both ordinary sync and async call forms |
| `Action`, sync `Func`, `ValueTask`, and generated callback overload families | One Task-returning `Func<...,CancellationToken,...>` family with a 15-BAML-parameter limit |
| Public sync `Next`/`Final` stream methods and terminal-sentinel union | One cold async-enumerable controller with separately typed cached final result |
| Typed wrapper for every encountered standard-library resource | Only types authorized by the canonical public inventory; otherwise `BamlHandle` or explicit unsupported |
| Hard-coded language ID/version and appended API-table field | Current frozen release identity and actual canonical ABI only |

## Reusable fixture blueprints

The dry run's feature claims are not inherited, but these fixture ideas are
valuable:

- primitive calls: sync/async, required/defaulted/nullable/generic values,
  cancellation races, errors, panics, hard-exit children;
- function calls: callback arity, generated nominal arguments, async completion,
  `ExecutionContext`, typed BAML throws, exact managed exception identity,
  cancellation and recovery;
- LLM functions: credential-free replay streams, partial/final class decoding,
  build-request and prompt shapes;
- resource fixtures: file/HTTP, glob, BAML cancel token, task group, and CSV,
  reclassified through the canonical resource ledger before use;
- LLM client fixture: typed per-call override and no managed provider-default
  injection;
- generator fixtures: semantic compiler-type exhaustiveness, typed names,
  deterministic ordering, case-insensitive routes, Windows-safe paths, atomic
  output transaction;
- package fixtures: isolated local feed/cache, project-reference versus exact
  package-reference consumers, RID publish inspection, unsupported RID, trim,
  single-file sidecar/self-extraction, and negative NativeAOT.

Prefer shared Python capability/test identities when they exist. Keep
C#-specific ABI, layout, packaging, path, and hard-exit probes separate rather
than renaming them into apparent cross-language parity.

## Evidence the dry run lacked (historical checklist)

This list records what the dry run had not yet established. It is not the
current gate ledger; `verification-gates.md` is authoritative for current
status, evidence, and blockers.

- Clean-package/default-resolution and complete API-table ownership/race
  probe; the narrow source-generated getter/table feasibility probe is
  complete and recorded under A2.
- A resolved trim-safe cross-assembly codec registration seam.
- Explicit recursive-alias and optional-callback behavior.
- Current compiler-owned type/qualified-name integration audit.
- Reproduction/correction of contradictory literal-union metadata.
- Exact BAML integer bounds.
- Canonical hex-array generated-source fixtures.
- Full Protobuf build-host/version-skew matrix.
- Eight real native binaries, binary/dependency/export inspection, and
  execution on every claimed RID.
- Final package normalization/signing/provenance order.
- Immutable URL/owned-byte media restoration on the actual protocol.
- Committed-source exact-package/trim reproduction of the locally passing cold
  pull-stream bounded-backpressure fixture.
- Warning-free executed trimmed JIT and trimmed single-file consumers.
- Both single-file native sidecar and native self-extraction.
- Reflection-only consumer rooting boundary.
- Targeted `BAML0019` NativeAOT rejection.
- Post-publish clean consumer using the exact NuGet.org artifact.
- Executable canonical C# user documentation.
