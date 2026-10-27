# BAML 0.23.0 release audit

Prepared on 2026-10-25. The live canary manifest reports 0.22.2, released 2026-10-13.

- Lower: `baml-language-0.22.2`, `01f5c412c73c213846ea8cb6ead89d303b053672`.
- Upper: `origin/canary`, resolved once to `303930923cd3f2bfc68ebca79dee96f5779175a8`.
- Range includes all 71 commits, across the entire repository.
- Release metadata PR: https://github.com/BoundaryML/baml/pull/5411.
- Exact upper-source compiler: published nightly `0.22.3-nightly.20261024.a` (its tag resolves to the upper SHA).
- Release metadata changes do not change the compiler examples. If more product changes merge before the release, reconcile the range before publishing.

Each numbered row is a distinct effect, classified once. A broad PR may have several independently classified effects.

1. **FEATURE**, [#5200](https://github.com/BoundaryML/baml/pull/5200): OpenAI Decisions maps typed predicates, choices, and class leaves to the Decisions endpoint, including images.
2. **BUGFIX**, [#5252](https://github.com/BoundaryML/baml/pull/5252): SAP uses realized generic return types when constructing the parse schema.
3. **BUGFIX**, [#5243](https://github.com/BoundaryML/baml/pull/5243): Unresolved MIR members produce diagnostics instead of emitting unsafe fallback access; use map indexing instead of unresolved dot access.
4. **FEATURE**, [#5225](https://github.com/BoundaryML/baml/pull/5225): Claude Code receives images/PDFs and long prompts through stream-JSON stdin; unsupported audio/video is rejected.
5. **BUGFIX**, [#5223](https://github.com/BoundaryML/baml/pull/5223): Runtime directory initialization accepts trailing forward slashes.
6. **BUGFIX**, [#5038](https://github.com/BoundaryML/baml/pull/5038): Generic arithmetic receiver inference no longer crashes on the reported method chains.
7. **FEATURE**, [#5253](https://github.com/BoundaryML/baml/pull/5253): Cached string lengths and ASCII prefixes speed specific string microbenchmarks; publish the measured workload and regression limits.
8. **BUGFIX**, [#5253](https://github.com/BoundaryML/baml/pull/5253): Deep appended strings no longer overflow the stack while being flattened or traversed.
9. **BREAKING_CHANGE**, [#5254](https://github.com/BoundaryML/baml/pull/5254): Package-local cross-namespace paths require root; the configured self-name and partial namespace paths no longer substitute for it.
10. **BUGFIX**, [#5254](https://github.com/BoundaryML/baml/pull/5254): JSON decoding uses an explicit stack and cyclic input conversion fails with a catchable error.
11. **BUGFIX**, [#5271](https://github.com/BoundaryML/baml/pull/5271): Shipped Rust dependencies receive the documented networking/TLS/browser advisory updates.
12. **BUGFIX**, [#5273](https://github.com/BoundaryML/baml/pull/5273): Generated persisted bytecode excludes tests/testsets and their absolute source paths; regenerate SDKs.
13. **BUGFIX**, [#5275](https://github.com/BoundaryML/baml/pull/5275): SDK distributions include the Apache license and npm metadata declares it.
14. **BUGFIX**, [#5280](https://github.com/BoundaryML/baml/pull/5280): Initial cloud telemetry failure warns and continues by default; explicit abort policies remain available.
15. **BREAKING_CHANGE**, [#5268](https://github.com/BoundaryML/baml/pull/5268): Reflection kind views and constructors move to root classes; regenerate host bindings and rename calls.
16. **FEATURE**, [#5272](https://github.com/BoundaryML/baml/pull/5272): Named concrete types expose static type(); Type.of<T>() remains for compound types and type parameters.
17. **BREAKING_CHANGE**, [#5278](https://github.com/BoundaryML/baml/pull/5278): Replace body-level unreflect aliases with let type bindings.
18. **BREAKING_CHANGE**, [#5276](https://github.com/BoundaryML/baml/pull/5276): Reflection get lookups throw NotFound; use find lookups to retain nullable misses.
19. **BREAKING_CHANGE**, [#5277](https://github.com/BoundaryML/baml/pull/5277): Type-taking reflection APIs accept TypeView; update invariant collection annotations, metadata() readers, and assumptions about to_string().
20. **FEATURE**, [#5279](https://github.com/BoundaryML/baml/pull/5279): Class.extends<I> builds runtime classes from interface fields and implementations.
21. **FEATURE**, [#5269](https://github.com/BoundaryML/baml/pull/5269): Builder.fields_from copies field metadata and skip flags, but not class metadata or implementations.
22. **BREAKING_CHANGE**, [#5270](https://github.com/BoundaryML/baml/pull/5270): Field.type is non-null; remove unreachable null fallbacks.
23. **BREAKING_CHANGE**, [#5297](https://github.com/BoundaryML/baml/pull/5297): Bounded let type requires diverging else, pending builder types cannot be bound, and invalid native reflection arguments consistently use reflect.errors.InvalidArgument.
24. **BUGFIX**, [#5297](https://github.com/BoundaryML/baml/pull/5297): Custom TypeView implementations are materialized through as_type in all supported reflection entry points.
25. **HEADLINE_CHANGE**, [#5282](https://github.com/BoundaryML/baml/pull/5282): AI executions own one operation through specs and streaming, with manual start_span lifecycle APIs; wrapper spans are suppressed.
26. **BREAKING_CHANGE**, [#5261](https://github.com/BoundaryML/baml/pull/5261): ModelCall replaces LLMCall, turns form a tree, usage counts become nullable, requested/resolved models are separate, and SQL usage/cost surfaces migrate.
27. **BUGFIX**, [#5261](https://github.com/BoundaryML/baml/pull/5261): Swift wrappers avoid shadowing a parameter named args.
28. **BUGFIX**, [#5310](https://github.com/BoundaryML/baml/pull/5310): Cloud telemetry delivers owned function operations and their captures/usage; collectors must support the closed completion flag.
29. **BUGFIX**, [#5316](https://github.com/BoundaryML/baml/pull/5316): TextMate and tree-sitter recognize let type declarations.
30. **FEATURE**, [#5319](https://github.com/BoundaryML/baml/pull/5319): BAML_CONTEXT supplies launch context to packed executables and baml run, with explicit flag and stdin precedence.
31. **BREAKING_CHANGE**, [#5317](https://github.com/BoundaryML/baml/pull/5317): Enforce exhaustive catches, keep ordinary wildcard panic policy, and compose catch chains as successive recovery stages.
32. **BUGFIX**, [#5335](https://github.com/BoundaryML/baml/pull/5335): Expression-client AI functions and entry futures retain the original function's trace name.
33. **BUGFIX**, [#5333](https://github.com/BoundaryML/baml/pull/5333): Deep host-argument validation reports type errors instead of letting mismatched scalar/container/class values reach unsafe VM operations.
34. **BREAKING_CHANGE**, [#5345](https://github.com/BoundaryML/baml/pull/5345): String/backtick escapes decode strictly; migrate literal backslashes to raw quoted strings or escaped backslashes and update unsupported byte escapes.
35. **BUGFIX**, [#5320](https://github.com/BoundaryML/baml/pull/5320): Rust union arms with colliding short names gain namespace-qualified variants; regenerate and update matches.
36. **BUGFIX**, [#5356](https://github.com/BoundaryML/baml/pull/5356): The v1 trust stores and TLS verifier receive updated compromised-CA distrust; mixed v0/dev-tool changes are not separately announced.
37. **HEADLINE_CHANGE**, [#5325](https://github.com/BoundaryML/baml/pull/5325): File-scope const globals initialize once, share heap values, and cannot be rebound; globals use const and locals use let.
38. **FEATURE**, [#5361](https://github.com/BoundaryML/baml/pull/5361): Package.globals/get_global/find_global discover global and client values, with existing initialization and type checks.
39. **BUGFIX**, [#5364](https://github.com/BoundaryML/baml/pull/5364): The user lesson uses const for the global annotation and let inside a function.
40. **BREAKING_CHANGE**, [#5357](https://github.com/BoundaryML/baml/pull/5357): Newlines separate ambiguous expression starts; parenthesize intended multiline subtraction/call/index continuations before reformatting.
41. **FEATURE**, [#5371](https://github.com/BoundaryML/baml/pull/5371): OpenAI ChatClient and ResponsesClient optionally use native structured outputs.
42. **BREAKING_CHANGE**, [#5371](https://github.com/BoundaryML/baml/pull/5371): Schema generation rejects unknown and corrects bigint schema representation; use JSON-compatible concrete types.
43. **FEATURE**, [#5169](https://github.com/BoundaryML/baml/pull/5169): Record class/enum definitions and reconstruct recorded runtime type expressions with SQL baml_code.
44. **BUGFIX**, [#5358](https://github.com/BoundaryML/baml/pull/5358): Instance methods resolve through aliases.
45. **BUGFIX**, [#5020](https://github.com/BoundaryML/baml/pull/5020): Reject non-place assignments; assign individual variables, fields, or indices instead.
46. **BUGFIX**, [#5291](https://github.com/BoundaryML/baml/pull/5291): Omitted nullable host class fields become null, while required/misspelled fields retain errors.
47. **BREAKING_CHANGE**, [#5377](https://github.com/BoundaryML/baml/pull/5377): TypeScript nullable class keys become optional; strict reads must handle undefined.
48. **BUGFIX**, [#5379](https://github.com/BoundaryML/baml/pull/5379): Bound final cloud delivery to five seconds; pending data can be discarded at the deadline and stdout is not closed early.
49. **BUGFIX**, [#5380](https://github.com/BoundaryML/baml/pull/5380): Cloud telemetry uses proxy environment variables, including NO_PROXY exclusions.
50. **FEATURE**, [#5293](https://github.com/BoundaryML/baml/pull/5293): Rust exit shutdown and explicit runtime::shutdown plus Node package-root shutdownRuntime finish in-flight work and telemetry.
51. **FEATURE**, [#5372](https://github.com/BoundaryML/baml/pull/5372): Required Rust class arguments accept Borrow<Class> across generated call forms; async references must outlive the future.
52. **BUGFIX**, [#5373](https://github.com/BoundaryML/baml/pull/5373): Native HTTP requests share a bounded client/connection pool keyed by timeout and proxy/TLS environment settings.
53. **BUGFIX**, [#5374](https://github.com/BoundaryML/baml/pull/5374): Rust class cycles through aliases are boxed; update affected field construction to Box::new.
54. **FEATURE**, [#5287](https://github.com/BoundaryML/baml/pull/5287): Rust emits json-backed types/functions using baml_bridge::Json; overlapping bare-json unions still skip.
55. **FEATURE**, [#5375](https://github.com/BoundaryML/baml/pull/5375): Generated Rust serde feature supports eligible structural types; declaration-order union decoding differs from BAML best-match decoding.
56. **FEATURE**, [#5389](https://github.com/BoundaryML/baml/pull/5389): Publisher http_bodies=false omits network bodies and error text; function captures and function errors remain independent.
57. **BUGFIX**, [#5388](https://github.com/BoundaryML/baml/pull/5388): Generated artifacts tolerate unusable telemetry levels/empty destinations and fall back locally for malformed URLs, with documented credential/CLI exceptions.
58. **BUGFIX**, [#5390](https://github.com/BoundaryML/baml/pull/5390): Release capture memory as delivery progresses, prepare/upload concurrently, and acknowledge only contiguous stored recordings.
59. **BUGFIX**, [#5387](https://github.com/BoundaryML/baml/pull/5387): Union members preserve complete function type structure.
60. **BUGFIX**, [#5405](https://github.com/BoundaryML/baml/pull/5405): Optional chaining expands nested nullable unions correctly.
61. **FEATURE**, [#5398](https://github.com/BoundaryML/baml/pull/5398): LLM/SAP/tool schemas prune non-JSON union alternatives while rejecting entirely impossible outputs.
62. **BREAKING_CHANGE**, [#5399](https://github.com/BoundaryML/baml/pull/5399): Remove redundant JSON/YAML names, honor FromJson in decode, change missing-field error paths, and allow stringify to throw SerializationError.
63. **FEATURE**, [#5399](https://github.com/BoundaryML/baml/pull/5399): JSON naming, null omission, and configurable indent options operate on typed values while preserving null data in containers.
64. **FEATURE**, [#5402](https://github.com/BoundaryML/baml/pull/5402): Direct JSON conversion improves measured release-build speedtest workloads; cache scope is one call.
65. **BUGFIX**, [#5402](https://github.com/BoundaryML/baml/pull/5402): Serialization detects cycles and processes deeply nested values without the intermediate recursive serde tree.
66. **BREAKING_CHANGE**, [#5404](https://github.com/BoundaryML/baml/pull/5404): Best-match JSON unions replace first-fit, non-finite JSON text throws, reason strings become specific, and null-admitting fields may be omitted.
67. **BUGFIX**, [#5401](https://github.com/BoundaryML/baml/pull/5401): Inference failures preserve diagnostics, callbacks with unpinned throws default appropriately, plain-binding matches can infer, and checked emission cannot ignore invalid dependencies.
68. **BUGFIX**, [#5386](https://github.com/BoundaryML/baml/pull/5386): VM threads use a 10 MiB stack budget instead of 256 frames; overflow remains a named catchable panic.
