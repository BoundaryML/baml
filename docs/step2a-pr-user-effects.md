# Per-PR user effects at the pinned upper boundary

Each effect has exactly one category. This list separates distinct effects within broad PRs. Paths link to immutable source at the upper boundary. See [inventory](step1a-full-commit-inventory.md) for the lower boundary.

## E01: #4625 — Offline agent skill installation

Category: FEATURE.

The CLI embeds its matching BAML skill. `baml agent install` no longer needs a network download. The installed skill records the toolchain version.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4625), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_cli/src/agent_command.rs).

## E02: #4630 — Interface default dispatch

Category: BUGFIX.

Default methods and associated types dispatch with the correct receiver and type frame. Out-of-body implementations resolve consistently.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4630), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src/ns_interfaces).

## E03: #4623 — Function specs, companions, and generated clients

Category: BREAKING_CHANGE.

BAML uses `Fn@spec` and `Fn@stream`. Specs provide `call`, `parse`, `prompt`, and `build_request(client = ...)`. Generated host APIs expose direct/spec/stream bindings. Remove legacy generated prompt/request/parse helpers and regenerate all clients. Callable companion names use @ internally; partial schema names still use $stream.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4623), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_builtins2/baml_std/ai/spec.baml).

## E04: #4623 — Portable prompt and runtime values

Category: FEATURE.

SDKs carry structured prompts and media. Dynamically created BAML class/enum values retain their identity across host calls and streaming partials. Python supports synchronous and asynchronous iteration over these streams.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4623), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/sdk_tests/crates/python_pydantic2/llm_functions/customizable/test_dynamic_runtime_streaming.py).

## E05: #4646 — Callable union inference

Category: BUGFIX.

A lambda passed to a callable union receives contextual parameter and return types from the callable arms.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4646), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src/ns_compiler/ns_generics/ns_function_values/contextual_specialization.baml).

## E06: #4632 — Linux installer compatibility

Category: BUGFIX.

The installer selects GNU or musl wrappers. GNU builds target glibc 2.17 on x86_64 and 2.28 on ARM64. Re-run the installer to replace an incompatible wrapper. Wrapper publication is independently versioned; check its live catalog before notifying users.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4632), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/scripts/install.sh).

## E07: #4686 — Diagnostics while editing

Category: BUGFIX.

Unresolved interface method types and invalid bounds produce diagnostics instead of compiler panics. Unknown values no longer suppress errors. PromptFiddle reports an LSP failure more clearly.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4686), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_compiler2_hir_ty/src/lower.rs).

## E08: #4692 — Cloudflare Workers startup

Category: BUGFIX.

The web SDK avoids unavailable startup-time entropy APIs under workerd. Regenerate the typescript/web SDK and update its matching bridge. Runtime identity uses zero random bytes in this environment; independent isolates can share trace identities.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4692), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/sdks/typescript/bridge_typescript_web).

## E09: #4714 — Mounted runtime package methods

Category: BUGFIX.

Runtime-compiled packages can implement mounted interfaces and call mounted class methods. Reflection type values use interface dispatch correctly.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4714), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/bex_project/src/runtime_compile.rs).

## E10: #4611 — Union members require a shared interface

Category: BREAKING_CHANGE.

Unrelated union arms no longer acquire a structural field/method join. Narrow the union before access or explicitly implement the same declaring interface on every arm.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4611), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_compiler2_hir_ty/src/method_resolution.rs).

## E11: #4720 — Type-bound diagnostics

Category: BUGFIX.

Associated-type pins in interface requirements are honored. Invalid implementation targets are rejected consistently. Required interface signatures reject inference holes, including partial throws holes; write complete types. The temporary runtime-hole policy is removed by #4834.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4720), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_compiler2_hir_ty/src/lower.rs).

## E12: #4723 — Agent commands require a matching skill

Category: BREAKING_CHANGE.

Detected coding agents must install the toolchain-matched skill before authoring commands run. `--agent-skill-check` and `BAML_AGENT_SKILL_CHECK` select auto, require, warn, or off. Run `baml agent install` after upgrading.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4723), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_cli/src/skill_check.rs).

## E13: #4725 — Standard-library names and panic handling

Category: BREAKING_CHANGE.

Rename public types to their shorter final names. Former implementation helpers become private. SAP parsing throws baml.errors.ParseError rather than LlmClient; update catch patterns and explicit throws clauses. Assertions change from UserPanic to AssertionFailed; thrown panics do not enter typed throws inference. Use panic-aware handling when recovery is intended.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4725), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_builtins2/baml_std).

## E14: #4721 — Closure return scope

Category: BUGFIX.

Early returns are checked against the closure rather than its enclosing function. Unannotated closure return inference includes explicit returns and its tail expression.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4721), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_compiler2_hir_ty/src/infer.rs).

## E15: #4739 — Total comparison and sorting

Category: BREAKING_CHANGE.

Implement baml.ops.Compare.cmp returning Ordering and baml.ops.Equals. Replace Comparable.compare and integer sort_by callbacks. Sortable no longer has SortError. NaN equals itself and sorts above numbers; signed zeros compare equal. Use is_nan rather than x != x.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4739), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_builtins2/baml_std/baml/ns_ops/comparison.baml).

## E16: #4727 — Developer documentation portal

Category: FEATURE.

A dedicated developer portal provides language, bridge, CLI, and guide navigation. Keep only final routes; the early Foundations pages are removed by #4805.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4727), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/lib/navigation.ts).

## E17: #4730 — Versioned package and CLI references

Category: FEATURE.

Generated package and CLI documentation is available by toolchain version. Static build expansion was replaced by #4764; it is not a release claim.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4730), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/app/baml/packages).

## E18: #4732 — Checked, highlighted documentation examples

Category: FEATURE.

Documentation renders compiler-checked canonical snippets with syntax highlighting. Early Foundations content is replaced by the final book chapters.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4732), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/components/baml-snippet.tsx).

## E19: #4733 — Current bridge and tutorial documentation

Category: FEATURE.

Bridge compatibility and tutorials use canonical source-backed content. Its intermediate directly rendered changelog is superseded by release-post redirects.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4733), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/content-data).

## E20: #4742 — Removed changelog feed endpoints

Category: BREAKING_CHANGE.

The aggregate and per-version changelog-feed APIs and Markdown representation are removed. Migrate consumers to authored release posts. The removed /changelog page becomes a redirect in #4743.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4742), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-website/next.config.mjs).

## E21: #4743 — Release posts and filters

Category: FEATURE.

Release notes appear as blog posts with a Releases filter. /changelog redirects to /blog?tags=release. Nightlies no longer receive generated changelog entries.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4743), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-website/app/blog/_lib/get-posts.ts).

## E22: #4745 — Readable inline code

Category: BUGFIX.

Inline code styling is consistent across product pages.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4745), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-website/app/globals.css).

## E23: #4759 — Discarded expressions retain effects

Category: BUGFIX.

Unused expressions still evaluate operations that can throw or panic. O0 preserves overwritten user variables for debugging.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4759), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_compiler2_emit/src/analysis.rs).

## E24: #4759 — Smaller optimized bytecode

Category: FEATURE.

O2 snapshot analysis at reviewed head f793a6484 versus pre-PR 048bf98ac reports 99,860 to 93,984 static instructions across the pre-existing inventory. This is unweighted code size including test/stdlib scaffolding, not latency or throughput. The fused branches and stack-carry implementation survive at the upper boundary; later corpus changes prevent treating this as a release-wide percentage.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4759), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_compiler2_emit/src/stack_carry.rs).

## E25: #4762 — Discoverable generated references

Category: FEATURE.

Version-preserving reference navigation, search, type links, member grouping, and copy controls make generated documentation easier to use. Intermediate static publishing is superseded by #4764.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4762), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/components).

## E26: #4764 — Live immutable documentation versions

Category: FEATURE.

Exact-version reference pages render from immutable records. Search, version catalogs, and sitemaps discover newly published references without expanding every page into a site build.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4764), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/lib).

## E27: #4777 — Read-only stdlib navigation

Category: FEATURE.

Go to Definition opens embedded standard-library source in VS Code and PromptFiddle. Hover and further navigation work in these read-only documents. Run baml ide install to update the extension; it no longer extracts stdlib files to disk.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4777), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_lsp/src/dispatch/requests.rs).

## E28: #4751 — Float math helpers

Category: FEATURE.

Adds exp, ln, log2, log10, cbrt, signum, max_finite, min_finite, and epsilon. No accuracy guarantee or scalar performance claim. signum preserves zero sign and maps NaN to +1; #4750 signed-zero literal interning is a separate issue.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4751), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_builtins2/baml_std/baml/float.baml).

## E29: #4783 — Developer changelog redirect

Category: BUGFIX.

Developer portal /changelog URLs redirect to the product release notes.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4783), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/next.config.ts).

## E30: #4496 — Accept WebSocket upgrades

Category: FEATURE.

HTTP servers can accept an HTTP/1.1 WebSocket handshake through the websocket callback returning WsAccept. Subprotocols, extensions, and HTTP/2 WebSockets are not negotiated.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4496), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_builtins2/baml_std/baml/ns_http/server.baml).

## E31: #4496 — WebSocket client API

Category: BREAKING_CHANGE.

Rename WsStream to WebSocket. send accepts text or binary data. next returns text, binary, or CloseEvent; close requires code and reason. Replace null termination checks with CloseEvent handling.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4496), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_builtins2/baml_std/baml/ns_ws/ws.baml).

## E32: #4788 — Release-note formatting

Category: BUGFIX.

Release posts gain BAML syntax highlighting and corrected spacing.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4788), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-website/app/blog/[slug]/content.tsx).

## E33: #4781 — Interface formatting

Category: BUGFIX.

baml fmt formats interface fields, associated types, methods, generics, and requirements. It emits semicolons on required signatures; source semicolons remain optional. Re-run baml fmt.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4781), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_fmt/src/ast).

## E34: #4800 — Map literals and thrown arrays

Category: BUGFIX.

map {} evaluates to an empty map rather than null. throw [] infers its element type from a declared array throws type.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4800), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src/ns_explicit_maps/explicit_maps.baml).

## E35: #4804 — Multiple IDE project roots

Category: FEATURE.

One LSP session can serve independent project roots, including matching package self-names. The playground selects the active project. Arbitrary user package dependencies remain disabled; runtime package-name collisions are not claimed fixed.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4804), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_lsp/src/discovery.rs).

## E36: #4799 — Checked lambda signatures

Category: BUGFIX.

MIR preserves solved lambda annotation holes, contextual types, generic frames, and throws types. Named functions still cannot infer a wildcard return type.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4799), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src/ns_lambdas/signature_holes.baml).

## E37: #4820 — Captured interface dispatch

Category: BUGFIX.

Interface calls on captured receivers work inside closures, including bound methods, field chains, and mutation.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4820), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src/ns_interfaces/b_1470.baml).

## E38: #4806 — Builtin aliases and media carriers

Category: BUGFIX.

Media carrier classes are equivalent to image/audio/video/pdf. Invalid builtin type arguments now produce E0171; remove arguments from non-generic builtins and supply both map key/value arguments.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4806), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src/ns_compiler_aliases/compiler_aliases.baml).

## E39: #4816 — Typed settled future outcomes

Category: BREAKING_CHANGE.

all_settled replaces all_complete. Awaiting it returns ordered Success<T>, Failure<E>, or Panicked outcomes with error type never. Input cancellation is an outcome; cancelling the collector stops waiting. Input failure does not cancel its siblings.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4816), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_builtins2/baml_std/baml/ns_future/future.baml).

## E40: #4816 — Producer panic context

Category: BUGFIX.

Await uses the producer trace when a collector materializes an error context.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4816), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/bex_vm/src/vm.rs).

## E41: #4805 — Book chapters

Category: FEATURE.

New Errors, Interfaces, and Concurrency chapters have annotated, checked examples. The old Foundations placeholder routes are removed; use /baml/book and the replacement chapters.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4805), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/content/baml/book).

## E42: #4807 — Structured journal content

Category: BREAKING_CHANGE.

UserMessage and ToolCompleted carry Block[] (Text or Media). ModelBlock is the model-turn superset. Use new/of constructors and text() projections. Media is accepted, degraded, or rejected by each provider; do not promise universal media support. UserContent from unmerged #4746 is not part of the lower boundary.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4807), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_builtins2/baml_std/ai/ns_events/events.baml).

## E43: #4836 — Production book styles

Category: BUGFIX.

Production builds verify the linked stylesheet contains book UI rules and avoid stale persistent build caches.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4836), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/typescript2/app-developer-docs/lib/built-styles.ts).

## E44: #4834 — Local rigid runtime type bindings

Category: BREAKING_CHANGE.

Only a local type T = unreflect(value) statement is valid. Replace inline unreflect with a local binding. T is rigid, cannot escape its lexical scope, and no longer suppresses type checks. Cast unknown data with a checked pattern.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4834), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src).

## E45: #4834 — Scoped runtime type slots

Category: BUGFIX.

Overlapping local runtime type bindings have distinct frame slots. Escaping inferred types are diagnosed and inferred throws is widened to a valid enclosing supertype.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4834), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_compiler2_hir_ty/src/infer.rs).

## E46: #4808 — Optional arguments through interfaces

Category: BUGFIX.

Calls map supplied named optionals and omitted parameters to the implementation layout. Interface implementations may add optional parameters without crashing direct or bound-method calls.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4808), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src/ns_optional_function_parameters/interface_dispatch.baml).

## E47: #4808 — Dynamic reflection witnesses

Category: BUGFIX.

Reject overlapping dynamic interface witnesses as a batch. Failed registration remains retryable and registered rules survive garbage collection. Remove overlapping registrations.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4808), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/baml_tests/baml_src/ns_dynamic_impl_registration/registration.baml).

## E48: #4808 — Native callback contracts and artifacts

Category: BUGFIX.

Explicit throws never on host callbacks is enforced. Handle host errors or declare the correct callback contract. Artifact version is 6 and cache format is 14 at the upper boundary; regenerate bytecode and repack executables with matching SDK bridges.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4808), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/crates/bex_engine/tests/host_value_callable.rs).

## E49: #4623 — Rust LLM function generation

Category: BUGFIX.

Rust generation no longer skips representable LLM functions solely because of the former ai.errors.Failure throws contract. Regenerate clients to obtain direct, spec, and stream bindings. The independent non-identifier string-literal union limitation (#4371) remains; do not claim all skipped-function cases or silent success are fixed.

Evidence: [merged PR](https://github.com/BoundaryML/baml/pull/4623), [upper source](https://github.com/BoundaryML/baml/tree/a748b3a694496fa96b043b4bb03b02b529c95584/baml_language/sdks/rust/sdkgen_rust/src/emit/function.rs).

## Net-effect corrections

- #4623: do not copy the design document’s proposed `spec.stream()` API. The final source has no such method. BAML uses `@stream`; generated TypeScript retains `$stream`.
- #4725/#4739: `baml.Sortable` moved files and remains public. Only its associated `SortError` is removed. `baml.Comparable` is removed.
- #4733/#4742/#4743/#4783: the final changelog is an authored release-post redirect. The intermediate deleted page and direct source-rendering portal are not separate release features.
- #4732/#4805: retain checked examples, not removed Foundations routes.
- #4762/#4764: retain versioned navigation and live immutable records, not static export/redeployment.
- #4720/#4834: no runtime-hole suppression or inline unreflect survives.
- #4807: #4746 UserContent was never merged into this range. Do not present that interim event as a lower-bound migration.
- #4759/#4804/#4834/#4808: artifact/cache version bumps are aggregated at the final format, not the intermediate 2→3 or 11→12 pair.
- #4751: the PR’s initial exp accuracy statement was withdrawn. No one-ulp or correctly-rounded guarantee is copied. No claim that #4750 is fixed.
