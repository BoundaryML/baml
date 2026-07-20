# C# bridge verification and implementation-readiness gates

This ledger tracks proof that is broader than one capability test. Keep it
separate from `state-of-csharp-completeness.md`: a call may work locally while
the ABI, packaging, trimming, regeneration, or release contract is still
unproven.

Every completed gate must record:

- target repository commit;
- exact command and relevant environment;
- test/probe source path; external promotion additionally requires that exact
  source to be committed at the recorded repository SHA;
- result summary and artifact path/digest where applicable;
- which design question and completeness rows it closes;
- remaining host/RID limitations.

Dry-run evidence is historical input only. `dry-run-findings.md` explains what
was observed and what may not be imported.

The target commit below is the unchanged Current-Canary baseline against which
the probes were designed. The C# fixtures, gate workflows, contract changes,
and `TASK/` records first landed in pushed provenance commit
`6d52aff1446c66be440771a14b85512c67214ca1`. Trigger-bootstrap commit
`9d29c01928df7ce726c49286a3067129fc039115` and
`c44ac516a6f71fac143c4ff239beae424b042222`, and
`ccf3bcfadd5a919b2cbee205ace07a1ac9cd565c`, with their matching exact-source
tags, are pushed. The third attempt produced the exact eight-RID package and
passed protocol plus semantic/deployment fan-in, but four consumer jobs failed
on verifier plumbing; the result is recorded below and promotes no
implementation-entry status.

## Target record

| Field | New-run value |
| --- | --- |
| Target branch | `paulo/csharp-bridge` (at audit start, identical to `origin/canary`) |
| Target commit | `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0` |
| Evidence-tree provenance | First provenance commit `6d52aff1446c66be440771a14b85512c67214ca1`; bootstrap commits `9d29c01928df7ce726c49286a3067129fc039115`, `c44ac516a6f71fac143c4ff239beae424b042222`, and `ccf3bcfadd5a919b2cbee205ace07a1ac9cd565c`, with their exact-source tags, are pushed. Atomic attempt [29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216) used the latest tag/SHA, produced the exact package, and failed two musl plus two Windows consumer jobs on verifier plumbing. |
| Starting point | Current Canary. Local historical branch `paulo/csharp` remains salvage-only and is not the implementation base. |
| .NET SDK/runtime | Local audit: SDK `10.0.110`, MSBuild `18.0.11`, runtime `10.0.10`; external workflow: exact SDK `10.0.301`; C# 14 / `net10.0` |
| Canonical CLI/package version | Canary `0.15.0` from `baml_language/release.toml` and `scripts/baml-language-version show` |
| Required native RIDs | `release/platforms.json` now owns each CFFI target's .NET RID, canonical package asset, and consumer runner for `osx-arm64`, `osx-x64`, `linux-arm64`, `linux-musl-arm64`, `linux-x64`, `linux-musl-x64`, `win-x64`, and `win-arm64`. The C# evidence workflow requires all eight atomically, including targets still marked experimental for other upstream uses; C6 remains blocked until all eight real consumers pass. |
| Last updated | `2026-07-20` |

## Gate status vocabulary

- `not started`
- `in progress`
- `blocked` — name the exact blocker and owner
- `passed locally` — useful evidence but host/RID matrix remains
- `passed` — complete acceptance criteria recorded
- `superseded` — design was explicitly amended; link the replacement

## A. Pre-implementation reconciliation

These items must be settled before the implementation plan treats the relevant
surface as fixed.

| ID | Status | Gate | Required outcome |
| --- | --- | --- | --- |
| A1 | passed | Current-Canary semantic integration audit | `TASK/current-canary-integration-audit.md` records the target compiler/codegen boundary, all 27 canonical type variants, typed-identity gaps, canonical C ABI/table and ownership, stream/media implications, eight-target platform contract, release graph, shared parity harness, and explicit PR #4074 salvage decisions. The implementation plan must cite this audit rather than stale experimental paths. |
| A2 | passed locally | Canonical API-table import feasibility | Current Canary's public header makes `baml_get_api_v1` the sole dynamic-host entry point and exposes `register_bridge` only through `BamlApiV1`. Q1 is explicitly amended to one source-generated getter plus a validated typed table. The .NET 10 probe compiles warning-free and validates/calls the actual Linux x64 table; default package resolution and the cross-RID matrix remain B1/C6 obligations. |
| A3 | passed locally | Cross-assembly trim-safe generated-code seam | A fresh `0.0.0-a3` package and isolated exact-feed unrelated consumer build/run pass warning-free. The editor-hidden V1 seam uses reference-bound opaque program/function/argument/type-binding tokens, rejects cross-builder aliases, duplicate/default/frozen/contradictory tokens and all three version mismatches, preserves exact canceled-task/token identity, and executes fixed sync/async, result-only generic, optional omission/null, receiver, build-request, and typed stream partial/final variants. Generated codecs exercise the complete representative V1 carrier vocabulary and nested generic class/list/map/enum/union/optional shapes without reflection, Protobuf, activation, or friend access; a trimmed publish also executes successfully. |
| A4 | passed | Recursive alias representation | Current compiler tests prove direct, mutual, collection, nullable, and union recursion reach codegen as finite named graphs. Q18 now explicitly rejects every recursive alias SCC in C# v1 before output replacement because erased CLR aliases cannot represent a cycle without nominal semantics. The experiment's wrapper and dynamic/partial fallbacks are forbidden. |
| A5 | passed locally | Optional host-callable argument model | The Rust production vectors and omission/nullability cases pass. The second-audit correction routes required and present-optional callback integers through one checked decoder, covers both exact bounds, rejects four out-of-domain carriers plus two malformed carriers at each position, and proves invalid carriers never invoke the callback. The external host matrix remains untriggered. |
| A6 | passed | Standard-library resource inventory | The completeness ledger now classifies every current rust-backed standard-library identity plus HTTP request, streams, prompt helpers, client/retry/provider shapes, raw `Resource`, host-exception identity, and plain option/result companions as an explicit managed value, opaque `BamlHandle`, internal/unsupported shape, or ordinary nominal value. Design Q17/public inventory records the exhaustive-allowlist rule; unknown `RustType` FQNs fail generation. |
| A7 | passed | Literal-union metadata correctness | The shared producer matcher and host-return validator now require exact literal equality. A Rust CFFI-envelope regression records exact `"crlf"` selected metadata and wire bytes; the pinned current C# Protobuf probe decodes those bytes and rejects selected `"lf"` with payload `"crlf"`. The schema comment makes strict selected-arm validation normative for descriptor-aware typed bindings such as C#, without retroactively requiring legacy dynamic bindings to reconstruct discarded descriptors. |
| A8 | passed locally | Exact BAML integer domain | Standalone scalar/literal/container checks and the corrected host-callback dispatch use the same checked outbound decoder. Required and present-optional paths accept the exact bounds; each rejects minimum-minus-one, maximum-plus-one, `long.MinValue`, `long.MaxValue`, missing values, and wrong-oneof values before callback invocation. |

The 2026-07-17 consistency audit also corrected stale narrative text about the
sole API-table getter, the distinct `u64` native function-call and `u32`
callback-correlation identities, language-neutral shared corrections, the A4
recursive-alias decision, and the historical status of the dry-run checklist.
These are documentation reconciliations of already-recorded decisions; they
do not promote any B- or C-series gate.

A later independent second audit reopened A3, A5, A8, B1, B3, B5, B7, B8,
B10, and B11 after finding executable false-positive paths and public-contract
gaps. Their earlier outputs remain useful diagnostics but are not acceptance
evidence until the corrected sources, assertions, hashes, and clean runs are
recorded below.

### A-series evidence log

#### A2 — canonical API table, Linux x64

- Target: `1ebf901f7896faaec4672fdc4b2f2835db2f1cc0`; .NET SDK
  `10.0.110`; native product version `0.15.0`.
- Authority: `baml_language/crates/bridge_cffi/include/baml_cffi.h` declares
  `baml_get_api_v1` as the only symbol a dynamically loaded host bridge needs
  and defines the append-only `BamlApiV1` required prefix through
  `register_bridge`.
- Current native revalidation:
  `cd baml_language && env RUSTC_WRAPPER= cargo build -p bridge_cffi --release`.
  The immutable isolated artifact is
  `/root/baml-current-native-evidence.NGfRFQ/libbridge_cffi.so`,
  20,961,256 bytes, SHA-256
  `cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`.
- Probe:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiProbe`.
  `dotnet build
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiProbe/Baml.Bridge.AbiProbe.csproj
  --configuration Release --nologo -p:NuGetAudit=false` completed with zero
  warnings/errors.
  `dotnet run --project
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.AbiProbe/Baml.Bridge.AbiProbe.csproj
  --configuration Release --no-build --no-restore --
  /root/baml-current-native-evidence.NGfRFQ/libbridge_cffi.so 0.15.0`
  reported `api_v1_size=176`, `product_version=0.15.0`, and
  `csharp_registration=ok`.
- Conclusion: Q1's per-operation direct-import premise was incompatible with
  the current public ABI and is explicitly amended in `TASK/design.md`. This
  closes getter/table feasibility on Linux x64, not B1 ownership/race/default
  package resolution or C6's host/RID matrix.

#### A3 — versioned cross-assembly generated-code seam

- Probe:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.GeneratedCodeContractProbe`.
  The runtime probe package defines the exact proposed public CLR seam in
  `Baml.Generated.V1`; every generated-only declaration is hidden with
  `EditorBrowsable(Never)`. Registry/type/function/argument/result-binding
  declarations are reference-bound, not trusted numeric or string claims.
- Fresh pack command (with fresh `NUGET_PACKAGES`, `NUGET_HTTP_CACHE_PATH`,
  `DOTNET_CLI_HOME`, feed, and artifact directories under
  `/tmp/baml-a3-final.m1lxmO`):
  `dotnet pack
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.GeneratedCodeContractProbe/Runtime/Baml.Bridge.GeneratedCodeContractProbe.Runtime.csproj
  -c Release --nologo --artifacts-path
  /tmp/baml-a3-final.m1lxmO/runtime-artifacts -o
  /tmp/baml-a3-final.m1lxmO/feed`.
  Run-specific raw OPC package SHA-256:
  `8aceace2f5e23e10fc7596d495fae02bb30a2b2194d206f334df9e845e379672`.
  Its inspected nuspec contains exact ID
  `Baml.Bridge.GeneratedCodeContractProbe.Runtime` and version `0.0.0-a3`;
  the archive has six entries and one `lib/net10.0` runtime DLL.
  This identifies that inspected input only. NuGet's raw OPC metadata is
  nondeterministic, so it is not a reproducible release identity; normalized
  unsigned-package identity is established by B4.
- The unrelated consumer has the exact package range `[0.0.0-a3]`,
  `RestoreSources=$(A3LocalFeed)`, `TreatWarningsAsErrors=true`,
  `IsTrimmable=true`, and `EnableTrimAnalyzer=true`. Its fresh artifact-path
  Release build restored from only the new local feed and isolated package
  directory, then completed with zero warnings and zero errors.
- Both the ordinary exact-package execution and a `PublishTrimmed=true`,
  `TrimmerSingleWarn=false`, `ILLinkTreatWarningsAsErrors=true` execution
  reported:

  ```text
  cross_assembly_generated_codec=full_v1_representative_ok
  cross_assembly_generated_dispatch=sync_async_generic_optional_self_request_stream_ok
  generated_token_negatives=cross_builder_duplicate_default_frozen_contradictory_ok
  generated_async_cancellation=status_canceled_exact_token_ok
  generated_program_fingerprint=039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81
  generated_contract_version=1
  generated_runtime_version=0.0.0-a3
  generated_bridge_version=bridge-v1
  ```

  The trimmed publish used the exact local probe feed plus official NuGet only
  for the .NET runtime/host packs that a self-contained trimmed publish needs.
- The generated application call path contains no raw BAML function or
  argument wire names. Generated registration creates opaque ordinary,
  generic-result, receiver, build-request, and stream partial/final tokens;
  calls accept those tokens plus provenance-bound frozen argument sets.
  Required/optional arguments, explicit null, result declaration identity, and
  generated/runtime/required-bridge versions all fail closed.
- Generated field-by-field codecs cover null, bool, checked exact-range int,
  finite float, bigint, string, copied bytes, list, map, exact-wire enum,
  nominal object/class, active-case union, dynamic, copied media, and copied
  handle metadata. A nested `Envelope<Person>` round trip composes generic
  class, list, map, enum, union, and optional shapes and proves byte/media
  mutation isolation.
- Fail-closed vectors cover same-ID/same-identity tokens from another builder,
  duplicate BAML type and function+variant identities, default type/function/
  stream/argument/result-parameter/result-binding tokens, frozen builders,
  cross-function arguments, contradictory generic bindings, fingerprint
  mismatch, and all three compatibility versions. Pre-canceled async dispatch
  finishes with `TaskStatus.Canceled` and the exact supplied token.
- `rg` found no reflection, `Activator`, Protobuf, `InternalsVisibleTo`, or
  `typeof` dependency in either project. Final source SHA-256 values:
  runtime source
  `5c3c0fb76790b19af4920de27bd4654f4aefea3116b71df88c37c451ee331d48`,
  runtime project
  `5311ce969c9c6a8a64c2444de0fdb5dd0eb56c4459357708ebfc287547fd7673`,
  consumer source
  `f0edf6e5fc4ab545ffbcad762b2cd846d4ab83900fe0c0077ae95dc87d6536cb`,
  and consumer project
  `10690041645e358ceee32199db455f32233de03bb9dabbf6b2c9403291f9b623`.

#### A7 — exact literal-arm producer selection

- Changed:
  `baml_language/crates/bex_engine/src/conversion.rs` and
  `baml_language/crates/bex_external_types/src/host_return.rs`.
- `cd baml_language && env RUSTC_WRAPPER= cargo test -p bex_engine
  literal_union_selection_tests --lib -q`: 4 passed.
- `cd baml_language && env RUSTC_WRAPPER= cargo test -p bex_external_types
  literal_value_equality --lib -q`: 1 passed.
- Exact integer, bigint, float, string, and bool literal equality now controls
  arm selection/validation.
- `cd baml_language && env RUSTC_WRAPPER= cargo test -p bex_engine
  outbound_cffi_envelope_records_the_exact_selected_literal --lib --
  --nocapture` records the current 40-byte CFFI envelope as
  `6a2622143a120a0642040a026c660a0842060a0463726c662a062263726c662232061a0463726c66`.
- `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProtocolProbe`
  pins `Grpc.Tools 2.82.0` and `Google.Protobuf 3.35.1`, generates all four
  canonical schemas internally, parses those exact Rust-produced bytes,
  returns `crlf`, and rejects a cloned envelope changed to selected `"lf"`.
  Release build completed with zero warnings/errors; execution reported
  `exact_literal_union_decode=ok`, `wire_bytes=40`, and
  `contradictory_metadata=rejected`.

#### A4 — recursive aliases

- Authority:
  `baml_language/crates/baml_project/src/client_codegen.rs` records recursive
  aliases and preserves `TypeAlias` back edges.
- Regression:
  `cd baml_language && env RUSTC_WRAPPER= cargo test -p baml_project
  recursive_alias_shapes_reach_codegen_as_finite_named_graphs --lib -q`:
  1 passed. It compiles and asserts finite graph shapes for direct `Direct =
  Direct`, mutual list/map recursion, nullable recursion, and union/list
  recursion.
- Decision: acyclic aliases keep Q18's erased CLR projection. Every recursive
  alias strongly connected component is deliberately unsupported in C# v1
  with one targeted, qualified, cycle-aware generator diagnostic before the
  whole-directory output transaction. This is recorded in `TASK/design.md`
  and the recursive-alias capability row is `unsupported`, not `blocked`.

#### A5 — optional host-callable binding

- Wire authority:
  `baml_language/crates/bridge_ctypes/types/baml_bridge/cffi/v1/baml_outbound.proto`.
  `BamlToHostCall` contains only supplied arguments; optionals carry
  `arg_name` and `is_optional_arg`.
- Producer authority:
  `baml_language/crates/bridge_ctypes/src/value_encode.rs` constructs the five
  cases through the production `build_to_host_call` path and the ignored
  `emit_csharp_optional_host_call_vectors` test serializes the resulting Prost
  messages. The five-line vector SHA-256 is
  `8950193938db37064a5488edf237a04ba817470b1651e8185d247b3aaadcedf5`.
- Probe:
  `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProtocolProbe`.
  Its generated descriptor model preallocates `first` and `later` as unset,
  maps supplied names to those typed slots, distinguishes explicit null, and
  invokes
  `Func<long,BamlOptional<string?>,BamlOptional<long>,CancellationToken,Task<string>>`
  in declaration order.
- `env NUGET_PACKAGES=/tmp/baml-csharp-protobuf-nuget dotnet build
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProtocolProbe/Baml.Bridge.ProtocolProbe.csproj
  --configuration Release --no-restore --nologo`: zero warnings/errors.
- `BAML_CSHARP_OPTIONAL_HOST_CALL_VECTORS=/tmp/baml-csharp-optional-host-call-vectors-final.tsv
  RUSTC_WRAPPER= cargo test -p bridge_ctypes
  value_encode::tests::emit_csharp_optional_host_call_vectors --lib --
  --ignored --exact --nocapture` passed and reproduced the digest above.
- The warning-free Release output was then run read-only in the official .NET
  10 Noble SDK image pinned at digest
  `sha256:548d93f8a18a1acbe6cc127bc4f47281430d34a9e35c18afa80a8d6741c2adc3`,
  with that vector file mounted as `/vectors.tsv`. It reported
  `optional_callback_slots=5/5`,
  `malformed_optional_callback_slots=6/6`, and
  `rust_produced_optional_callback_slots=5/5`. The malformed set executes
  missing required, required-flagged-optional, unknown name, duplicate name,
  wrong first-argument type, and null supplied to a nonnullable later slot.
- The 2026-07-17 second-audit correction used one
  `DecodeHostCallbackInt` path for both required and present-optional integer
  carriers and delegated its domain check to `DecodeBamlInt`. An isolated
  restore plus warning-free Release build, followed by a run with the same
  Rust-produced vector file, reported
  `callback_baml_int_valid=4/4`,
  `callback_baml_int_rejected=12/12`, and
  `callback_baml_int_fail_closed=ok`. The valid set covers the exact minimum
  and maximum in both callback positions. The rejected set covers
  minimum-minus-one, maximum-plus-one, both `long` extremes, missing value,
  and wrong-oneof value in both positions; a counting callback remained at
  zero invocations.
- The manual workflow carries the exact Rust-emitted vector artifact to every
  Linux, macOS, and Windows protocol builder. Atomic attempt
  [29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216)
  passed all three builders and the byte-identity fan-in; the complete atomic
  run remains required before this local status is promoted.

#### A6 — current standard-library resource/client/prompt inventory

- Authorities:
  `baml_language/crates/baml_builtins2/baml_std/boundary/core.baml` and
  `baml_language/crates/baml_builtins2/baml_std/baml/ns_{csv,fs,glob,http,llm,media,net,spawn}`.
- `rg -n '\\$rust_type'
  crates/baml_builtins2/baml_std/baml
  crates/baml_builtins2/baml_std/boundary` found 25 Rust-backed fields in 24
  classes. Every containing identity has its own row or explicit grouped media
  identity split into four rows in
  `TASK/state-of-csharp-completeness.md`.
- The same ledger separately classifies `baml.http.Request`,
  `baml.llm.Stream<TPartial,TFinal>`, all prompt/context/orchestration helpers,
  `Client`/`ClientType`/`RetryPolicy`, provider option/primitive-client
  internals, `CodegenTy::Resource`, the host-callable exception handle, and
  ordinary standard-library option/result values.
- Public managed values are limited to `BamlHttpRequest`, `BamlClient` plus its
  enum/retry metadata, the four immutable media types, and the resolved stream
  controller. Known stateful pass-through resources use `BamlHandle`.
  Prompt/provider/cache internals are unsupported in direct user signatures.
  A new/unclassified raw `RustType` FQN is a generator error, not a convention
  for manufacturing another wrapper.

#### A8 — semantic integer bounds

- `baml_language/crates/bex_vm_types/src/types/value.rs` defines
  `INT_MIN = -4_611_686_018_427_387_904` and
  `INT_MAX = 4_611_686_018_427_387_903` and enforces them in `Value::try_int`.
- `baml_language/crates/baml_compiler2_tir/src/lib.rs` mirrors those constants;
  `baml_language/crates/bex_engine/src/conversion.rs` enforces them at host
  conversion.
- Inbound/outbound protobuf fields remain `int64`, intentionally wider than
  the language domain.
  `baml_language/crates/baml_tests/tests/bigints.rs` covers
  rejection at `2^62` and success at `2^62-1`.
- `baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProtocolProbe`
  implements checked inbound `InboundValue` encoding and outbound
  `BamlOutboundValue` decoding. It covers min, max, adjacent/interior values;
  rejects min-minus-one, max-plus-one, `long.MinValue`, and `long.MaxValue` in
  both directions; validates integer literals; and propagates exact index paths
  through list codecs. Required and present-optional host-callback integers
  also flow through that same checked outbound decoder; callback-specific
  negatives cover every out-of-domain carrier and malformed missing/wrong
  value case at both positions and prove no host dispatch occurs.
- `env NUGET_PACKAGES=/tmp/baml-csharp-protobuf-nuget dotnet build
  baml_language/sdks/csharp/bridge_csharp/tests/Baml.Bridge.ProtocolProbe/Baml.Bridge.ProtocolProbe.csproj
  --configuration Release --no-restore --nologo` completed with zero
  warnings/errors. The exact `dotnet run` command recorded under A5 reported
  `baml_int_vectors=encode_decode_checked`,
  `callback_baml_int_valid=4/4`,
  `callback_baml_int_rejected=12/12`, and
  `callback_baml_int_fail_closed=ok`.

### Implementation-document entry consistency audit

The final local consistency audit is complete, but it does not bypass the
committed-source external gate:

- a structural heading audit finds exactly one detailed decision for each
  question 1–20 and a resolved-summary entry for every question; question 15
  intentionally has four callable-surface summary parts;
- the four Python capability tables contain 14 call-form, 6 runtime-behavior,
  32 value, and 14 compatibility rows. The corresponding C# tables contain 14,
  9, 33, and 14 nonempty rows: the extra C# rows are the required explicit
  type-mismatch/cancellation splits and recursive-alias classification rather
  than missing or merged Python identities. The standard-library and
  C#-specific tables add 48 and 47 nonempty rows respectively;
- the managed public-contract audit contains 21 nonempty categories covering
  the complete public runtime/generated inventory, including the four media
  types and exception hierarchy as grouped contracts;
- the v1 placeholder scan finds no unresolved `TBD`, `to taste`, `where
  possible`, `if retained`, or “decide during implementation” decision.
  Remaining `object?` occurrences are ordinary `Equals` overrides or explicit
  prohibitions/unsupported boundaries;
- the A2 API-table probe was rerun after all current-run native corrections
  against the immutable 20,961,256-byte artifact with SHA-256
  `cdb5bcbe5b23ab973953a4ec000e0d37413741c594d2b3c0365a0278e9be06ad`;
  it again passed warning-free with the exact 176-byte table prefix, product
  version, and bridge registration; and
- every reachable external action reference in the manual caller, native
  builder, verifier, and shared Rust setup is an exact commit SHA. Repository
  `actionlint`, independent YAML parsing, Rust formatting, and diff checks
  pass.

The external workflow must still reproduce and promote B3/B4/B8/B11/B12 and
C6 from one reviewed, committed source SHA. Until that result is recorded,
`TASK/implementation.md` remains intentionally absent.

The first remote dispatch attempt against the feature branch returned GitHub
Actions `404` even though the file bytes were present at the remote commit.
GitHub's
[manual-run documentation](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)
requires `workflow_dispatch` files to exist on the default branch. The caller
therefore retains manual dispatch for normal post-merge use and adds an
exact-name tag-push bootstrap:
`csharp-entry-gates-<full source SHA>`. The plan job verifies that the tag and
checked-out SHA agree before producing any artifact. This is trigger
availability hardening, not a relaxation of B4 provenance or publication
authority.

## B. Compiled design evidence

| ID | Status | Gate | Required outcome |
| --- | --- | --- | --- |
| B1 | passed locally | Q1 actual-ABI interop/lifetime probe | `TASK/abi-lifetime-evidence.md` records the actual ABI/lifetime slice plus a fresh-cache exact-package run of package-default and absolute-override success, eight fail-closed invalid override/table cases, and product-version mismatch. The repaired negative driver no longer catches its own “unexpectedly loaded” sentinel; a valid-load-in-failure-mode regression now exits nonzero with the exact sentinel, proving silent fallback cannot pass. Final product registry/SafeHandle races and the real eight-RID runner matrix remain implementation/external gates. |
| B2 | passed locally | Q8 union layout | Current .NET 10/C# 14 Release probe covers arities 2/8/16/32 for reference, primitive, enum, `BigInteger`, generated-class-shaped, and mixed arms; asserts duplicate closed cases and invalid default; and records exact sizes, copy cost, allocations, source hashes, commands, runtime, and the selected one-field-per-arm layout in `TASK/union-layout-evidence.md`. |
| B3 | passed locally | Q9 protocol generation/package | A fresh current-source package and fresh-cache exact-package consumer build pass warning-free. The fail-closed audit inspects the complete public signature graph with no generated/Google.Protobuf exposure and requires a private generated-message edge as positive control. Atomic attempt [29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216) additionally proved four generated sources byte-identical across Linux x64, macOS ARM64, and Windows x64; exact hashes are in `TASK/csharp-entry-gates-handoff.md`. Status remains local until the complete atomic run passes as required by the handoff. |
| B4 | blocked | Q10 real atomic package | Atomic attempt [29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216) passed all eight producers and deterministic assembly of the exact 15-entry, 68,548,097-byte package, SHA-256 `9195e1dd1cf8886c68d4f07bfa2ee87049537cb2787f73b04d6655036883b029`, under the 200,000,000-byte ceiling. Four native consumer jobs passed. Both Windows consumers also passed restore/publish/ABI/RID execution before GNU's escaped-filename marker caused a post-run checksum text mismatch; both musl jobs failed before restore because Docker did not receive the required package mode. Protocol and semantic/deployment fan-in passed; completeness skipped. `TASK/package-feasibility-evidence.md` records every native/diagnostic/package measurement. B4 remains blocked on the focused verifier repairs and one complete exact-source attempt. |
| B5 | passed locally | Public managed-type contract audit | The repaired warning-free managed probe freezes the public `BamlTypeDescriptorKind`/`BamlValueKind` split and enum numbers, exact descriptor/reflection surface, public dynamic enum/class/union inspection, nullable decoding, owned collection decode, and the no-public-union-tag contract. B7 reconciles Q16 to the real wire, and A3 proves the sole editor-hidden, versioned, provenance-bound generated dispatch/carrier seam through ordinary and trimmed exact-package consumers. Raw bootstrap/dispatch remains internal. Final product public-surface inspection, lifecycle parity, and cross-RID execution remain implementation/external gates. |
| B6 | passed locally | Generic/nullability compile matrix | `TASK/managed-contract-evidence.md` records the checked-in fail-closed executor: a warning-free isolated positive build/run, seven separately restored/built negative fixtures with exactly one assigned compiler diagnostic (`CS0411`, `CS1503`, or `CS8625`) and no unrelated diagnostics, plus an isolated unknown-case rejection with only `BAMLGEN001`. Runtime vectors cover canonical/noncanonical numerics and collections, generated generic/class+method scopes, nullable-reference reification, redundant wrappers, maps, unsupported conveniences, and path/replacement diagnostics. Product generated calls remain implementation work. |
| B7 | passed locally | Q16 failure/cancellation identity | `TASK/failure-cancellation-evidence.md` records a warning-free fixture aligned to the current outbound envelope: decoded thrown/panic value identity, immutable ordered rendered trace lines, nullable call-context/value-derived identities, no invented trace-frame/type-mismatch/panic fields, and exact panic exit discriminator/code handling. It also preserves all three cancellation origins with frozen enum numbers, canceled-task subtype/token identity, callback EDI identity and unrelated-token faulting, a single-winner 64-signal terminal race with exact releases, and bounded child-only hard exit. Actual product envelope decoding, native registry integration, and shared parity remain implementation gates. |
| B8 | passed locally | Q17 stream/backpressure | `TASK/stream-media-abi-evidence.md` records warning-free direct actual-ABI and trimmed exact-package executions. Atomic attempt [29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216) repeated the exact package lane successfully: two positive/seven negative union metadata cases, ordered strict extensions, exact 789-byte final SHA-256 `2e950ddb…`, one completion per pull, zero unsolicited idle completions, exact cold/final/cancellation/release behavior, and bounded callbacks. Status remains local until the complete atomic run passes; final product parity remains implementation work. |
| B9 | passed locally | Q18 media restoration | `TASK/stream-media-abi-evidence.md` records 17 actual native calls covering image/audio/PDF/video as BAML-created and round-tripped URL/base64/file values. It proves the real nominal-class/`_data`/typed-handle envelope, Unicode/MIME/representation preservation, eager file and byte ownership, clone independence, 79 exact buffer releases, and handle cleanup on decode failure. No protocol amendment is required; final public media types and cross-RID product parity remain implementation work. |
| B10 | passed locally | Q18 dynamic/type translation | A fresh warning-as-error Release build/run constructs all 14 payload kinds and freezes the separate 15-case descriptor-kind enum. It proves explicit nullable-only null decode, ambiguous reference-null rejection, canonical list/map decode to owned read-only snapshots, dynamic empty/heterogeneous `Unknown` descriptors without null/first-item inference, exact public enum/class/zero-based-union inspection with wrong-kind outs, supplied-only alias/literal text, exact descriptor argument/FQN rules, and reflection/numeric ABI audits. Product adapters, parity, trim, and cross-RID execution remain implementation/external gates. |
| B11 | passed locally | Q19 trim/single-file | The local warning-as-error Linux x64 matrix covers ABI/lifetime, media, stream, dynamic/value/generic, reflection, RID policy, and all four single-file shapes. Atomic attempt [29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216) repeated that full matrix from the exact normalized package; every sidecar and four extracted/sidecar natives were byte-identical to package digest `e545e6d…`. Status remains local because B4 and complete atomic fan-in remain required. |
| B12 | passed locally | Q19 NativeAOT rejection | Local normal JIT and exact-package builds reject `PublishAot=true` before compilation with exactly `BAML0019`, no application artifact, and no escape property. Atomic attempt [29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216) repeated `BAML0019` against the real eight-RID package while its normal JIT/deployment lane passed. Status remains local until complete atomic fan-in; final product package integration remains implementation work. |
| B13 | passed locally | Q20 canonical byte array | `TASK/program-bootstrap-deployment-evidence.md` proves the realistic current 683,918-byte compiler payload and a checked-in deterministic 16 MiB compiled/executed lower-bound fixture: exact single private hexadecimal array/fingerprint, byte-for-byte compiled identity, no alternate carrier, deterministic re-emission, actual initialization/lifecycle failures, and normal/single-file deployment. The 16 MiB result is explicitly not a maximum; 32/64 MiB compiles exceeded this audit host's ~23 GiB Roslyn capacity, and no arbitrary 8 MiB product ceiling was introduced. Product generator/runtime integration remains implementation work. |

## C. Generator, parity, packaging, and release gates

| ID | Status | Gate | Required outcome |
| --- | --- | --- | --- |
| C1 | not started | Narrow end-to-end slice | On the target branch: generate into an existing net10.0 project, reference exact `baml-bridge`, initialize canonical bytes, resolve packaged native runtime, and complete sync/async primitive calls through `sdk_test_csharp`. |
| C2 | not started | Typed naming/routing | Typed request allocation precedes rendering. Prove 100 order permutations, source/wire separation, generated-local reservations, namespace qualification, member collisions, case-insensitive paths, Windows device names, and injected hash-prefix/full-hash collisions. |
| C3 | not started | Whole-directory atomic regeneration | Stage/validate/replace the wholly owned `baml_client/` directory, write deterministic manifest and byte hashes, preserve last complete output on failure, safely clean interrupted staging, and never modify user code outside the boundary. Test committed-output and clean CI-generated workflows. |
| C4 | not started | Python parity enforcement | Every Python capability identity appears in `state-of-csharp-completeness.md`; shared tests retain recognizable names/cases. `supported` requires the matching `sdk_test_csharp` path. C#-specific ABI/layout/package/path/exit tests remain additional. |
| C5 | not started | Project/package consumers | Run project-reference development fixtures and clean exact-package consumers. Prove no CLI, `baml_src`, `.proto`, `Grpc.Tools`, loose bytecode, generated project, or repository path is required at runtime/downstream. |
| C6 | blocked | Platform contract and exact package matrix | `release/platforms.json` remains the sole RID/native/runner authority. Atomic attempt [29785957216](https://github.com/BoundaryML/baml/actions/runs/29785957216) assembled all eight assets and passed native consumers on both Apple plus both glibc runners. Both Windows runners also executed the selected asset and RID policy successfully before a post-run checksum parser mismatch; both musl jobs stopped before restore because package mode was not forwarded into Docker. The local repairs preserve the all-eight requirement and exact no-substitution policy. C6 remains blocked until all eight jobs and final completeness pass together. |
| C7 | blocked | Immutable publish pipeline | Build once, normalize/inspect, sign if applicable, verify exact bytes in clean consumers, publish those exact bytes from a non-compiling trusted-publisher job, record package identity/version/digest in release metadata, and make release completion depend on NuGet publication. |
| C8 | blocked | NuGet administration | BoundaryML organization owns `baml-bridge`; trusted publisher and least-privilege identity are configured and recorded. No individual account owns the permanent package identity. |
| C9 | not started | Post-publish smoke | Fresh public-registry cache restores the published version and executes a representative generated consumer with normal RID resolution. |
| C10 | not started | Canonical C# documentation | Complete the design's final documentation phase. Compile all examples with nullable analysis/warnings as errors and run credential-free examples. Document canonical style, supported deployment matrix, ownership, DI/mocking, callbacks, streams, dynamic values, generics, and explicit non-goals. |

## Promotion rule

An implementation phase is complete only when:

1. its design reconciliation is settled;
2. its narrow unit/compile tests pass;
3. its matching shared parity tests pass where applicable;
4. its clean package/final-consumer proof passes where distribution or
   deployment behavior is part of the claim;
5. both this ledger and `state-of-csharp-completeness.md` are updated with the
   exact evidence.

Passing a later broad test does not excuse a missing earlier ownership, race,
negative, or packaging proof.
