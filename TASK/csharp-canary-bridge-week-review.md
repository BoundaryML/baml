# C# review of canary bridge week

Review snapshot: 2026-07-17 07:43:54 UTC.

## Executive verdict

PR [#4074](https://github.com/BoundaryML/baml/pull/4074) has a sound
ecosystem-specific direction: a single all-RID NuGet package, normal .NET
native-asset probing, exact SDK/runtime version registration, deterministic
generation, and explicit unsupported-shape handling are all appropriate for
C#.

It should not be merged onto current `canary` unchanged. The review found three
P0 integration/correctness areas:

1. The generator must migrate to the compiler-owned `CodegenTy` introduced by
   [#4048](https://github.com/BoundaryML/baml/pull/4048). The current C# source
   uses the removed type variants and direct qualified-name fields, so it will
   not compile after a semantic rebase.
2. The C# bridge must consume the generated public C ABI contract introduced by
   [#4055](https://github.com/BoundaryML/baml/pull/4055), including regenerating
   the header after appending `flush_events`. That contract also exposes two
   current C# buffer/optional-value correctness bugs.
3. C# type lowering must collapse duplicate CLR alternatives and specialize
   valid map keys. Today literal unions can produce invalid or ambiguous shapes
   such as `BamlUnion<string, string>` and
   `Dictionary<BamlUnion<string, string>, ...>`.

Before publication, C# should also derive its claimed platform set from the new
repository platform contract, run clean NuGet consumer tests on the claimed
RIDs, add a publish/post-publish gate, and make its parity-test ledger as
explicit as the Rust, Go, C++, and TypeScript bridge work.

## Scope and branch position

The GitHub query covered all 47 PRs merged to `canary` from Monday
2026-07-13 through the snapshot above. The review used PR metadata and patches,
then compared relevant changes with the local C# generator, managed bridge,
test harness, and release workflow.

The C# branch point is `408b2be28a`, the merge of
[#4034](https://github.com/BoundaryML/baml/pull/4034). At the snapshot,
`canary` is at `1ebf901f78`, the merge of
[#4073](https://github.com/BoundaryML/baml/pull/4073). The C# branch is one
commit ahead and 18 commits behind, and GitHub reports the draft PR as
conflicting.

## P0: compiler-owned codegen types

[#4048](https://github.com/BoundaryML/baml/pull/4048) removed the parallel
generator type algebra. `baml_codegen_types::Ty` is now a re-export of
compiler-owned `baml_type::CodegenTy`, and `Name` is a re-export of
`QualifiedTypeName`.

This affects these C# files directly:

- `baml_language/sdks/csharp/sdkgen_csharp/src/translate_ty.rs`
- `baml_language/sdks/csharp/sdkgen_csharp/src/lib.rs`
- `baml_language/sdks/csharp/sdkgen_csharp/src/routing.rs`
- `baml_language/sdks/csharp/sdkgen_csharp/src/models.rs`
- `baml_language/sdks/csharp/sdkgen_csharp/src/leaf.rs`

Required migration:

- Replace `Ty::Callable` with
  `Ty::Function { params, ret, throws, attr }`.
- Replace `Ty::Unit` with `Ty::Void { .. }`; explicitly classify
  `Ty::Never { .. }`.
- Remove the `Ty::BamlOptions` special case and use the canonical replacement.
- Update every primitive, literal, container, class, enum, alias, and type-var
  pattern for its canonical attributes.
- Explicitly classify `Interface`, `EnumVariant`, `Future`, `Type`,
  `Resource`, and `PromptAst`. Do not use a wildcard arm; a new compiler type
  should force a target-policy audit.
- Use `name.is_local()`, `name.package()`, `name.namespace()`, `name.name()`,
  `name.is_stream()`, and `name.bare_name()` instead of reading `pkg`,
  `namespace_path`, and `name` fields.
- Keep `name.to_string()` for canonical wire identity.
- Continue transparently expanding nonrecursive aliases because C# has no
  exported alias declaration. Keep nominal wrappers for recursive aliases.
- Preserve the exact byte protocol in `models.rs::enum_discriminant_with`
  while switching it to name accessors. Changing those hash inputs would
  change the generated enum numeric ABI.

Suggested C# target policy for the newly visible variants:

| Canonical variant | C# policy |
| --- | --- |
| `Function` | Keep existing `Func`/`Action`/custom delegates; bind and intentionally ignore `throws` because CLR exceptions are unchecked. |
| `Interface` | Use an explicitly unsupported/opaque boundary until generated interface semantics are designed; do not invent CLR interface declarations. |
| `EnumVariant` | Lower to the owning generated enum type. |
| `PromptAst` | Reuse `BamlPromptAst`. |
| `RustType` | Keep `BamlHandle`. |
| `Type`, `Resource`, `Future` | Keep unsupported unless a verified managed wrapper exists. Do not mark an `object?` placeholder as codec-supported. |
| `Void`, `Never` | Give each an explicit callable/value policy and tests. |

The shared compiler now owns recursive union/null canonicalization,
alias identity/chains, and alias-aware map-key validity. C# should consume
those guarantees and retain only target-specific CLR lowering.

## P0: public C ABI and ownership

[#4055](https://github.com/BoundaryML/baml/pull/4055) makes
`baml_language/crates/bridge_cffi/include/baml_cffi.h` the generated,
reviewable C ABI contract. It adds deterministic header drift checks, Rust/C/C++
layout assertions, calling-convention checks, and one-symbol dynamic-loader
smokes.

The C# branch appends `flush_events` in
`baml_language/crates/bridge_cffi/src/api.rs` and manually mirrors the table in
`baml_language/sdks/csharp/bridge_csharp/src/Baml.Bridge/Bridge/NativeApi.cs`.
Its branch point predates the canonical header.

Required integration:

- Reapply `flush_events` as an append-only `BamlApiV1` field on current
  `canary`.
- Regenerate the checked-in header through the pinned cbindgen path.
- Extend the header freshness, ABI layout, C11/C++17 assertions, and runtime
  smoke coverage for the appended field.
- Add a managed ABI layout/drift test for `ApiV1`, `BridgeInfoV1`,
  `NativeBuffer`, enum widths/discriminants, field offsets, function-pointer
  order, and C calling conventions. Generating the managed declarations from
  the header is optional; testing the manual declarations is not.
- Check compatibility through the end of the `flush_events` field because C#
  requires that extension, while preserving the original V1 minimum prefix for
  consumers that do not.

Two concrete correctness bugs follow from the now-documented buffer contract:

1. `NativeApi.CopyAndFree` calls `free_buffer` only when `Pointer != null`.
   Every owned return buffer must be passed exactly once to the allocating
   table's `free_buffer`, including `{ptr = null, len = 0}`.
2. `NativeApi.ReadMediaString` detects an absent optional value using
   `Pointer == null`. The ABI defines absence by `len == 0`; an empty Rust-owned
   slice may have a non-null pointer. The current code can therefore decode an
   absent URL/file/MIME value as `""` instead of `null`, and it can skip the
   required release.

## P0: CLR union and map-key lowering

The compiler now preserves literal and alias identity at the public codegen
boundary. [#4065](https://github.com/BoundaryML/baml/pull/4065) also makes
numeric literal types easier to author with base prefixes and separators.

`translate_literal` widens literals to their CLR primitive, but
`translate_union` does not deduplicate the widened alternatives:

- `"a" | "b"` becomes `BamlUnion<string, string>`.
- `0x1 | 0x2` becomes `BamlUnion<long, long>`.
- Nullable versions retain the duplicate alternatives.
- A compiler-valid map key such as `"a" | "b"` becomes
  `Dictionary<BamlUnion<string, string>, ...>` rather than
  `Dictionary<string, ...>`.

Required correction:

- Add target-level post-translation deduplication by emitted CLR type.
- Merge support/host-callable flags for alternatives that collapse.
- Collapse a union with one distinct non-null CLR alternative, then apply
  nullability.
- Add a dedicated map-key translation that follows aliases and maps every
  compiler-validated string-denoting key, including string-literal unions, to
  CLR `string`.

Regression cases should include string-literal unions, aliased string-literal
map keys, `0x1 | 0x2`, `1 | 2 | null`, enum variants, and alias chains.

## P1: platform and release convergence

[#4045](https://github.com/BoundaryML/baml/pull/4045) introduced
[`release/platforms.json`](https://github.com/BoundaryML/baml/blob/canary/release/platforms.json)
as the repository-owned target contract. The C# implementation independently
hardcodes the same eight RIDs in:

- `.github/workflows/build2-csharp-sdk.reusable.yaml`
- `baml_language/sdks/csharp/bridge_csharp/tools/pack-all-native.sh`
- `baml_language/sdks/csharp/bridge_csharp/tools/pack-native.sh`
- `baml_language/sdks/csharp/bridge_csharp/tools/pack-native.ps1`
- `baml_language/sdks/csharp/bridge_csharp/buildTransitive/baml-bridge.targets`
- `baml_language/sdks/csharp/bridge_csharp/src/Baml.Bridge/Baml.Bridge.csproj`
- `baml_language/sdks/csharp/bridge_csharp/src/Baml.Bridge/Bridge/BridgePlatform.cs`

The contract currently marks both musl targets and Windows ARM64 CFFI artifacts
as experimental. The atomic NuGet build requires all eight and therefore makes
those experimental artifacts release-blocking.

Recommended integration:

- Add or validate a C# RID mapping from the platform contract rather than
  maintaining an independent target set.
- Decide explicitly whether the all-RID NuGet package promotes experimental
  targets to required. Record that policy in the contract or release plan.
- Add a contract-derived `verify-csharp-sdk` matrix. On every claimed target,
  install the exact assembled `.nupkg` from a clean local feed, disable
  repository/native-path fallbacks, generate a small SDK, execute a real call,
  and verify native RID selection and the frozen version.
- Include native Alpine/musl and Windows ARM64 coverage if those RIDs remain
  claimed.

Keep the single all-RID NuGet package. The Go/Rust runtime downloaders solve
different ecosystem constraints; NuGet `runtimes/<rid>/native` assets are the
idiomatic .NET delivery mechanism.

The current C# workflow builds and uploads `csharp-sdk-nuget` but does not
publish it. Before production:

- Publish the exact verified `.nupkg`; do not repack after verification.
- Use a separate trusted-publishing job, following the build/publish split in
  [#4052](https://github.com/BoundaryML/baml/pull/4052).
- Make public release/manifest completion depend on successful NuGet
  publication.
- Record the package ID, registry, and exact version in the release manifest
  using a schema agreed with the release owners.
- Run a post-publish clean consumer smoke, following
  [#4067](https://github.com/BoundaryML/baml/pull/4067).

NuGet organization and trusted-publisher setup remain external prerequisites,
but the repository workflow should encode them as required release gates.

## P1: parity tests

Rust [#4045](https://github.com/BoundaryML/baml/pull/4045) ports shared test
identities and uses an explicit Now/Later gate. C++
[#4004](https://github.com/BoundaryML/baml/pull/4004) separates shared-fixture
compile checks from runtime checks. TypeScript
[#4062](https://github.com/BoundaryML/baml/pull/4062) copies one shared source
tree and selects runtime-specific suites inline.

C# currently opts in only the nine fixture directories that contain a C#
`customizable/` overlay:

- `csharp_cancel_token`
- `csharp_csv`
- `csharp_glob`
- `csharp_llm_clients`
- `csharp_resources`
- `csharp_task_group`
- `function_calls`
- `llm_functions`
- `primitive_calls`

This gives strong bespoke coverage but makes cross-language completeness
difficult to audit. Before publication:

- Reuse shared `type_shapes` and function-call test identities for supported
  behavior, or add a Rust-style explicit Now/Later parity ledger enforced by
  the harness.
- Keep C#-specific resource, packaging, union-layout, and hard-exit probes
  separate.
- Add canonical-type migration cases: interface boundaries, `PromptAst`,
  `Void`, `Never`, function `throws`, alias chains, same short alias names in
  different namespaces, and the literal-union/map-key cases above.
- Add a clean package-reference test rather than relying only on a project
  reference to `Baml.Bridge.csproj`.

## P2: naming and loader consistency

The C# naming implementation is already strong: it allocates deterministically,
escapes keywords, reserves generated locals, handles case-insensitive path
collisions, keeps exact wire identities, and does not depend on discovery
order.

Go [#4067](https://github.com/BoundaryML/baml/pull/4067) and C++
[#4004](https://github.com/BoundaryML/baml/pull/4004) improve maintainability
further by allocating typed requests of the form
`(FQN, declaration kind, scope/visibility) -> (host identifier, wire identity)`
before rendering. C# could consolidate its free-form identity strings into a
similar `CSharpNames` table. This is an optional refactor; the current naming
behavior should not change.

The bridges also converge on `BAML_RUNTIME_PATH`, with
`BAML_LIBRARY_PATH` as a compatibility alias and conflict diagnostics. C# uses
only `BAML_BRIDGE_LIBRARY`. Keep NuGet probing first, but standardize the
explicit-path override and retain the C# spelling only as a conflict-checked
compatibility alias if needed. Do not port the Go/Rust downloader.

## Patterns already aligned

- C# resolves only `baml_get_api_v1`, keeps the native library loaded, validates
  the append-only table, and performs exact SDK/runtime registration from
  [#4041](https://github.com/BoundaryML/baml/pull/4041).
- It uses the shared bytecode initializer from
  [#4009](https://github.com/BoundaryML/baml/pull/4009).
- It consumes the language-neutral CFFI artifacts built once by
  [#4037](https://github.com/BoundaryML/baml/pull/4037) and the musl fix from
  [#4040](https://github.com/BoundaryML/baml/pull/4040), verifies their hashes,
  and does not rebuild them in the C# packaging job.
- Exact release-plan stamping, deterministic NuGet normalization, the exact
  eight-entry package inspection, the package-size ceiling, and
  `Grpc.Tools` with `PrivateAssets=all` are appropriate.
- One distinct BAML program per process is an explicit V1 limitation rather
  than an accidental behavior.
- [#4063](https://github.com/BoundaryML/baml/pull/4063) confirms that provider
  defaults belong in the runtime. `BamlClient.FromShorthand` correctly avoids
  injecting environment/provider defaults in managed code.
- Compiler caching and parallel emit remain byte-identical, so the C# embedded
  bytecode/fingerprint contract needs no changes for
  [#3924](https://github.com/BoundaryML/baml/pull/3924),
  [#4058](https://github.com/BoundaryML/baml/pull/4058), or
  [#4073](https://github.com/BoundaryML/baml/pull/4073).

## Post-branch-point PR matrix

These are the 18 `canary` commits after the C# branch point, in merge order.

| PR | Relevance to C# |
| --- | --- |
| [#4047](https://github.com/BoundaryML/baml/pull/4047) unified declaration namespace | No representation change. Keep C# projection collision handling because casing, keywords, generated names, and path rules can still collide. |
| [#4038](https://github.com/BoundaryML/baml/pull/4038) compiler profiler | Tooling only; no C# action. |
| [#4051](https://github.com/BoundaryML/baml/pull/4051) TypeScript runtime split | Multi-runtime split is not applicable; shared test-source organization is. |
| [#4050](https://github.com/BoundaryML/baml/pull/4050) target-neutral CFFI | Native ABI is preserved. Reapply C# changes on the new native/Wasm module structure without coupling managed code to internals. |
| [#4048](https://github.com/BoundaryML/baml/pull/4048) canonical codegen types | P0 compile blocker and type-policy migration. |
| [#4055](https://github.com/BoundaryML/baml/pull/4055) generated V1 C ABI | P0 ABI/header/ownership integration. |
| [#4058](https://github.com/BoundaryML/baml/pull/4058) cold compile optimization | Upstream only; no C# query/cache work. |
| [#4057](https://github.com/BoundaryML/baml/pull/4057) size baselines | No C# action. |
| [#4052](https://github.com/BoundaryML/baml/pull/4052) Web bridge release scaffold | Reuse the immutable build, separate publish, and registry-verification pattern. |
| [#4042](https://github.com/BoundaryML/baml/pull/4042) mathematical operator interfaces | Bytecode/runtime behavior only. Do not synthesize CLR operators. Explicitly classify interface-typed boundaries after #4048. |
| [#4062](https://github.com/BoundaryML/baml/pull/4062) TypeScript Web/Workers tests | Reuse the shared-test-source and explicit runtime-selection ideas where applicable. |
| [#4065](https://github.com/BoundaryML/baml/pull/4065) numeric literal syntax | No lexeme handling in C#; add normalized literal-union regression cases. |
| [#4045](https://github.com/BoundaryML/baml/pull/4045) Rust bridge | Reuse platform-contract, parity-ledger, package verification, and release-gating patterns. |
| [#4063](https://github.com/BoundaryML/baml/pull/4063) client defaults/validation | Already correctly layered in the native runtime; no managed default injection. |
| [#4064](https://github.com/BoundaryML/baml/pull/4064) ItemTree firewalls | No action. C# consumes the finished symbol pool and bytecode, not compiler query internals. |
| [#4004](https://github.com/BoundaryML/baml/pull/4004) C++ bridge/package | Reuse ABI-header, platform consumer, and canonical loader-override patterns. Preserve its C++ language ID/header changes. |
| [#4067](https://github.com/BoundaryML/baml/pull/4067) Go bridge/release | Reuse typed naming ideas, immutable publication gating, and clean external consumer smoke. Keep NuGet delivery instead of the Go downloader. |
| [#4073](https://github.com/BoundaryML/baml/pull/4073) parallel compile/emit | No C# semantic action; serialized output is tested byte-identical. |

## Earlier foundations already present at the branch point

| PR | Assessment |
| --- | --- |
| [#3998](https://github.com/BoundaryML/baml/pull/3998) runtime `RealizedTy` | Already inherited. C# correctly sends concrete CLR type descriptors for generic calls; add a closed/open generic regression test. |
| [#4009](https://github.com/BoundaryML/baml/pull/4009) bytecode runtime initializer | Already used. Correct the owned-buffer release behavior described above. |
| [#4032](https://github.com/BoundaryML/baml/pull/4032) canonical type algebra | Reinforces that normalization belongs upstream of `sdkgen_csharp`. |
| [#4034](https://github.com/BoundaryML/baml/pull/4034) explicit function throws | Already in the branch point. The canonical `Function.throws` child becomes visible after #4048; CLR delegate signatures remain unchanged. |
| [#4037](https://github.com/BoundaryML/baml/pull/4037) CFFI release artifacts | Already reused correctly. |
| [#4040](https://github.com/BoundaryML/baml/pull/4040) musl cdylib fix | Already inherited through shared artifacts; still needs a real managed musl smoke. |
| [#4041](https://github.com/BoundaryML/baml/pull/4041) versioned bridge API | Already used correctly; #4055 makes its generated contract authoritative. |

## Remaining weekly PR accounting

The other 22 merges were checked and do not require C# bridge changes:

- Compiler/source behavior with no new C# boundary shape:
  [#3924](https://github.com/BoundaryML/baml/pull/3924),
  [#4005](https://github.com/BoundaryML/baml/pull/4005), and
  [#4031](https://github.com/BoundaryML/baml/pull/4031).
- Runtime provider behavior inherited below the bridge:
  [#3964](https://github.com/BoundaryML/baml/pull/3964).
- Release baseline already reflected by C# version stamping:
  [#4013](https://github.com/BoundaryML/baml/pull/4013).
- Grammar, LSP, CLI UX, documentation, and repository maintenance:
  [#3867](https://github.com/BoundaryML/baml/pull/3867),
  [#3996](https://github.com/BoundaryML/baml/pull/3996),
  [#3999](https://github.com/BoundaryML/baml/pull/3999),
  [#4000](https://github.com/BoundaryML/baml/pull/4000),
  [#4001](https://github.com/BoundaryML/baml/pull/4001),
  [#4002](https://github.com/BoundaryML/baml/pull/4002),
  [#4007](https://github.com/BoundaryML/baml/pull/4007),
  [#4008](https://github.com/BoundaryML/baml/pull/4008),
  [#4010](https://github.com/BoundaryML/baml/pull/4010),
  [#4011](https://github.com/BoundaryML/baml/pull/4011),
  [#4015](https://github.com/BoundaryML/baml/pull/4015),
  [#4017](https://github.com/BoundaryML/baml/pull/4017),
  [#4021](https://github.com/BoundaryML/baml/pull/4021),
  [#4023](https://github.com/BoundaryML/baml/pull/4023),
  [#4024](https://github.com/BoundaryML/baml/pull/4024),
  [#4026](https://github.com/BoundaryML/baml/pull/4026), and
  [#4029](https://github.com/BoundaryML/baml/pull/4029).

## Suggested integration order

1. Rebase semantically and preserve every output target, workspace member,
   release job, nextest rule, and harness addition from current `canary`.
2. Migrate `sdkgen_csharp` to canonical compiler-owned types and qualified-name
   accessors.
3. Fix union/map-key lowering and add canonical-type regression coverage.
4. Port `flush_events` onto the generated ABI contract, fix buffer ownership,
   and add managed ABI drift tests.
5. Derive/validate the RID set from the platform contract and run clean local
   NuGet consumer tests across the claimed matrix.
6. Make parity status explicit, then add NuGet publication and post-publish
   consumer gates.
7. Consider typed name requests and loader-variable cleanup after the
   correctness/release work is complete.

No implementation files were changed as part of this review.
