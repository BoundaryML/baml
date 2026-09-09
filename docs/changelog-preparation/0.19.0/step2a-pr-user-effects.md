# Effects by PR

## [#4625](https://github.com/BoundaryML/baml/pull/4625)

- C04 — FEATURE: Install the matching agent skill offline. `baml agent install` installs the skill bundled with the selected toolchain. It no longer fetches the skill from a separate GitHub repository. The skill records its exact toolchain version.

## [#4630](https://github.com/BoundaryML/baml/pull/4630)

- C27 — BUGFIX: Correct interface method dispatch. Fixed calls to interface default methods and out-of-body implementations, including methods involving associated types. Calls now use the correct implementation and type arguments.

## [#4623](https://github.com/BoundaryML/baml/pull/4623)

- C01 — FEATURE: Call and parse bound LLM specifications. A bound `ai.FunctionSpec<Out>` now exposes `call()` and `parse()`. Use `call(client = ..., on_event = ...)` to run it. Use `parse()` to turn an existing model reply into its output type. The example parses locally without calling a model.
- C10 — BREAKING_CHANGE: Use @ projections in BAML source. Use `Fn@spec(...)` and `Fn@stream(...)` in BAML source. Compiler-generated `$spec` and `$stream` callables are no longer exposed there. The generated TypeScript SDK still uses `Fn$stream`; Python keeps its `Fn_stream` exports. Generated spec and stream companions are also omitted from reflection and function listings.
- C11 — BREAKING_CHANGE: Rename the request client override. The named argument on `FunctionSpec.build_request()` is now `client`, replacing `override_client`.

## [#4646](https://github.com/BoundaryML/baml/pull/4646)

- C03 — FEATURE: Infer lambda arguments through a union. A lambda can infer its parameter types from a union with exactly one callable alternative. For example, a parameter accepting either a string or a callback can supply the callback’s input type. Unions with multiple callable alternatives still need disambiguation.

## [#4632](https://github.com/BoundaryML/baml/pull/4632)

- C34 — BUGFIX: Bootstrap on older glibc and musl Linux systems. The Linux installer selects a compatible GNU wrapper and falls back to musl. Wrapper GNU builds target glibc 2.17 on x86_64 and 2.28 on ARM64; Debian Bookworm and Alpine were covered in the PR. This is an installer/wrapper update delivered alongside the language toolchain, with its own version.

## [#4686](https://github.com/BoundaryML/baml/pull/4686)

- C28 — BUGFIX: Diagnose invalid types without crashing the LSP. Unresolved types in required interface method signatures and invalid bounds now produce diagnostics instead of crashing the language server or reaching a later compiler panic. PromptFiddle also shows a clearer message if its language server panics.

## [#4692](https://github.com/BoundaryML/baml/pull/4692)

- C29 — BUGFIX: Initialize generated web SDKs in Cloudflare Workers. Generated TypeScript/web SDKs can initialize under workerd without trapping on module-scope random-number access.

## [#4714](https://github.com/BoundaryML/baml/pull/4714)

- C30 — BUGFIX: Call mounted methods from runtime-compiled packages. `reflect.Package.compile()` preserves mounted class methods and required/default interface methods. Runtime-compiled code can implement a mounted interface and call mounted methods without namespace-shadowing errors. Calling `to_string()` on reflected types in that code no longer triggers a compiler panic.

## [#4611](https://github.com/BoundaryML/baml/pull/4611)

- C13 — BREAKING_CHANGE: Make shared union members explicit. Same-named fields or methods on unrelated union arms no longer create a shared API, even when their types match. Narrow the union before access, or declare the shared member on one interface implemented by every arm.

## [#4720](https://github.com/BoundaryML/baml/pull/4720)

- C31 — BUGFIX: Respect inherited associated-type constraints. The type checker respects associated-type pins inherited through an interface’s `requires` constraints and consistently diagnoses invalid implementation targets.

## [#4723](https://github.com/BoundaryML/baml/pull/4723)

- C15 — BREAKING_CHANGE: Refresh the skill before agent authoring commands. Detected coding agents must have a BAML skill matching the selected toolchain before authoring commands run. Install the skill again after changing toolchains. Human sessions warn by default. `--agent-skill-check` and `BAML_AGENT_SKILL_CHECK` accept `auto`, `require`, `warn`, and `off`; installation and management commands remain available.

## [#4725](https://github.com/BoundaryML/baml/pull/4725)

- C16 — BREAKING_CHANGE: Use shorter standard-library type names. Update type annotations, constructors, catches, and generated-SDK references to the names below. Regenerate SDKs with the new toolchain.
- C17 — BREAKING_CHANGE: Use public parsing and testing APIs. Parser caches and polling sentinels are now internal: `baml.sap.ParseCache`, `NoYield`, `baml.csv.CsvNeedData`, `CsvSkip`, and `CsvHeaders` gained underscore-prefixed names. Testing registry execution/selection helpers and pool types also became internal. Use `baml.sap.parse<T>()` or `parse_type()` for parsing, CSV readers for records, and authored `test`/`testset` blocks or the public testing runners. Provider implementation helpers under `*.internal` also dropped provider-name prefixes; those helpers are not stable public APIs.
- C18 — BREAKING_CHANGE: Keep panics out of throws clauses. Explicitly throwing a `baml.panics.*` value raises a panic. It no longer contributes to inferred `throws`, and wildcard `catch` or `catch_all` arms skip it. Name a panic type explicitly if you intend to intercept it. Assertions now raise `baml.panics.AssertionFailed` instead of a generic user panic.
- C19 — BREAKING_CHANGE: Handle the updated I/O error contracts. `baml.http.send()` and `fetch_sse()` include `baml.errors.InvalidArgument`. `baml.io.input()` can throw `baml.errors.Io`. Invalid glob patterns throw `ParseError`, while `Glob.matches()` is `throws never`. `File.close()` is idempotent and only declares `Io`; remove `InvalidArgument` from wrappers that declared it solely for close. CSV reader and writer close operations now propagate file-close failures.
- C20 — BREAKING_CHANGE: Remove obsolete catchable runtime error types. `baml.errors.NotImplemented` and `baml.errors.DevOther` were removed. Unsupported runtime capabilities use a panic channel, and runtime invariant failures use internal errors. If your own code threw either removed class, define an application error. Handle only the documented errors from standard-library calls.
- C21 — BREAKING_CHANGE: Check float division results explicitly. Float division by zero produces infinity or NaN. It no longer raises `DivisionByZero`; integer division still does. Validate the denominator when your application needs to reject zero.
- C22 — BREAKING_CHANGE: Update integer overflow handling. `int.abs()` is now `throws never` and raises `baml.panics.IntegerOverflow` for `int.min_value()`. It no longer throws `InvalidArgument`. Integer left-shift interface calls now match the `<<` operator’s truncation: high bits are discarded, `1 << 62` is `int.min_value()`, and `1 << 63` is zero. Negative shifts still panic.
- C23 — BREAKING_CHANGE: Omit the time argument to request midnight. `PlainDate.to_plain_datetime()` takes a non-null `PlainTime`. Omit the argument for midnight instead of passing `null`.
- C35 — BUGFIX: Keep built-in frames out of user tracebacks. User-facing tracebacks omit standard-library frames, including built-ins implemented in BAML.
- C36 — BUGFIX: Return empty buffers for non-positive reads. Native and web read operations return empty buffers for non-positive byte counts.

## [#4721](https://github.com/BoundaryML/baml/pull/4721)

- C32 — BUGFIX: Check closure returns against the closure. An early `return` inside a closure is checked against that closure’s return type, rather than the enclosing function’s type. Closure return inference also accounts for explicit and tail returns.

## [#4739](https://github.com/BoundaryML/baml/pull/4739)

- C24 — BREAKING_CHANGE: Use Compare and Ordering for sorting. `baml.ops.Compare.cmp()` returns `baml.ops.Ordering.Less`, `Equal`, or `Greater`. It replaces `baml.Comparable.compare()`. Implement `Equals` and `Compare` for custom naturally sorted types. `Compare` supplies `min`, `max`, and `clamp`. `Comparable.CompareError` and `Sortable.SortError` are gone; natural comparison and `sort()` are `throws never`. For fallible ordering, use `sort_by()`, whose callback still propagates its declared errors but now returns `Ordering` rather than int. `sort_by_key()` keys must implement `Compare`.
- C25 — BREAKING_CHANGE: Use is_nan instead of non-reflexive equality. Equality is reflexive, including NaN. NaN equals NaN and sorts above every number. Positive and negative zero compare equal. Replace `x != x` NaN checks with `x.is_nan()`, and review logic that relied on IEEE unordered comparisons.

## [#4727](https://github.com/BoundaryML/baml/pull/4727)

- C06 — FEATURE: Read the new developer guide. The [developer documentation portal](https://developer.boundaryml.com) brings the language guide, tutorials, TypeScript bridge guidance, search, and light/dark themes together. BAML examples come from a shared source catalog checked with a selected compiler.

## [#4730](https://github.com/BoundaryML/baml/pull/4730)

- C07 — FEATURE: Find references for your exact toolchain. Browse published versions in the [package reference](https://developer.boundaryml.com/baml/packages) and [CLI reference](https://developer.boundaryml.com/cli). Search includes generated declarations and members. Type links stay on the selected version, and members have copyable declarations and links. New reference snapshots become available through release publication.

## [#4732](https://github.com/BoundaryML/baml/pull/4732)

- C06 — FEATURE: Read the new developer guide. The [developer documentation portal](https://developer.boundaryml.com) brings the language guide, tutorials, TypeScript bridge guidance, search, and light/dark themes together. BAML examples come from a shared source catalog checked with a selected compiler.

## [#4733](https://github.com/BoundaryML/baml/pull/4733)

- C06 — FEATURE: Read the new developer guide. The [developer documentation portal](https://developer.boundaryml.com) brings the language guide, tutorials, TypeScript bridge guidance, search, and light/dark themes together. BAML examples come from a shared source catalog checked with a selected compiler.

## [#4743](https://github.com/BoundaryML/baml/pull/4743)

- C08 — FEATURE: Browse releases alongside articles. Release notes now appear in the [blog’s release filter](https://boundaryml.com/blog?tags=release). Both product and developer-documentation changelog links lead to the release history.

## [#4759](https://github.com/BoundaryML/baml/pull/4759)

- C09 — FEATURE: Generate smaller optimized bytecode. The PR’s audit of 246 pre-existing bytecode snapshots counted 99,860 → 93,984 displayed instructions, about 5.9% fewer. At O2, a plain null-coalescing expression shrank from 9 to 4 instructions, and a chained AND from 8 to 6. These are static instruction counts, include test and standard-library scaffolding, and are not a throughput measurement.
- C26 — BREAKING_CHANGE: Rebuild compiled artifacts and regenerate SDKs. The compiled artifact ABI moves from 2 to 3 and the cache format from 11 to 12. Rebuild packed applications and regenerate SDKs with the new toolchain. Keep each generated SDK and its bridge on matching versions.
- C33 — BUGFIX: Preserve effects of discarded expressions. Discarding an expression’s value no longer discards its required effects. Conditional mutations on the right of `&&` or `||` still run, and unused division or indexing still raises catchable panics when it fails. The fixes cover O0, O1, and O2.

## [#4762](https://github.com/BoundaryML/baml/pull/4762)

- C07 — FEATURE: Find references for your exact toolchain. Browse published versions in the [package reference](https://developer.boundaryml.com/baml/packages) and [CLI reference](https://developer.boundaryml.com/cli). Search includes generated declarations and members. Type links stay on the selected version, and members have copyable declarations and links. New reference snapshots become available through release publication.

## [#4764](https://github.com/BoundaryML/baml/pull/4764)

- C07 — FEATURE: Find references for your exact toolchain. Browse published versions in the [package reference](https://developer.boundaryml.com/baml/packages) and [CLI reference](https://developer.boundaryml.com/cli). Search includes generated declarations and members. Type links stay on the selected version, and members have copyable declarations and links. New reference snapshots become available through release publication.

## [#4777](https://github.com/BoundaryML/baml/pull/4777)

- C05 — FEATURE: Browse standard-library source in your editor. Go to Definition opens the toolchain’s embedded standard-library source in read-only documents in VS Code and PromptFiddle. Hover, highlighting, symbols, and further navigation work inside those documents. `baml ide install` no longer needs to extract a local source copy.

## [#4751](https://github.com/BoundaryML/baml/pull/4751)

- C02 — FEATURE: More float math and range constants. Floats gain `exp`, `ln`, `log2`, `log10`, `cbrt`, and `signum`. `float.max_finite()`, `float.min_finite()`, and `float.epsilon()` expose the finite range and machine epsilon. `min_finite()` is the most negative finite value. `cbrt()` accepts negative inputs. `signum()` returns ±1.0, including for signed zero; NaN returns 1.0. Transcendental results have no cross-platform accuracy guarantee.

## [#4783](https://github.com/BoundaryML/baml/pull/4783)

- C08 — FEATURE: Browse releases alongside articles. Release notes now appear in the [blog’s release filter](https://boundaryml.com/blog?tags=release). Both product and developer-documentation changelog links lead to the release history.
