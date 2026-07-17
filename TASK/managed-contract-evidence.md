# C# managed contract, generic, and dynamic-value evidence

This record covers compiled design gates B5, B6, and B10 against Current
Canary. It composes the new managed-contract and generic compile probes with
the earlier actual-ABI, union, generated-contract, failure, stream/media, and
program-bootstrap evidence. These are design fixtures; supported product rows
still require the final runtime/generator and shared C# parity lane. They are
included in the local provenance change; promotion beyond `passed locally`
still requires the exact reviewed/pushed source SHA and each named product or
external fixture.

## Target and probes

- Target baseline: `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0` on
  `paulo/csharp-bridge`.
- Audit host: Linux x64; .NET SDK `10.0.110`; runtime `10.0.10`; C# 14 /
  `net10.0`.
- Managed type/dynamic fixture:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ManagedContractProbe`.
- Warning-as-error generic fixture:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.GenericCompileProbe`.

Both projects target only `net10.0` and C# 14 with nullable warnings as errors.
The managed fixture also enables trim analysis and `IsTrimmable`; its Release
build completed with zero warnings/errors.

The repaired managed probe was restored and built from a fresh package and
artifacts directory, with warnings promoted to errors:

```shell
env NUGET_PACKAGES=/tmp/baml-managed-contract-nuget-20260717f \
  dotnet build \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ManagedContractProbe/Baml.Bridge.ManagedContractProbe.csproj \
  --configuration Release \
  --artifacts-path /tmp/baml-managed-contract-repair-20260717f \
  -warnaserror

dotnet \
  /tmp/baml-managed-contract-repair-20260717f/bin/Baml.Bridge.ManagedContractProbe/release/Baml.Bridge.ManagedContractProbe.dll
```

The build reported zero warnings and zero errors; execution reported:

```text
optional_nullable=orthogonal_complete
stream_state=pending_incomplete_complete
media_values=4x_url_bytes_base64_file_owned
http_request=immutable_duplicate_headers_fresh_messages
client_retry=immutable_structural_checked
handle=safe_clone_lease_dispose_identity
baml_value=kinds_14_structural_typed
descriptor_kinds=unknown_plus_14_value_shapes
descriptors=alias_literal_nominal_generic_union
dynamic_inspection=enum_class_union_public
dynamic_null=explicit_nullable_only
collections=owned_readonly_canonical_maps
generic_binder=canonical_and_fail_closed
limits=depth_collection_bytes_nodes_bigint_cycle
partial_projection=semantic_states
unsupported_clr=explicit
public_contract=audited
```

## B5: exhaustive public managed-type audit

This table records every public bridge-owned category from the normative
inventory. “Internal construction” means the public type is returned by the
bridge/generated runtime but applications cannot manufacture an invalid empty
instance. It does not mean generated source calls runtime internals; A3's
versioned public hidden seam remains the cross-assembly boundary.

| Public type/category | Default/invalid state and construction | Equality, ownership, concurrency | Wire/deployment/version evidence |
| --- | --- | --- | --- |
| `BamlOptional<T>` | Permanent zero/default/new state is `Unset`; `FromValue` and one implicit conversion create `Set(T)`, including explicit null/default values. | Structural case/value equality; owns/disposes nothing; readonly/thread-safe. | Managed probe plus B6 compile matrix; A5 proves descriptor-driven optional callback wire omission. |
| `BamlNullable<T>` / static `BamlNullable` | Permanent zero/default/new state is BAML null; `FromValue` and one implicit conversion create value unless the reference is CLR null. | Structural case/value equality; owns/disposes nothing; readonly/thread-safe. | Managed/B6 probes cover value/reference/default/composed states and redundant-wrapper rejection. |
| `BamlUnion<T0...T31>` | Default is permanently invalid; only `FromTn`/unambiguous conversion constructs a case. V1 publicly exposes only `IsTn`/`AsTn`/`Match`/`Switch`; neither the one-based storage tag nor `IsValid`/`CaseIndex` is public. | Structural active-case/value equality; readonly, no disposal. | B2 covers one-field-per-arm arities 2/8/16/32, duplicates, allocation/size/copy; the managed reflection probe proves the representative public/internal split; B6 covers generic/optional/nullable composition and inference boundaries. |
| `BamlValueKind` | Exact `int` values `Null=0` through `Handle=13`; these are payload kinds and never include `Unknown`. | Value enum; values are never renumbered/reused. | Managed probe constructs every value in exact numeric order and rejects invalid descriptor construction. |
| `BamlTypeDescriptorKind` | Exact `int` values `Unknown=0`, then `Null=1` through `Handle=14`. `Unknown` is type metadata, never a constructible `BamlValue` payload. | Value enum; values are never renumbered/reused. | Managed probe proves that empty, homogeneous, and heterogeneous dynamic lists/maps use `Unknown`, never a null sentinel or first-item inference. |
| `BamlTypeDescriptor` | Sealed, bridge/factory-created; no public arbitrary descriptor constructor. Its exact public properties are `Kind`, `Fqn`, `Arguments`, `IsNullable`, `Alias`, and textual `Literal`. It preserves nominal FQN, owned generic/collection arguments, nullability, supplied alias/literal text, and ordered union metadata. | Immutable structural equality/hash; owned read-only snapshots; thread-safe. | Fail-closed reflection asserts the exact property types/no constructor; runtime vectors cover every argument/FQN rule, alias/literal only when supplied, dynamic unknown, and contradictory union rejection. |
| `BamlValue` | Sealed; CLR null is invalid, `BamlValue.Null` is explicit BAML null. Public canonical factories plus registered typed codecs; no arbitrary object reflection. | Structural descriptor/case/payload equality; map order normalized, list order preserved, bytes copied, handles compare wrapper identity and are not owned/disposed by the value. | B10 vectors below; A7 exact union metadata and B9 media envelope compose with it. |
| `BamlStream<TPartial,TFinal>` | Sealed, generated-factory-created, cold; no public/native-handle constructor. | Reference identity; one partial consumer, cached final waiters, async disposal/token state machine. | B8's warning-free actual-table fixture executes every preimplementation lifecycle mode through one ordinary native pull per demand, including slow-consumer idle bounds, final waiters/final-only mode, wait-only cancellation, pre-cancel/release, and bounded callback state. Product lifecycle/race parity remains implementation work. |
| `BamlStreamStateKind` / `BamlStreamState<T>` | Exact `int` values are `Pending=0`, `Incomplete=1`, and `Complete=2`; zero/default is `Pending` with `default(T)` and only the decoder creates the other states. | Immutable structural state/value equality; owns nothing. | Managed probe freezes the numeric ABI and generated semantic partial shapes cover nullable final, nonnull partial, done, and with-state distinctions. |
| `BamlHttpRequest` | Sealed, internal construction after bridge validation; no public arbitrary request/default. | Reference equality; immutable copied strings/header order/body; non-disposable and thread-safe; each adapter call returns independent disposable state. | Managed probe preserves request ID/method/URL, duplicate ordered headers, content headers/type/body bytes, mutation/disposal isolation, and redacted display. |
| `BamlClientType` | Exact `long` values are `Primitive=1`, `Fallback=2`, and `RoundRobin=3`; zero/unknown is rejected at decode/construction. | Value equality. | Managed probe freezes the numeric ABI and asserts rejection. |
| `BamlRetryPolicy` | Sealed immutable bridge-created snapshot; checked nonnegative BAML-int retry/delay values and finite positive multiplier. | Structural equality/hash; no credentials/mutable execution/disposal. | Managed probe covers equality, bounds, and invalid values. |
| `BamlClient` | Sealed immutable bridge-created structural value; `FromShorthand` is the public convenience; no registry discovery. | Structural name/type/sub-client/retry/counter equality; recursively copied read-only sub-clients; thread-safe/non-disposable. | Managed probe covers shorthand, mutation isolation, recursion, equality/hash, counter bounds, and invalid enum. |
| `BamlHandle` | Sealed `IDisposable`; no public ownership-taking/raw constructor; one internal `SafeHandle` reference. `Clone` creates one independent owner. | Deliberate wrapper/reference equality; idempotent dispose, leased use/dispose race, exactly-once critical release. | B1 proves actual native clone/release; managed probe proves public SafeHandle clone/lease/dispose/race shape and no raw `IntPtr`/`SafeHandle` property. |
| `BamlImage`, `BamlAudio`, `BamlVideo`, `BamlPdf` | Sealed, no empty constructor; public URL/bytes/base64/file factories. URL may omit MIME; byte forms require it. | Structural exact-representation equality/hash; copied bytes/eager file; immutable/thread-safe/non-disposable; safe redacted display. | Managed probe covers all four construction families/ownership; B9 proves actual nominal `_data` handle restoration and failure cleanup. |
| `BamlException` hierarchy | Abstract category bases; invariant-preserving internal constructors; concrete sealing exactly as Q16 specifies. | Ordinary exception reference identity; immutable structured properties; no formatter persistence contract. | B7 compiles every leaf/catch relationship, structured/redacted diagnostics, direct sync rethrow, exact callback EDI identity, races, and child hard exit. |
| `BamlOperationCanceledException` / `BamlCancellationOrigin` | Caller, engine, or stream-disposed origin with exact winning token. | Exception reference semantics; tasks remain Canceled; final-wait cancellation remains ordinary per-wait cancellation. | B7 proves all origins/custom task status/token behavior; B8 executes stream wait-only cancellation semantics. |
| `BamlTrace` / `BamlPanicInfo` | Sealed immutable with no public uninitialized constructor. `BamlTrace` exposes only ordered rendered `Lines`; `BamlPanicInfo` exposes exactly `Value`, `IsExitPanic`, and nullable `ExitCode`. There is no public trace-frame type or invented panic/type-mismatch metadata. | Structural field equality/hash; owned snapshots; thread-safe. | B7 exact current-wire diagnostics/redaction fixture. |
| `Baml.Generated.V1.*` | Public only for cross-assembly generated code; editor-hidden, exact generated/runtime/required-bridge version checked, and opaque reference-bound program/function/argument/result-binding tokens; no raw FQN/wire-name application invocation. Default, cross-builder, duplicate, contradictory, and frozen states fail closed. | Explicit deterministic registration and stored declaration-result identity; no assembly/member scanning, reflection/`typeof`, arbitrary `Activator`, Protobuf, or friend assembly. Async cancellation remains Canceled with the exact token. | A3 freshly packs `0.0.0-a3`, then warning-free builds and executes an unrelated exact-feed consumer normally and trimmed across fixed sync/async, result-generic, optional, receiver, build-request, and stream variants. |
| Generated classes | Program-specific sealed partial shapes with required/init semantic fields. Generated codecs have opaque V1 carriers for null, bool, checked int, finite float, bigint, string, copied bytes, list, map, exact-wire enum, class, active-case union, dynamic, media, and handle. | Generated field semantics and owned carrier snapshots; no runtime-reflection codec contract. | A3 proves the cross-assembly field-by-field nominal codec seam with a nested generic `Envelope<Person>` composed from list/map/enum/union/optional shapes; managed/B6 probes cover wider nominal/generic registration and semantic partial shapes. |
| Generated enums | Program-specific native `enum : long`; string wire identity is independent of its numeric value. Q7 freezes the tagged/length-delimited SHA-256 byte grammar and collision policy. | Generated value semantics; zero, arbitrary casts, unknown wire names, and duplicate discriminants fail at their specified boundaries. | `Baml.Bridge.EnumDiscriminantProbe` compiles four SHA/value golden vectors, segment-boundary separation, reorder/insertion stability, and zero/collision rejection. The production allocator and generated-name/wire-name/version-skew fixtures remain C2/C5 implementation work. |

The enum contract probe ran warning-free under .NET SDK `10.0.110` and
reported:

```text
enum_discriminant_golden_vectors=4/4
enum_discriminant_segment_boundaries=distinct
enum_discriminant_reorder_insert=stable
enum_discriminant_zero_collision=fail_closed
```

Its project, source, and README SHA-256 values are respectively
`083bf25d9e1fcad0bff524b36ba3f2920a31ce2a79b85ce8707834c39db3a67e`,
`03dea11efe3fbe5cff22538d7991fbdd3bf2b69f749efade8315b323960eed36`,
and `3d120aa9c32a738866962b29a7713e8231c66e334f1065e2f40b239b388a8287`.

The repaired managed slice therefore has no unnamed public resource wrapper,
prompt/provider object, raw native handle, Protobuf type, `object?` dynamic
fallback, public union storage tag, public bytecode loader, or alternate stream
family. Raw `BamlBridge`/`BamlProgram` bootstrap and dispatch remain internal;
the only cross-assembly registration/dispatch metadata is the editor-hidden,
versioned, provenance-bound A3 seam. With A3's exact-package ordinary and
trimmed consumer now passing, B5 is `passed locally`. Final product
public-surface inspection, stream lifecycle parity, B11's trimmed consumer,
generator discriminant vectors, and cross-RID execution remain their own
gates rather than hidden qualifications.

## B6: generic/nullability compile matrix

The default project build and run completed with zero warnings/errors:

```text
generic_compile_positive=complete
```

The positive generated-style surface covers:

- inferred and explicit native generics for `long`, `double`, `BigInteger`,
  nullable references, result-only methods, and class+method scopes;
- unset, supplied, and explicit-null `BamlOptional<T>`;
- required `BamlNullable<T>` and reified nullable reference bindings;
- all three `BamlOptional<BamlNullable<T>>` states using the canonical helper
  syntax;
- canonical `IReadOnlyList<T>` / `IReadOnlyDictionary<string,T>`;
- generated generic `Box<T>`;
- union first/second arms and duplicate closed CLR projections.

The complete matrix was executed through the checked-in fail-closed runner:

```shell
bash \
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.GenericCompileProbe/verify.sh
```

It reported:

```text
positive=passed
NegativeRawOptionalInference=CS0411
NegativeRawNullableInference=CS0411
NegativeComposedRaw=CS1503
NegativeBareNullInference=CS0411
NegativeResultOnlyInference=CS0411
NegativeNonNullableNull=CS8625
NegativeUnionRawInference=CS0411
unknown_case=BAMLGEN001
generic_compile_matrix=complete
```

| Negative case | Required compiler result |
| --- | --- |
| `NegativeRawOptionalInference` | `CS0411`: nested wrapper does not infer `T` from raw string. |
| `NegativeRawNullableInference` | `CS0411`: nested wrapper does not infer `T` from raw long. |
| `NegativeComposedRaw` | `CS1503`: C# does not chain raw long through nullable and optional user conversions. |
| `NegativeBareNullInference` | `CS0411`: bare null cannot infer `T`. |
| `NegativeResultOnlyInference` | `CS0411`: result context does not infer method type arguments. |
| `NegativeNonNullableNull` | `CS8625`, promoted to error: null cannot enter `BamlOptional<string>`. |
| `NegativeUnionRawInference` | `CS0411`: raw value cannot identify union type arguments/occurrence. |

The runner assigns a separate `--artifacts-path` to the positive case, every
negative case, and the typo check. It restores each graph, requires a
warning-free positive build and exact runtime completion marker, requires
every negative build to return nonzero with exactly one assigned compiler
diagnostic and no other warning/error, and proves the project rejects an
unknown `BamlNegativeCase` before compilation with only `BAMLGEN001`.

The runtime half rejects compiler-selected but noncanonical closures without
changing `T`: narrow/unsigned/float/decimal numerics, concrete
`List<T>`/`Dictionary<K,V>` closures, arrays, arbitrary object, JSON DOM,
datetime family, `Guid`, `Uri`, tuples, `BamlOptional<T>` as a value,
context-free union, redundant `BamlNullable`, and noncanonical map keys. Each
failure carries CLR type, nested path, and canonical replacement when one
exists. B6 is `passed locally`; product generated APIs and parity calls remain
implementation work.

## B10: dynamic/type translation matrix

The managed fixture proves:

- acyclic aliases erase at the CLR surface while descriptor aliases survive;
  exact textual literal metadata survives only when an occurrence supplies it;
- every exact numeric `BamlValueKind` from `Null=0` through `Handle=13`
  constructs and round-trips structurally, while the separate
  `BamlTypeDescriptorKind` freezes `Unknown=0` through `Handle=14`;
- standalone null is exactly `BamlValue.Null`, not CLR null. Decode succeeds
  for `BamlValue`, `Nullable<T>`, and `BamlNullable<T>`, while `object`,
  nonnullable references, generated classes, interface/concrete collections,
  and unsupported targets return false/throw rather than receiving
  `default(T)`. Context-free encode similarly rejects ambiguous reference
  nulls and accepts a reified nullable value-type null;
- `long` enforces `[-2^62,2^62-1]`; canonical `double`/`BigInteger` paths are
  distinct; `int`, `float`, `decimal`, unsigned/narrow values fail closed;
- `ReadOnlyMemory<byte>` and media bytes are copied; lists/maps are copied and
  read-only; lists preserve order; dynamic string-key maps sort ordinally and
  reject duplicate canonical keys and CLR-null children. Dynamic empty,
  homogeneous, and heterogeneous containers always use the explicit
  descriptor `Unknown` item/value without first-child inference or a null
  sentinel, and every nested value retains its own descriptor;
- canonical typed list/map decode recursively validates children and returns
  owned `ReadOnlyCollection<T>`/`ReadOnlyDictionary<string,T>` snapshots;
- generated `Person` uses one explicit registered field-by-field codec; no
  reflected member, anonymous object, serializer, or discovery path exists;
- public enum/class/union inspection preserves the exact wire variant,
  declaration-order field names/owned snapshot, and zero-based descriptor arm
  index; every wrong-kind call returns false with zero/null outs;
- nominal class/enum/handle FQNs, concrete generic arguments, ordered union
  arms, active case, media kind/payload identity, and handle wrapper identity
  remain in the descriptor/value tree;
- contradictory union selected-arm metadata fails rather than first-matching;
- canonical generic binding accepts primitives, generated nominal/generic
  types, media/handle/value, native nullable value types,
  `BamlNullable<string>`, `IReadOnlyList<T>`, and legal-key
  `IReadOnlyDictionary<K,V>`;
- semantic partial output distinguishes ordinary nullable absence,
  `@stream.done`, `@stream.not_null`, and zero-default progress state; it never
  introduces caller-presence `BamlOptional<T>`;
- generated graph traversal rejects reference cycles at the exact path.

The following managed allocation limits are now frozen for C# v1. Declared
lengths and counts are rejected before allocation or materialization; depth
and total-node limits are enforced incrementally during graph traversal:

| Limit | Exact value |
| --- | ---: |
| Dynamic/generated value nesting depth | 64 |
| Items in one list/map | 1,000,000 |
| One bytes/media payload | 67,108,864 bytes (64 MiB) |
| Total visited value nodes | 2,000,000 |
| Canonical bigint hex carrier length | 67,108,866 characters (`2^28 / 4 + 2`, matching current Rust/Python/TypeScript guard) |

The item limit permits one one-million-item container; the independent
two-million-node limit bounds shared/repeated nested traversal. The byte limit
is checked before touching a declared oversized `MemoryManager` and before
reading a known-oversized file, then checked again after file read. The
bigint-length path is validated without allocating a cap-sized integer in the
test.

B10 is `passed locally` as compiled semantic evidence after the explicit
`BamlTypeDescriptorKind.Unknown` design amendment. Product Protobuf/native
adapters, generated occurrence descriptors, full shared parity, and final
trim/RID consumers remain implementation work.

## Source hashes

| Source | SHA-256 |
| --- | --- |
| managed project | `6d51462394edc7991aa39b38f7bd05f44a85a6dba12141e49955f2d94c6757e5` |
| optional/nullable/state | `f64ff03e3fa055e2da5332a7c1835846c62ea4ecaada9d017671bba8eb7355ce` |
| media | `6f86d12062f4d4778f7ff63d84126369e37f49a1346e5e82b26b027c41a47999` |
| request/client/handle | `61892f7d62b2ed37ccad29099000e6a80343649bc032b385c6258da275bb1bac` |
| dynamic values | `57c043e897714d71354400f72782bea3e2547f1e3b9ce7ece11bd638a33b8c42` |
| union/binder | `b8f6e5b05a009f0c25262501a057e41566a269bcab265a5368ad0d5bc39e3cb8` |
| managed `Program.cs` | `30a8041c6d8e899f6a684813ba8fb36f23f8d0d40c4b3a3c907c46338bafb0a4` |
| generic project | `e7e623a3964ac916fce371784f373590c2f0f30add5a9e5a594cc3f1b6634358` |
| generic fail-closed executor | `5adc9e453ffffa42abc5bab539c0388678d5c9a590e9d3c3dfeaef85f63cd3e6` |
| generated-style API | `4c466d13775200b9e6184a7c3cb1b93111a04a8f2eb7bab7c768d7d0189da485` |
| positive matrix | `d0c767b05a73d5914f14acad10e08bab7aecfb9e10c1e5d42b1bb173c492997a` |

The remaining small negative source hashes are available directly from the
checked-in files; their exact one-line contents and compiler codes are the
stable evidence identity.
