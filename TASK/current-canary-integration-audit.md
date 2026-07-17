# C# bridge current-Canary integration audit

Date: 2026-07-17

Target branch: `paulo/csharp-bridge`

Target commit: `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0`

Baseline relationship: the target commit is identical to `origin/canary`.
Local branch `paulo/csharp` is a three-commit-ahead historical implementation
and PR #4074 salvage source, not the implementation base.

Toolchain: .NET SDK `10.0.110`, MSBuild `18.0.11`, .NET runtime `10.0.10`,
Rust/Cargo `1.93.0`, Linux x64.

Canonical product version: `0.15.0`, from `baml_language/release.toml` and
`scripts/baml-language-version show`.

## Audit conclusion

Current Canary has the native CFFI substrate and compiler-owned semantic type
model required to build a C# bridge, but it has no tracked C# bridge runtime,
C# generator, C# parity harness, NuGet package, C# release node, or canonical
C# bridge documentation. The only tracked C# project is the unimplemented
`languages/csharp/baml` placeholder with package ID `baml` and version
`0.0.1`.

PR #4074 cannot be merged or cherry-picked as the product contract. It has
useful structural and test fragments, but its Base64 bootstrap, reflection,
mutable collection surfaces, dynamic API loader, recursive alias wrappers,
callback shapes, union fallback/collapse behavior, partial integer checks, and
release assumptions contradict the canonical design. Salvage is permitted
only one fragment at a time after a current-target proof establishes the
desired behavior.

The most important stale design premise was Q1's requirement to import every
native operation directly. Current Canary guarantees only
`baml_get_api_v1`; `register_bridge` exists only in the returned table. Q1 is
therefore explicitly amended to one source-generated getter plus a validated,
typed `BamlApiV1` table. The repository-owned .NET 10 probe proves that
contract against the actual current native artifact on Linux x64.

## Compiler and code-generation boundary

### Canonical type algebra

The single code-generation algebra is compiler-owned:

- `baml_language/crates/baml_type/src/family.rs` declares `CodegenTy`.
- `baml_language/crates/baml_codegen_types/src/ty.rs` reexports it as `Ty` and
  provides exhaustive validation.
- `baml_language/crates/baml_type/src/codegen_ty.rs` recursively canonicalizes
  nested positions and literals.
- `baml_language/crates/baml_project/src/client_codegen.rs` builds the
  generator `SymbolPool`.

The 27 `CodegenTy` variants are:

1. `Int`
2. `Bigint`
3. `Float`
4. `String`
5. `Bool`
6. `Null`
7. `Uint8Array`
8. `Media`
9. `Literal`
10. `Class`
11. `Interface`
12. `Enum`
13. `EnumVariant`
14. `List`
15. `Map`
16. `Union`
17. `Function`
18. `Future`
19. `RustType`
20. `Type`
21. `Resource`
22. `PromptAst`
23. `Void`
24. `TypeAlias`
25. `TypeVar`
26. `BuiltinUnknown`
27. `Never`

The C# translator must match this algebra exhaustively. It may emit a precise
unsupported diagnostic for a deliberately unsupported shape, but it may not
use a wildcard or silently widen unsupported variants to `object?`.

`client_codegen.rs` currently lowers TIR `AssociatedTypeProjection`, `Unknown`,
and `Error` to `BuiltinUnknown`, and `Infer` to `Void`. Those distinctions do
not reach a downstream generator. The C# generator must therefore fail
precisely on the boundary shape it receives; it must not claim it can identify
which collapsed upstream variant produced it.

### Typed identities and allocation inputs

`baml_language/crates/baml_type/src/names.rs` owns package-aware
`QualifiedTypeName` with package, namespace, and name segment APIs.
`baml_language/crates/baml_codegen_types/src/symbols.rs` exposes qualified
class, enum, and alias symbols but only short free-function names.

The closest allocator architecture is
`baml_language/sdks/go/sdkgen_go/src/names.rs`: it keeps typed BAML identity,
wire identity, projected source identity, kind/visibility, scope, and
deterministic allocation separate. C# should reuse that architecture, not its
language-specific spellings.

Current shared-boundary gaps that must land before the corresponding C#
emission:

- Companion functions are inserted as independent `$`-suffixed function
  symbols in `client_codegen.rs`. There is no compiler-owned
  `CallableVariant`/family descriptor. The shared boundary must expose typed
  execute/build-request/build-stream-request/stream/parse identities; the C#
  generator must never recognize `$` text.
- Instance/static classification currently checks whether the first parameter
  string is `self`. The shared descriptor must carry semantic receiver
  identity and placement so the C# generator never infers a receiver from a
  projected string.
- `canonical_union` flattens, deduplicates, and moves null last but preserves
  first-seen arm order. The design requires a deterministic typed-identity
  order independent of source order, so a shared semantic ordering pass or an
  equivalently authoritative C# boundary pass is required before public union
  emission.
- Structural map-key validation currently accepts only string-denoting keys
  through alias/union chains. The design also permits generated enum keys.
  Enum-key support requires a shared validator change and parity tests; until
  that lands the C# generator must reject it rather than disagreeing with the
  compiler.

### Recursive aliases

Recursive aliases already reach codegen as finite named graphs.
`client_codegen.rs` records recursive declarations and its
`aliases_keep_identity_chains_and_canonical_targets_in_codegen` test proves
that `type Rec = int | Rec[]` contains a named `TypeAlias Rec` edge rather
than infinitely expanding.

Gate A4 resolved the public C# projection: ordinary aliases erase to their
underlying CLR types under Q18, while recursive aliases receive a targeted
generator diagnostic before any output transaction replaces existing files.
The generator must not introduce PR #4074's nominal wrapper and must not fall
back to `BamlValue` or `object?`.

## Native ABI and ownership

Canonical authority:

- `baml_language/crates/bridge_cffi/include/baml_cffi.h`
- `baml_language/crates/bridge_cffi/src/api.rs`
- `baml_language/crates/bridge_cffi/src/ffi/runtime.rs`
- `baml_language/crates/bridge_ctypes/src/value_decode.rs`

The public header says dynamic hosts resolve only `baml_get_api_v1`. The
returned immutable, append-only V1 table contains version, initialization,
buffer release, result and host callbacks, ordinary calls, cancellation,
handle clone/release, media construction/access, and bridge registration.
`register_bridge` is not a standalone exported symbol.

Required interop invariants:

- validate non-null table, ABI version 1, `struct_size` through
  `register_bridge`, and every required function pointer before use;
- map `size_t` to `nuint`, fixed-width values exactly, and use the C calling
  convention for every pointer;
- free every returned `BamlBuffer`, including zero-length buffers, exactly
  once through the same table;
- copy borrowed callback payloads before return and contain all managed
  exceptions;
- never unload while buffers, callbacks, calls, or handles can reach the
  library; V1 has no shutdown/unregistration;
- clone a managed-owned ordinary handle before inbound encoding because
  native inbound decode drains the transmitted handle key;
- keep native function-call IDs (`uint64`) distinct from result callback
  correlation IDs (`uint32`);
- preserve Current Canary's process-wide function-call allocator range
  `1_000_000..=u64::MAX`; lower IDs are reserved, exhaustion is permanent,
  and the C ABI alone maps exhaustion to its zero sentinel;
- treat host-owned value keys as registry identities released by the host
  release callback, never by native `handle_release`.

### Compiled feasibility evidence

Current ordinary-release native revalidation:

```shell
cd baml_language
env RUSTC_WRAPPER= cargo build -p bridge_cffi --release
```

Artifact:
`/root/baml-current-native-evidence.NGfRFQ/libbridge_cffi.so`

Size/SHA-256: 20,961,256 bytes /
`cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`.

That digest identifies the immutable isolated artifact consumed by the final
A2 revalidation after the current-run ABI corrections.

Probe:
`baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiProbe`

```shell
dotnet build baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiProbe/Baml.Bridge.AbiProbe.csproj --configuration Release --nologo -p:NuGetAudit=false
dotnet run --project baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiProbe/Baml.Bridge.AbiProbe.csproj --configuration Release --no-build --no-restore -- /root/baml-current-native-evidence.NGfRFQ/libbridge_cffi.so 0.15.0
```

Result: zero build warnings/errors; `api_v1_size=176`;
`product_version=0.15.0`; `csharp_registration=ok`.

This settles A2 feasibility on Linux x64. It does not substitute for B1's
default package resolution, initialization/call/callback/race/cleanup matrix
or C6's all-RID evidence.

## Streams, media, resources, and values

### Streams

Current Canary already exposes the preferred pull model through ordinary
functions `baml.llm.Stream.next` and `baml.llm.Stream.final`; Python uses that
path in `baml_language/sdks/python/src/baml_bridge/_stream.py`. One awaited
`next` per `MoveNextAsync` can implement one-demand/one-partial backpressure
without a new stream-specific C ABI. Each invocation must clone the stream
handle for inbound wire ownership. There is no stream-specific close
operation. The B8 actual-runtime fixture subsequently proved one-demand/
one-completion delivery, idle bounds, final/final-only modes, wait-only
cancellation, pre-cancel, and early release. Product single-enumerator,
concurrent-`MoveNextAsync`, disposal/decode, and late-event parity remain
implementation work.

### Media

The API table and outbound protocol retain media kind, URL/file/base64
representation, and MIME type. Normal in-process CFFI output currently sends
media as a handle rather than an inline media message. C# can eagerly restore
URL/base64 values through the accessors, copy/decode owned data, release all
buffers and the handle, and return immutable non-disposable managed media.

The B9 actual-table probe subsequently proved the exact in-process shape:
`baml.media.Image|Audio|Pdf|Video` is a nominal `class_value` whose sole
`_data` field is the corresponding typed media handle. The C# bridge must
validate both layers, eagerly restore the one URL/base64/file representation
and MIME type, and release the inner handle even when decode validation fails.
File output can be read and copied before returning; deletion of the source
path afterward does not affect the managed value. All four kinds passed URL,
base64, and file round trips, so no protocol amendment is required. See
`TASK/stream-media-abi-evidence.md`.

### Resolved standard-library boundary inventory

A6's subsequent exhaustive audit found 25 `$rust_type` fields in 24 current
standard-library classes and separately classified the current
resource/client/prompt/helper surfaces. The identity-by-identity normative
table is now in `TASK/state-of-csharp-completeness.md`.

The resolved policy uses immutable `BamlHttpRequest`, `BamlClient` plus its
client/retry metadata, the four media values, and `BamlStream` only for their
explicit contracts. Known stateful pass-through resources—including BAML's
own cancel token, which is not CLR operation cancellation—use descriptor-bound
`BamlHandle`. `PromptAst` is opaque pass-through; prompt/provider/cache
internals are unsupported in direct user signatures. Any new unclassified raw
`RustType` FQN is a generator error rather than permission to invent a wrapper.

## Literal unions and integer domain

The literal-union producer bug was in the shared engine matcher: a value could
match the first arm with the same runtime tag rather than the exact literal,
creating contradictory selected-arm metadata. The matcher and host-return
validator now compare exact integer, bigint, float, string, and boolean
literals. Targeted tests pass:

```shell
cd baml_language
env RUSTC_WRAPPER= cargo test -p bex_engine literal_union_selection_tests --lib -q
env RUSTC_WRAPPER= cargo test -p bex_external_types literal_value_equality --lib -q
```

A7 subsequently recorded the exact 40-byte outbound CFFI `"crlf"` envelope
and a pinned C# Protobuf decoder negative test rejecting selected `"lf"` with
payload `"crlf"`; see `TASK/verification-gates.md`.

The canonical BAML integer domain is
`[-4_611_686_018_427_387_904, 4_611_686_018_427_387_903]`
(`[-2^62, 2^62-1]`). Authority is
`baml_language/crates/bex_vm_types/src/types/value.rs`, mirrored in
`baml_language/crates/baml_compiler2_tir/src/lib.rs` and enforced by
`baml_language/crates/bex_engine/src/conversion.rs`. Protobuf uses `int64` as
a deliberately wider carrier. A8's pinned C# protocol probe now rejects
min-minus-one, max-plus-one, `long.MinValue`, and `long.MaxValue` during both
encode and decode and covers scalar, literal, and list paths.

## Generator, harness, and parity integration

Current registration has no C# variant:

- `baml_language/crates/baml_codegen_types/src/generator_fields.rs` has no
  C# `OutputType`;
- `baml_language/crates/baml_cli/src/generate.rs` has no C# dispatch;
- `baml_language/Cargo.toml` has no `sdkgen_csharp` or `sdk_test_csharp`;
- `baml_language/sdk_tests/harness_setup/src/lib.rs`,
  `baml_language/sdk_tests/harness_runner/src/lib.rs`, and
  `baml_language/.config/nextest.toml` have no C# lane;
- `.github/workflows/cargo-tests.reusable.yaml` has no C# SDK-test matrix
  entry.

The closest harness pattern is TypeScript Node: build the native bridge once,
generate one existing-project `.csproj` per fixture, overlay C# customizable
tests, then run `dotnet build`/`dotnet test` through the harness runner and
nextest setup. C# parity identities must be ported from the current Python
tests and recorded by exact file/test name in
`TASK/state-of-csharp-completeness.md`; ignored local generated residue is not
evidence.

## Platform, package, and release graph

`release/platforms.json` is the repository-owned target authority. The
evidence working tree extends each CFFI entry with an explicit nested .NET RID,
canonical package asset, and consumer runner so package and verification jobs
do not maintain a second triple-to-RID mapping:

| Rust target | .NET RID | release asset | in-package native name | current tier |
| --- | --- | --- | --- | --- |
| `aarch64-apple-darwin` | `osx-arm64` | `libbaml_cffi-aarch64-apple-darwin.dylib` | `libbridge_cffi.dylib` | required |
| `x86_64-apple-darwin` | `osx-x64` | `libbaml_cffi-x86_64-apple-darwin.dylib` | `libbridge_cffi.dylib` | required |
| `aarch64-unknown-linux-gnu` | `linux-arm64` | `libbaml_cffi-aarch64-unknown-linux-gnu.so` | `libbridge_cffi.so` | required |
| `x86_64-unknown-linux-gnu` | `linux-x64` | `libbaml_cffi-x86_64-unknown-linux-gnu.so` | `libbridge_cffi.so` | required |
| `aarch64-unknown-linux-musl` | `linux-musl-arm64` | `libbaml_cffi-aarch64-unknown-linux-musl.so` | `libbridge_cffi.so` | experimental |
| `x86_64-unknown-linux-musl` | `linux-musl-x64` | `libbaml_cffi-x86_64-unknown-linux-musl.so` | `libbridge_cffi.so` | experimental |
| `x86_64-pc-windows-msvc` | `win-x64` | `baml_cffi-x86_64-pc-windows-msvc.dll` | `bridge_cffi.dll` | required |
| `aarch64-pc-windows-msvc` | `win-arm64` | `baml_cffi-aarch64-pc-windows-msvc.dll` | `bridge_cffi.dll` | experimental |

The native build matrix is derived from this file in
`.github/workflows/build2-bridge-cffi.reusable.yaml`. A NuGet assembler must
copy/rename the triple-suffixed release artifacts to
`runtimes/{rid}/native/{canonical-name}`; it must not publish the release asset
name as the P/Invoke base name.

Three targets are marked experimental and permitted to fail during the native
build, yet downstream C++ verification currently requires all eight. The tier
contract is already inconsistent. C# must not independently hard-code an
untested eight-item list. Before claiming all eight RIDs, add a tested
repository-owned `.NET RID` mapping and either promote the experimental
targets with consumer evidence or make their omission explicit in package
support. The design's one-package/eight-RID decision means a production
release ultimately requires all eight inputs atomically.

Current `release-baml-language.yml` has no C# build/verify/publish node.
`scripts/baml-language-version` explicitly omits NuGet, and release manifests
record no NuGet coordinate/digest. PR #4074's workflow fragments may inform a
rewrite, but no current immutable package assembly, exact-artifact publisher,
public restore smoke, or organization/trusted-publisher proof exists.

## PR #4074 salvage decisions

| Fragment | Decision |
| --- | --- |
| compiler-owned `Ty` consumption, routing skeleton, CLI/workspace hook shape | candidate for line-level salvage after shared semantic boundary changes |
| generated-code whole-directory transaction ideas | candidate only after replacing the experimental manifest/adoption contract with canonical entire-directory ownership |
| union structs/tests | rerun the Q8 layout matrix first; never retain projection deduplication, first-payload selection, or `object?` fallback |
| integer constants/tests | salvage inbound bounds/tests only; add checked outbound decode and complete vectors |
| API-table struct concept | concept confirmed; rewrite as one source-generated getter with assembly-owned resolution and structured table validation |
| reflection-driven codecs/factories | reject |
| `object?`, `List<>`, `Dictionary<>`, invented prompt/resource wrappers | reject |
| Base64 bytecode and 8 MiB cap | reject |
| sync/`ValueTask`/tokenless optional callback delegates | reject |
| recursive alias wrapper classes | reject; A4/Q18 now deliberately diagnose every recursive alias SCC before output replacement |
| C# workflow/RID/package snippets | candidate reference only after current platform mapping, exact package identity, and non-compiling publisher contracts are implemented |
| ignored generated fixtures and cached build artifacts | reject as evidence; regenerate from the target implementation |

## A1 disposition and preimplementation dependencies

A1's current-Canary audit is complete. Subsequent target probes have also
settled A2's canonical API-table binding, A3's versioned cross-assembly seam,
A4's recursive-alias diagnostic, A5's optional callback slots, A6's exhaustive
resource/client/prompt inventory, A7's exact union metadata, and A8's checked
integer domain; their current evidence is authoritative in
`TASK/verification-gates.md`.

The implementation plan must use those resolved contracts and sequence these
shared compiler changes before broad C# emission:

1. Add typed callable variants/receivers and enum-map-key validation to the
   shared compiler/codegen boundary.
2. Establish deterministic typed union-arm ordering independent of source
   discovery order.

No implementation-plan phase may cite PR #4074 line numbers or ignored local
outputs as current behavior.
