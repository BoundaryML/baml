# Recommended agent goal

Deliver a production-ready C#/.NET BAML bridge on current Canary. Use
BoundaryML/baml PR #4074 only as an experimental salvage source: preserve
measured evidence and design-conformant implementation where it survives a
semantic current-Canary audit, but do not treat the PR, its dry-run design, or
its public API as authoritative.

Before editing implementation code, read every document in this task seed in
the order specified by `README.md`. Treat `design.md` as the completed
normative C# design. Do not inspect, search, read, summarize, or reference
`/engine`.

Work persistently from design through implementation, verification, packaging,
release integration, and canonical user documentation. Do not stop after
writing a plan or after obtaining one local happy-path call.

## Required operating procedure

1. Record the current target branch/commit, .NET SDK, frozen BAML version, and
   platform contract in `verification-gates.md`.
2. Perform a semantic integration audit of current Canary:
   - compiler-owned codegen types and qualified-name APIs;
   - exhaustive C# target policy for every compiler type variant;
   - actual public C ABI/header, canonical API-table entry point, and
     ownership;
   - native platform/RID contract;
   - release build/publish graph;
   - shared SDK parity harness.
3. Decide whether each experimental PR area should be salvaged, rewritten, or
   discarded. Make that decision by canonical behavior, maintainability,
   ownership, performance evidence, and current interfaces—not by minimizing
   diff size.
4. Resolve and record every A-series reconciliation in
   `verification-gates.md`, especially:
   - source-generated canonical getter plus typed API-table feasibility;
   - a trim-safe cross-assembly generated codec/registration seam;
   - finite recursive-alias behavior;
   - optional host-callable argument omission with canonical Task-returning
     `Func` delegates;
   - explicit standard-library resource classification;
   - literal-union metadata correctness;
   - exact BAML integer bounds.
5. Complete the design's required pre-implementation probes. Compiled evidence
   may disprove a design decision; if it does, amend the canonical design and
   both ledgers explicitly before implementing the replacement. Never let an
   implementation shortcut silently become the new contract.
6. Produce a dependency-ordered implementation document from the settled
   design. Name concrete files/components, tests, clean-consumer fixtures,
   package artifacts, acceptance criteria, and stop conditions for each phase.
7. Build the typed identity, naming, and routing foundation, then implement
   the first functional slice as a narrow end-to-end primitive free function:
   deterministic C# generation into an existing net10.0 project, exact
   generated/runtime/native versioning, one canonical hexadecimal byte-array
   bootstrap, packaged RID resolution, one source-generated
   `baml_get_api_v1` import plus validated typed API-table calls, and sync/async
   execution through `sdk_test_csharp`.
8. Bring exact package-reference consumption forward immediately after the
   narrow slice. Do not defer native asset selection, clean restore, version
   skew, or consumer-only build behavior until feature work is finished.
9. Expand in dependency order through naming/routing coverage and atomic
   regeneration, optionality/nullability, nominal values, unions, generics,
   failures/cancellation, companions, callbacks, streams,
   handles/resources/media, dynamic values/remaining type translations,
   trimming/single-file, and the deliberate NativeAOT rejection.
10. Port Python/shared capability identities and tests continuously. Keep
    C#-specific ABI, layout, path, package, trim, and hard-exit tests in
    addition to parity tests.
11. Update `state-of-csharp-completeness.md` and `verification-gates.md` in the
    same work that changes support. A row becomes `supported` only when its
    named current-branch parity/final-consumer evidence passes.
12. Finish the immutable one-package NuGet pipeline: real required RID assets,
    architecture/dependency/export inspection, deterministic normalization,
    signing order if applicable, exact consumer verification, non-compiling
    trusted publication, release-manifest integration, and post-publish clean
    smoke.
13. Finish the canonical C# documentation phase from the design. Compile all
    examples with nullable analysis and warnings as errors; run every
    credential-free example.

## Correctness and API constraints

- Target .NET 10/C# 14 only.
- Use namespaces plus the static `Functions` holder for free functions.
- Use one atomic `baml-bridge` NuGet package and ordinary .NET RID resolution.
- Use exactly one source-generated
  `[LibraryImport("bridge_cffi", EntryPoint = "baml_get_api_v1")]` getter, one
  assembly-owned resolver, and calls through the validated typed API table
  unless current compiled evidence causes an explicit design amendment.
- Preserve typed BAML identity, wire identity, and projected C# identity
  independently throughout naming, codecs, hashing, routing, and diagnostics.
- Emit deterministic `.g.cs` directly into the wholly generator-owned
  `baml_client/` directory of an existing project. Do not generate a project,
  assembly, source generator, automatic build-time generation target, resource,
  loose bytecode, or Base64 carrier.
- Keep the selected one-field-per-arm `BamlUnion<T...>` semantics, including
  duplicate CLR projections, explicit case identity, invalid default, and
  `FromTn` as the authoritative overlapping-arm constructor.
- Preserve unconstrained native C# generics and the exact `BamlOptional<T>` /
  `BamlNullable<T>` distinctions.
- Use generated trim-safe codecs/factories. Do not fall back to arbitrary
  reflection, `Activator` member discovery, serializers, `dynamic`, or
  `object?`.
- Preserve the complete exception hierarchy, structured diagnostics, exact
  managed callback exception identity, cancellation token/origin/task state,
  late-result cleanup, and hard process exit.
- Preserve the cold single-consumer stream, multiple cached final waiters,
  final-only mode, token domains, bounded ordered lossless delivery, and
  exactly-once cleanup.
- Keep media as immutable managed URL/owned-byte values and opaque resources as
  `BamlHandle` unless the canonical public inventory is explicitly amended.
- Support normal/trimmed JIT and normal/trimmed single-file. Reject NativeAOT
  with targeted `BAML0019`.
- Never add silent fallback, source-tree probing, discovery-order naming,
  allocation-order suffixes, inferred wire names, first-compatible union
  selection, permissive map-key stringification, or arbitrary CLR type
  inference.

## Completion condition

The goal is complete only when:

- every resolved design decision is implemented or explicitly amended from
  current compiled evidence;
- every Python capability row has an honest C# status and test identity;
- every supported row has its required passing parity or final-consumer proof;
- all C# design evidence and release gates are passed, including real RID
  package execution, trimming/single-file, and negative NativeAOT;
- the exact verified package is publishable/published through the approved
  release flow and passes its post-publish consumer smoke;
- canonical idiomatic C#-BAML documentation and executable examples are
  complete;
- no required work remains hidden only in chat, an experimental log, or an
  untracked implementation assumption.

If blocked only by external administration or unavailable release hardware,
finish every safe in-repository task, record the exact blocker/evidence/owner
in `verification-gates.md`, and leave a reproducible handoff rather than
weakening or pretending to satisfy the gate.

Do not actually publish a NuGet package, push a branch, open or merge a pull
request, alter trusted-publisher/registry configuration, or trigger a
production release unless the user separately authorizes that external action.
Implement and verify the repository pipeline as far as local/CI-safe evidence
allows, then record any external execution gate precisely.
