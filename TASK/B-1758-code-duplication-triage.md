# B-1758: duplication and dead-code triage

Reviewed against commit `5463838df031118d82f2640aa3a30949e79fd3f6` on 2026-09-22. Source: [B-1758](https://linear.app/boundaryml2/issue/B-1758/triage-code-duplication-and-dead-code-report), including its description, both comments, and the embedded report screenshot. The comments contain the Slack synchronization notice and issue-created notification, with no additional requirements. The screenshot is the available report; it does not provide the underlying detector output or enumerate all 14 dependency claims.

The issue requests review and triage. This change records findings and bounded follow-ups; it does not remove dependencies, change runtime behavior, migrate APIs, or start a workspace-wide refactor. Paths below refer to the reviewed revision, and all findings are source-level unless explicitly described as a command result.

## Disposition

| Reported finding | Result | Recommended disposition |
| --- | --- | --- |
| 14 dependencies never imported, including `baml_type` in three crates | The three named declarations are credible cleanup candidates. The total cannot be reconciled without the original inventory; the current scanner emits 71 findings of several kinds. | Audit small groups before removal; establish a reviewed baseline before a CI gate. |
| `vm_args_to_external`, `receiver_input_type`, `STABLE_VERSION`, `EXIT_BUILD_FAILED` are dead | Two unused internal helper candidates; one tooling-consumed constant; one unused exit-code constant with a behavior question. | Remove helpers in a small cleanup; retain the version stamp; decide the size-gate exit-code contract separately. |
| Playground host duplicated between native and Wasm | Confirmed overlap, including a nearly identical diagnostic builder. Other similarly named functions have different behavior or platform requirements. | Share selected builders after defining ordering, clocks, and wire-type conversions. |
| SDK type traversal duplication and missing `Interface`/`Future` recursion | Recursion differs, but the compared helpers serve different purposes. Missing branches alone do not establish a user-visible defect. | Add targeted supported-type reproductions before changing traversal semantics. |
| Java/Rust route `ai` and `reflect` differently from TS/Python | Confirmed routing-policy difference. | Resolve the intended public layout and add routing tests before relocating generated APIs. |
| C# test crate has not adopted the common harness | Confirmed; it also has a different fixture corpus and distinct assertions. | Extract process plumbing first, preserving each test and special case. |
| Node/Python reimplement `bridge_cffi::handle` | Overstated: media and handle behavior already delegates through the C ABI; repeated marshalling is real. Host-value registries have a different purpose. | Consider direct safe-core calls for media/handles; retain language-specific host-value ownership. |
| Mechanical duplicates: hash, string escaping, test names, fixture loops, media kinds | Mostly confirmed, with corrections to exact-copy and count claims. | Optional small cleanups; do not add production APIs solely to remove tiny test helpers. |

## Dependencies and dead code

### Dependencies

`baml_type` occurs only in the manifest within each of [baml_fmt](../baml_language/crates/baml_fmt/Cargo.toml), [baml_lsp](../baml_language/crates/baml_lsp/Cargo.toml), and [baml_lsp_server](../baml_language/crates/baml_lsp_server/Cargo.toml). Searches of those crate trees found no source imports or qualified references. The declarations do not enable special features, none of these crates has a build script, and their manifest feature lists do not refer to `baml_type`. These are high-confidence removal candidates, not compile-validated removals. A follow-up should check all targets of the affected crates, native crypto configurations, and the Wasm consumers of `baml_lsp` before deleting them.

Running `cargo shear --format json` from `baml_language/` returned exit 1, with 68 errors, 3 warnings, and 0 fixes:

| Scanner category | Count |
| --- | ---: |
| `unused_dependency` | 62 |
| `unused_workspace_dependency` | 5 |
| `misplaced_dependency` | 1 |
| `unused_optional_dependency` | 1 |
| `unused_feature_dependency` | 2 |

These are diagnostic counts, not a verified removal list: a crate/dependency pair can occur in multiple manifest sections. The installed tool reports `Version: dev`, and [mise.toml](../mise.toml) selects `latest`, so the counts are not a reproducible version-pinned baseline. The screenshot supplies neither the identities of the other 11 entries nor a tool version. Its total of 14 should not be used as an acceptance criterion. The issue description also mentions undeclared-import gaps, but the screenshot provides no concrete example, and this scan does not prove their existence or absence.

There are concrete reasons not to bulk-apply the scanner's fixes:

- [bex_cache/Cargo.toml](../baml_language/crates/bex_cache/Cargo.toml) uses optional `rustls` to select `ring-crypto`, including `rustls/ring` and `rustls/std`. This is deliberate feature wiring, even without direct Rust imports. Removing it changes a target configuration.
- [bex_project/Cargo.toml](../baml_language/crates/bex_project/Cargo.toml) forwards crypto features to `bex_cache`, and [bridge_python/Cargo.toml](../baml_language/sdks/python/rust/bridge_python/Cargo.toml) forwards them to `bex_project`. The two feature-only warnings require a feature-graph audit, not deletion based on textual import counts.
- [sys_types/Cargo.toml](../baml_language/crates/sys_types/Cargo.toml) already exempts `bex_str` and `num-bigint`. Its [build script](../baml_language/crates/sys_types/build.rs) generates code through [codegen_io.rs](../baml_language/crates/baml_builtins2_codegen/src/codegen_io.rs), which emits `bex_str::BexStr` and `num_bigint::BigInt`. The exemption represents generated-code usage, not evidence that the dependencies are dead.
- The presence of cargo-shear metadata in one crate and an installed tool is not the same as an existing CI audit. A repository search found no cargo-shear invocation in `.github/` or `scripts/` at this revision.

Proposed follow-up: pin a tool version, classify the baseline with reasons for generated-code and feature exceptions, remove the three named candidates with appropriate checks, then gate new findings. The remaining scanner findings require their own usage review; this triage does not certify them as unused.

### Named dead-code claims

| Symbol | Evidence | Decision |
| --- | --- | --- |
| `vm_args_to_external` | [conversion.rs](../baml_language/crates/bex_engine/src/conversion.rs), line 2145: `pub(crate)` helper with `#[allow(unused)]`; repository search finds its definition only. It maps the separately used `vm_arg_to_external` over a slice. | Confirmed source-level unused internal helper. Remove only the plural wrapper, after the engine check; do not remove the singular conversion. |
| `receiver_input_type` | [codegen.rs](../baml_language/crates/baml_builtins2_codegen/src/codegen.rs), line 2147: private helper with `#[allow(dead_code)]`. Its only other occurrence is a doc comment. Actual generation calls `receiver_input_type_with_vm_usage`. | Confirmed source-level unused wrapper. Remove the wrapper and update the adjacent documentation, keeping the VM-usage-aware implementation. This file is generator source, not generated output. |
| `STABLE_VERSION` | [baml_version/src/lib.rs](../baml_language/crates/baml_version/src/lib.rs), line 7, has no Rust caller. However, [scripts/baml-language-version](../scripts/baml-language-version), lines 663 and 866–870, writes and validates this exact field; [its tests](../scripts/tests/test_baml_language_version.py) construct it too. | False positive for deletion. Retain the stamp unless the release tooling is deliberately migrated in the same change. |
| `EXIT_BUILD_FAILED` | [tools_size_gate/src/main.rs](../baml_language/crates/tools_size_gate/src/main.rs), line 40: unused private constant `3`. Build errors propagate via `?` in `build_and_measure`; `main` maps errors to `EXIT_TOOL_ERROR` (`4`). | The constant is unused, but the intended distinction between build and tool errors is unresolved. Remove the reserved constant if exit 4 is intended, or implement/test exit 3 in a separate behavior change. Do not silently change CLI exit semantics as deduplication. |

The two helper conclusions are based on whole-repository searches, visibility, and callers; they are not proof from compiling every feature combination. None of the named helpers is a public downstream Rust API. The version example demonstrates why non-Rust consumers must still be checked.

## Playground host

`wc -l baml_language/crates/baml_lsp_server/src/playground_*.rs` reproduces the screenshot's **7,877 lines**. All Rust files under that crate's `src/` total 14,191 lines, so the “more than half” observation is accurate, including test code embedded in those files. This is not an estimate of removable lines.

The exact `log_loss_diagnostic_patch` comparison is also accurate: [native playground_server.rs](../baml_language/crates/baml_lsp_server/src/playground_server.rs), line 671, and [Wasm runs.rs](../baml_language/crates/bridge_wasm/src/runs.rs), line 620, differ only in qualification of `DiagnosticSeverity::Warning`. Their adjacent `capture_loss_message` functions match. However, `log_loss_diagnostic_patch` mutates `InMemoryRunStore`; the message formatter is pure, while the patch builder is shared state-changing behavior. A shared implementation can use the existing run types/store rather than importing either transport.

`flatten_diagnostics` is not an exact behavioral duplicate:

- [Native playground_notify.rs](../baml_language/crates/baml_lsp_server/src/playground_notify.rs), line 233, preserves document/diagnostic iteration order and produces a native notification type with borrowed severity strings.
- [Wasm playground.rs](../baml_language/crates/bridge_wasm/src/playground.rs), line 284, produces its own notification type with owned strings and sorts the result by message.

An extraction must explicitly preserve these order policies or intentionally standardize them with tests. Run outcomes also involve clocks and cancellation classification; shared constructors should accept time explicitly where deterministic behavior is needed.

Platform ownership remains real: [native playground_server.rs](../baml_language/crates/baml_lsp_server/src/playground_server.rs) uses `Arc` and Tokio tasks/broadcasting; [Wasm runs.rs](../baml_language/crates/bridge_wasm/src/runs.rs) uses `Rc<RefCell<...>>`, browser callbacks, and `spawn_local`. Sharing the generation fence or scheduler is a separate architectural task. The fact that playground behavior is hosted in `baml_lsp_server` is a placement concern, not evidence of dead code.

A simple function-name intersection over native `playground_*.rs` and Wasm `src/*.rs` gives 40 shared names, including generic names such as `new`, `from`, and `get`; it does not reproduce 49 verified duplicate functions. The original detector method is unavailable. Do not use either count as a refactor quota.

Proposed follow-up: extract the capture-loss message and diagnostic builder first, using a native/Wasm-compatible dependency location. Validate identical patches on both hosts, then address flattening with multi-document ordering tests. Keep origin checks, task scheduling, admission, cancellation, and generation ownership with their current hosts until separately specified.

## SDK generators

### Type traversal

Repeated recursion is real, but the report conflates different queries:

| Implementation | Actual purpose and coverage |
| --- | --- |
| Java [collect_type_vars](../baml_language/sdks/java/sdkgen_java/src/translate_ty.rs), line 398 | Collects deduplicated Java identifiers in first-appearance order for minted union generics. Traverses classes, lists, maps, unions, callable parameters and return types. Skips interfaces, futures, and callable `throws`. It is not named `contains_type_var`. |
| Swift [ty_contains_type_var](../baml_language/sdks/swift/rust/sdkgen_swift/src/emit.rs), line 504 | Checks whether a particular parameter can be inferred from an argument **value**. Its documentation deliberately excludes callables because they are opaque handles. Interfaces/futures also fall through. Replacing it with a universal “contains any variable” traversal would change inference behavior. |
| C# [contains_type_var](../baml_language/sdks/csharp/sdkgen_csharp/src/semantic.rs), line 1814 | Boolean structural query used by validation/emission. Traverses callable parameters, return type, and non-synthetic `throws`; excludes synthetic effect parameters from that throws check. Interfaces/futures fall through. |
| TS/Python [collect_interface_tys](../baml_language/sdks/typescript/sdkgen_typescript_shared/src/lib.rs) / [collect_interface_tys](../baml_language/sdks/python/rust/sdkgen_python_pydantic2/src/lib.rs) | Collect interface names for public tokens, including interface generic/associated types, futures, and callable throws. This is an interface-discovery query, not the same type-variable predicate. |

The missing branches are established source facts. A resulting broken generated API is not established: Java currently translates interface/future values to `java.lang.Object`; Swift's [translator](../baml_language/sdks/swift/rust/sdkgen_swift/src/translate_ty.rs) declines unsupported forms; C#'s `require_supported_type` rejects forms outside its supported set, including interfaces/futures. TS/Python likewise have `unknown`/`typing.Any` fallbacks for these forms in their type translators. A concrete supported input must show whether a skipped variable should survive into emitted types or inference.

[baml_codegen_types/src/ty.rs](../baml_language/crates/baml_codegen_types/src/ty.rs) now re-exports `baml_type::CodegenTy`; it does not own a separate enum. [baml_type_macros](../baml_language/crates/baml_type_macros/src/emit.rs) already generates `visit_heads`/`visit_heads_mut`, and [baml_type/src/template.rs](../baml_language/crates/baml_type/src/template.rs) has template traversal. These are not direct substitutes for all-node, policy-sensitive generator queries, but a new traversal should respect the existing type-family ownership rather than creating another representation.

Proposed follow-up: reproduce nested-interface, future, and callable-throws cases with focused generator tests. Then consider a shared child traversal while retaining first-appearance ordering, target naming, Swift inferability, alias-resolution policy, and C# synthetic-effect filtering in their callers. The screenshot's “38 sites across 17 files” and “600–900 lines” lack a site inventory and are not validated savings estimates.

### Package routing

Confirmed at the routing-function level:

| Package | [TS route](../baml_language/sdks/typescript/sdkgen_typescript_shared/src/routing.rs) / [Python raw_route_segments](../baml_language/sdks/python/rust/sdkgen_python_pydantic2/src/routing.rs) | [Java route](../baml_language/sdks/java/sdkgen_java/src/routing.rs) / [Rust route](../baml_language/sdks/rust/sdkgen_rust/src/routing.rs) |
| --- | --- | --- |
| local `user` | Namespace tree | Namespace tree |
| `baml` | `baml/...` | `baml/...` |
| `ai` | `ai/...` | `vendor/ai/...` |
| `reflect` | `reflect/...` | `vendor/reflect/...` |
| other external package | `vendor/<package>/...` | `vendor/<package>/...` |

For example, a routed `ai.stream.Stream` name gets `vendor/ai/stream` segments in Java/Rust and `ai/stream` in TS/Python. This is a source-derived routing result, not a claim that every generator actually emits that symbol: emission filters and runtime-owned projections can intervene. Java and Rust routing comments claiming parity with TS/Python are stale.

Proposed follow-up: choose the public namespace contract, test `ai` and `reflect` directly in all four routers, and check emitted imports/references for supported symbols. Relocating Java packages or Rust modules changes generated consumer APIs. Any shared classification must preserve target-specific escaping: Python keywords, Java identifiers, and Rust directory segments have different rules.

## C# SDK tests

The size comparison reproduces exactly: [sdk_tests/crates/csharp/src/lib.rs](../baml_language/sdk_tests/crates/csharp/src/lib.rs) is 612 lines; the other nine language crate entry files range from 13 to 90 lines. C# does not use the language suite macros in [harness_runner/src/lib.rs](../baml_language/sdk_tests/harness_runner/src/lib.rs), which has no C# suite module.

There is repeated setup validation, project-path construction, `dotnet run --no-build --configuration Release`, exit-status checking, and stdout-marker checking. The entire blocks are not interchangeable:

- The union-generator check supplies `--check` arguments; generated-client hygiene invokes Git.
- Generic negative-compilation coverage invokes a script.
- Host-callable tests share an `Output` through `OnceLock`; resource tests pass different arguments.
- Dynamic-value coverage performs `dotnet publish` before execution.
- Individual tests assert different markers, including multiple markers and documentation-consumer coverage.

[codegen/src/csharp.rs](../baml_language/sdk_tests/codegen/src/csharp.rs) explicitly documents that C# fixtures pair their own in-crate `baml_src`, `Program.cs`, and `.csproj`, unlike the shared fixture corpus. Therefore “never adopted the harness” is accurate as a current-state observation, but moving to another language's suite macro is not a mechanical replacement.

Proposed follow-up: factor a C# process helper that returns `Output`, centralize the setup guard, and keep all per-test assertions and special launch modes. Only then consider a C# suite macro. Acceptance requires unchanged test discovery and execution of the C# setup/test path, including publish, negative compilation, host-callable markers, and repository hygiene.

## Bridges

The report's host-value claim is a false equivalence. [bridge_cffi::handle](../baml_language/crates/bridge_cffi/src/handle.rs) owns ordinary engine/media handle operations; it does not store Node or Python objects. The [Node registry](../baml_language/sdks/typescript/bridge_typescript/src/host_value.rs) holds thread-safe N-API dispatch functions and coordinates JavaScript-side opaque values. The [Python registry](../baml_language/sdks/python/rust/bridge_python/src/host_value.rs) holds `Py<PyAny>` and manages Python invocation, exceptions, and release. The [Web host-value adapter](../baml_language/sdks/typescript/bridge_typescript_web/src/host_value.rs) delegates to `sys_wasm`, not to `bridge_cffi::handle`. These registries must not be replaced with ordinary handle-table rows.

There is a narrower, useful cleanup opportunity:

- [Node media.rs](../baml_language/sdks/typescript/bridge_typescript/src/media.rs) and [Python media.rs](../baml_language/sdks/python/rust/bridge_python/src/media.rs) already call `baml_media_*` C FFI functions. They repeat `CString`, output-pointer, buffer-copy/free, and status-conversion plumbing.
- [Node handle.rs](../baml_language/sdks/typescript/bridge_typescript/src/handle.rs) and [Python py_handle.rs](../baml_language/sdks/python/rust/bridge_python/src/py_handle.rs) already call `baml_handle_*` and `__testonly_seed_*`. The underlying seeding behavior is shared, though wrappers repeat ABI plumbing.
- [Web media.rs](../baml_language/sdks/typescript/bridge_typescript_web/src/media.rs) and [Web handle.rs](../baml_language/sdks/typescript/bridge_typescript_web/src/handle.rs) call the safe Rust `handle` core directly. Native adapters could do likewise without the C-string round trip.

Proposed follow-up: migrate one native adapter at a time to safe-core calls, preserving its exposed names, integer/key representation, exception classes/messages, NUL rejection, missing-media semantics, cloning, and finalization. Run handle ownership/refcount, media, rollback, and native host-callable tests. Do not count language-specific lifetime management or the intentionally repeated binding macro expansions as removable code. The report's approximate 315-line saving is not independently established.

## Smaller duplicates

| Finding | Verification and bounded next step |
| --- | --- |
| Two C# `sha256` helpers | Confirmed in [output.rs](../baml_language/sdks/csharp/sdkgen_csharp/src/output.rs), line 255, and [pipeline.rs](../baml_language/sdks/csharp/sdkgen_csharp/src/pipeline.rs), line 292. Same digest/lowercase-hex algorithm with different import placement. A private shared helper is reasonable; retain fingerprint and output-manifest tests. |
| `ts_string` and `py_string` are identical | [TS](../baml_language/sdks/typescript/sdkgen_typescript_shared/src/lib.rs), line 356, and [Python](../baml_language/sdks/python/rust/sdkgen_python_pydantic2/src/lib.rs), line 848, have the same character mapping but differ in write-error handling (`let _ = write!` versus `.unwrap()`). Share only with a documented cross-language escaping contract and tests for control characters, quotes, backslashes, and Unicode. Replacing them with JSON serialization is not automatically equivalent. |
| `name()` test helper copied 13 times | Confirmed semantic copies across TS (5), Java (3), Rust (3), and Python (2) generator test modules, with local naming/formatting differences. C++ has a fourteenth variant fixed to the `user` package. These are tiny test-only constructors; leaving them local is reasonable. A production public API solely to deduplicate them is not justified. |
| `run_all` copied 6 times | Nine definitions currently exist under [sdk_tests/codegen/src](../baml_language/sdk_tests/codegen/src). Python, Java, Swift, C++, and Rust share the same discover/assert/iterate shape; Node adds customizable paths, Go loads/stages fixtures and then stages package edges, Web uses a sibling source root, and C# discovers its own corpus. A shared corpus-check helper is plausible; whole-function equality and a uniform nine-target loop are not. |
| `media_kind_from_proto` copied 3 times | Exactly two functions with that name were found: [C FFI](../baml_language/crates/bridge_cffi/src/ffi/handle.rs), line 42, and [JNI](../baml_language/sdks/java/bridge_java/src/lib.rs), line 618. [Web media_kind](../baml_language/sdks/typescript/bridge_typescript_web/src/media.rs), line 10, is a third equivalent mapping with a different error wrapper. A shared proto-to-kind conversion could retain boundary-specific error handling. Test every discriminant, unspecified/unknown values, and generic media. |

The “about 210 lines” and “no real risk” characterization is not an acceptance criterion. Most of these are small maintainability improvements; string escaping, protocol mappings, and generated paths still have observable contracts.

## Proposed order and validation

1. Small cleanup: the two unused wrappers and the three named dependency declarations, with crate/feature checks. Keep `STABLE_VERSION`. Resolve the size-gate contract independently.
2. Potential correctness follow-ups: supported-input reproductions for traversal omissions and an explicit Java/Rust package-layout decision. Fix confirmed behavior separately from generic abstraction work.
3. Maintenance follow-ups: capture-loss helpers, C# command plumbing, and native safe-core media/handle adapters, each in its own change with existing behavior preserved and focused tests.
4. Optional cleanup: private C# hash helper and shared fixture-corpus validation. Defer test-name helpers and broader visitors until their value exceeds the new coupling.

No follow-up issues or implementation changes are implied by this ordering. It is a proposed backlog for maintainers to select, not a claim that all listed work is necessary to close this triage issue.

Validation for this report consists of reading the issue/report, repository-wide symbol searches, manifest/build-script/feature inspection, direct implementation comparisons, line counts, and a non-mutating cargo-shear scan. No runtime or SDK suite was run because the change is documentation-only; source-inferred behavior and unproven impact are identified above. Before any implementation follow-up, use its applicable crate, target, SDK, and generated-output checks rather than treating this report as removal authorization.

Useful reproduction commands from the repository root:

```sh
git rev-parse HEAD
rg -n 'vm_args_to_external|receiver_input_type|STABLE_VERSION|EXIT_BUILD_FAILED' .
rg -n 'baml_type' baml_language/crates/baml_fmt baml_language/crates/baml_lsp baml_language/crates/baml_lsp_server
rg -n 'cargo.shear' mise.toml .github scripts baml_language
cargo shear --version
cargo shear --format json baml_language
wc -l baml_language/crates/baml_lsp_server/src/playground_*.rs
wc -l baml_language/sdk_tests/crates/*/src/lib.rs
rg -n 'fn (collect_type_vars|ty_contains_type_var|contains_type_var|collect_interface_tys|sha256|ts_string|py_string|media_kind_from_proto)' baml_language
rg -n 'pub fn run_all' baml_language/sdk_tests/codegen/src
```
