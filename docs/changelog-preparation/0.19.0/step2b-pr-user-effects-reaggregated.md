# Aggregated user effects

Each effect has exactly one category. No group contains more than three PRs. There are no headline changes. Examples and migration pairs are in the [release draft](../../../typescript2/app-website/blog-releases/2026-09-08-baml-0.19.0.md).

## C01 — Call and parse bound LLM specifications

Category: `FEATURE`. PRs: [#4623](https://github.com/BoundaryML/baml/pull/4623).

A bound `ai.FunctionSpec<Out>` now exposes `call()` and `parse()`. Use `call(client = ..., on_event = ...)` to run it. Use `parse()` to turn an existing model reply into its output type. The example parses locally without calling a model.

Evidence: baml_language/crates/baml_builtins2/baml_std/ai/spec.baml.

## C02 — More float math and range constants

Category: `FEATURE`. PRs: [#4751](https://github.com/BoundaryML/baml/pull/4751).

Floats gain `exp`, `ln`, `log2`, `log10`, `cbrt`, and `signum`. `float.max_finite()`, `float.min_finite()`, and `float.epsilon()` expose the finite range and machine epsilon. `min_finite()` is the most negative finite value. `cbrt()` accepts negative inputs. `signum()` returns ±1.0, including for signed zero; NaN returns 1.0. Transcendental results have no cross-platform accuracy guarantee.

Evidence: baml_language/crates/baml_builtins2/baml_std/baml/float.baml; PR final accuracy correction.

## C03 — Infer lambda arguments through a union

Category: `FEATURE`. PRs: [#4646](https://github.com/BoundaryML/baml/pull/4646).

A lambda can infer its parameter types from a union with exactly one callable alternative. For example, a parameter accepting either a string or a callback can supply the callback’s input type. Unions with multiple callable alternatives still need disambiguation.

Evidence: PR #4646 contextual lambda and host callback regression tests.

## C04 — Install the matching agent skill offline

Category: `FEATURE`. PRs: [#4625](https://github.com/BoundaryML/baml/pull/4625).

`baml agent install` installs the skill bundled with the selected toolchain. It no longer fetches the skill from a separate GitHub repository. The skill records its exact toolchain version.

Evidence: baml_language/crates/baml_cli/src/agent_command.rs.

## C05 — Browse standard-library source in your editor

Category: `FEATURE`. PRs: [#4777](https://github.com/BoundaryML/baml/pull/4777).

Go to Definition opens the toolchain’s embedded standard-library source in read-only documents in VS Code and PromptFiddle. Hover, highlighting, symbols, and further navigation work inside those documents. `baml ide install` no longer needs to extract a local source copy.

Evidence: baml_language/crates/baml_lsp; typescript2/pkg-editor; typescript2/app-vscode-ext.

## C06 — Read the new developer guide

Category: `FEATURE`. PRs: [#4727](https://github.com/BoundaryML/baml/pull/4727), [#4732](https://github.com/BoundaryML/baml/pull/4732), [#4733](https://github.com/BoundaryML/baml/pull/4733).

The [developer documentation portal](https://developer.boundaryml.com) brings the language guide, tutorials, TypeScript bridge guidance, search, and light/dark themes together. BAML examples come from a shared source catalog checked with a selected compiler.

Evidence: typescript2/app-developer-docs/content; typescript2/app-developer-docs/content/code.

## C07 — Find references for your exact toolchain

Category: `FEATURE`. PRs: [#4730](https://github.com/BoundaryML/baml/pull/4730), [#4762](https://github.com/BoundaryML/baml/pull/4762), [#4764](https://github.com/BoundaryML/baml/pull/4764).

Browse published versions in the [package reference](https://developer.boundaryml.com/baml/packages) and [CLI reference](https://developer.boundaryml.com/cli). Search includes generated declarations and members. Type links stay on the selected version, and members have copyable declarations and links. New reference snapshots become available through release publication.

Evidence: typescript2/app-developer-docs/lib/generated-content.

## C08 — Browse releases alongside articles

Category: `FEATURE`. PRs: [#4743](https://github.com/BoundaryML/baml/pull/4743), [#4783](https://github.com/BoundaryML/baml/pull/4783).

Release notes now appear in the [blog’s release filter](https://boundaryml.com/blog?tags=release). Both product and developer-documentation changelog links lead to the release history.

Evidence: typescript2/app-website/app/blog; typescript2/app-developer-docs/next.config.ts.

## C09 — Generate smaller optimized bytecode

Category: `FEATURE`. PRs: [#4759](https://github.com/BoundaryML/baml/pull/4759).

The PR’s audit of 246 pre-existing bytecode snapshots counted 99,860 → 93,984 displayed instructions, about 5.9% fewer. At O2, a plain null-coalescing expression shrank from 9 to 4 instructions, and a chained AND from 8 to 6. These are static instruction counts, include test and standard-library scaffolding, and are not a throughput measurement.

Evidence: PR #4759: Snapshot Regression Audit and Before/After Bytecode.

## C10 — Use @ projections in BAML source

Category: `BREAKING_CHANGE`. PRs: [#4623](https://github.com/BoundaryML/baml/pull/4623).

Use `Fn@spec(...)` and `Fn@stream(...)` in BAML source. Compiler-generated `$spec` and `$stream` callables are no longer exposed there. The generated TypeScript SDK still uses `Fn$stream`; Python keeps its `Fn_stream` exports. Generated spec and stream companions are also omitted from reflection and function listings.

Evidence: baml_language/crates/baml_tests/baml_src/ns_llm_stream_contract/stream_contract.baml.

## C11 — Rename the request client override

Category: `BREAKING_CHANGE`. PRs: [#4623](https://github.com/BoundaryML/baml/pull/4623).

The named argument on `FunctionSpec.build_request()` is now `client`, replacing `override_client`.

Evidence: baml_language/crates/baml_builtins2/baml_std/ai/spec.baml.

## C13 — Make shared union members explicit

Category: `BREAKING_CHANGE`. PRs: [#4611](https://github.com/BoundaryML/baml/pull/4611).

Same-named fields or methods on unrelated union arms no longer create a shared API, even when their types match. Narrow the union before access, or declare the shared member on one interface implemented by every arm.

Evidence: PR #4611; B-1646.

## C15 — Refresh the skill before agent authoring commands

Category: `BREAKING_CHANGE`. PRs: [#4723](https://github.com/BoundaryML/baml/pull/4723).

Detected coding agents must have a BAML skill matching the selected toolchain before authoring commands run. Install the skill again after changing toolchains. Human sessions warn by default. `--agent-skill-check` and `BAML_AGENT_SKILL_CHECK` accept `auto`, `require`, `warn`, and `off`; installation and management commands remain available.

Evidence: baml_language/crates/baml_cli/src/agent_command.rs.

## C16 — Use shorter standard-library type names

Category: `BREAKING_CHANGE`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

Update type annotations, constructors, catches, and generated-SDK references to the names below. Regenerate SDKs with the new toolchain.

| 0.18.0 | 0.19.0 |
| --- | --- |
| `anthropic.AnthropicClient` | `anthropic.Client` |
| `google.GoogleClient` | `google.GeminiClient` |
| `ai.ClientSelector`, `ai.clients.ClientSelector` | `ai.Selector`, `ai.clients.Selector` |
| `ai.stream.StreamEvent` | `ai.stream.Event` |
| `ai.mcp.McpConnection` | `ai.mcp.Connection` |
| `baml.csv.CsvError`, `CsvErrorKind`, `CsvPosition`, `CsvValue` | `baml.csv.Error`, `ErrorKind`, `Position`, `Value` |
| `baml.csv.CsvReader`, `CsvRecord`, `CsvRows<T>`, `CsvWriter` | `baml.csv.Reader`, `Record`, `Rows<T>`, `Writer` |
| `baml.errors.ErrorContext` | `baml.errors.Context` |
| `baml.future.FutureState` | `baml.future.State` |
| `baml.host.HostValue` | `baml.host.Value` |
| `baml.spawn.SpawnParams<T, E>` | `baml.spawn.Params<T, E>` |
| `baml.json.JsonParseError`, `JsonDecodeError`, `JsonSerializationError`, `JsonPathError` | `baml.json.ParseError`, `DecodeError`, `SerializationError`, `PathError` |
| `baml.toml.TomlParseError` | `baml.toml.ParseError` |
| `baml.yaml.YamlParseError` | `baml.yaml.ParseError` |

Evidence: baml_language/crates/baml_builtins2/baml_std; declaration rename audit.

## C17 — Use public parsing and testing APIs

Category: `BREAKING_CHANGE`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

Parser caches and polling sentinels are now internal: `baml.sap.ParseCache`, `NoYield`, `baml.csv.CsvNeedData`, `CsvSkip`, and `CsvHeaders` gained underscore-prefixed names. Testing registry execution/selection helpers and pool types also became internal. Use `baml.sap.parse<T>()` or `parse_type()` for parsing, CSV readers for records, and authored `test`/`testset` blocks or the public testing runners. Provider implementation helpers under `*.internal` also dropped provider-name prefixes; those helpers are not stable public APIs.

Evidence: PR #4725 baml_std/{baml/ns_sap,testing,anthropic/ns_internal,google/ns_internal,openai/ns_internal}.

## C18 — Keep panics out of throws clauses

Category: `BREAKING_CHANGE`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

Explicitly throwing a `baml.panics.*` value raises a panic. It no longer contributes to inferred `throws`, and wildcard `catch` or `catch_all` arms skip it. Name a panic type explicitly if you intend to intercept it. Assertions now raise `baml.panics.AssertionFailed` instead of a generic user panic.

Evidence: baml_language/crates/baml_tests/baml_src/ns_panic_channel/panic_channel.baml; baml_std/assert/assert.baml.

## C19 — Handle the updated I/O error contracts

Category: `BREAKING_CHANGE`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

`baml.http.send()` and `fetch_sse()` include `baml.errors.InvalidArgument`. `baml.io.input()` can throw `baml.errors.Io`. Invalid glob patterns throw `ParseError`, while `Glob.matches()` is `throws never`. `File.close()` is idempotent and only declares `Io`; remove `InvalidArgument` from wrappers that declared it solely for close. CSV reader and writer close operations now propagate file-close failures.

Evidence: baml_std/baml/ns_{http,io,glob,fs,csv}.

## C20 — Remove obsolete catchable runtime error types

Category: `BREAKING_CHANGE`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

`baml.errors.NotImplemented` and `baml.errors.DevOther` were removed. Unsupported runtime capabilities use a panic channel, and runtime invariant failures use internal errors. If your own code threw either removed class, define an application error. Handle only the documented errors from standard-library calls.

Evidence: baml_std/baml/ns_errors/errors.baml; sys_ops/src/lib.rs.

## C21 — Check float division results explicitly

Category: `BREAKING_CHANGE`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

Float division by zero produces infinity or NaN. It no longer raises `DivisionByZero`; integer division still does. Validate the denominator when your application needs to reject zero.

Evidence: baml_language/crates/bex_vm/src/package_baml/ops_math.rs.

## C22 — Update integer overflow handling

Category: `BREAKING_CHANGE`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

`int.abs()` is now `throws never` and raises `baml.panics.IntegerOverflow` for `int.min_value()`. It no longer throws `InvalidArgument`. Integer left-shift interface calls now match the `<<` operator’s truncation: high bits are discarded, `1 << 62` is `int.min_value()`, and `1 << 63` is zero. Negative shifts still panic.

Evidence: baml_language/crates/bex_vm/src/package_baml/{int,ops_bitwise}.rs.

## C23 — Omit the time argument to request midnight

Category: `BREAKING_CHANGE`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

`PlainDate.to_plain_datetime()` takes a non-null `PlainTime`. Omit the argument for midnight instead of passing `null`.

Evidence: baml_std/baml/ns_time/plaindate.baml.

## C24 — Use Compare and Ordering for sorting

Category: `BREAKING_CHANGE`. PRs: [#4739](https://github.com/BoundaryML/baml/pull/4739).

`baml.ops.Compare.cmp()` returns `baml.ops.Ordering.Less`, `Equal`, or `Greater`. It replaces `baml.Comparable.compare()`. Implement `Equals` and `Compare` for custom naturally sorted types. `Compare` supplies `min`, `max`, and `clamp`. `Comparable.CompareError` and `Sortable.SortError` are gone; natural comparison and `sort()` are `throws never`. For fallible ordering, use `sort_by()`, whose callback still propagates its declared errors but now returns `Ordering` rather than int. `sort_by_key()` keys must implement `Compare`.

Evidence: baml_std/baml/ns_ops/comparison.baml; baml_std/baml/sortable.baml.

## C25 — Use is_nan instead of non-reflexive equality

Category: `BREAKING_CHANGE`. PRs: [#4739](https://github.com/BoundaryML/baml/pull/4739).

Equality is reflexive, including NaN. NaN equals NaN and sorts above every number. Positive and negative zero compare equal. Replace `x != x` NaN checks with `x.is_nan()`, and review logic that relied on IEEE unordered comparisons.

Evidence: baml_std/baml/ns_ops/comparison.baml; baml_language/crates/bex_vm/src/package_baml/ops_comparison.rs.

## C26 — Rebuild compiled artifacts and regenerate SDKs

Category: `BREAKING_CHANGE`. PRs: [#4759](https://github.com/BoundaryML/baml/pull/4759).

The compiled artifact ABI moves from 2 to 3 and the cache format from 11 to 12. Rebuild packed applications and regenerate SDKs with the new toolchain. Keep each generated SDK and its bridge on matching versions.

Evidence: PR #4759; baml_language/crates/baml_artifact; baml_language/crates/bex_cache.

## C27 — Correct interface method dispatch

Category: `BUGFIX`. PRs: [#4630](https://github.com/BoundaryML/baml/pull/4630).

Fixed calls to interface default methods and out-of-body implementations, including methods involving associated types. Calls now use the correct implementation and type arguments.

Evidence: PR #4630 dispatch and type-frame regression tests.

## C28 — Diagnose invalid types without crashing the LSP

Category: `BUGFIX`. PRs: [#4686](https://github.com/BoundaryML/baml/pull/4686).

Unresolved types in required interface method signatures and invalid bounds now produce diagnostics instead of crashing the language server or reaching a later compiler panic. PromptFiddle also shows a clearer message if its language server panics.

Evidence: PR #4686.

## C29 — Initialize generated web SDKs in Cloudflare Workers

Category: `BUGFIX`. PRs: [#4692](https://github.com/BoundaryML/baml/pull/4692).

Generated TypeScript/web SDKs can initialize under workerd without trapping on module-scope random-number access.

Evidence: PR #4692; B-1656; bridge_typescript workerd export.

## C30 — Call mounted methods from runtime-compiled packages

Category: `BUGFIX`. PRs: [#4714](https://github.com/BoundaryML/baml/pull/4714).

`reflect.Package.compile()` preserves mounted class methods and required/default interface methods. Runtime-compiled code can implement a mounted interface and call mounted methods without namespace-shadowing errors. Calling `to_string()` on reflected types in that code no longer triggers a compiler panic.

Evidence: baml_language/crates/baml_tests/tests/runtime_package_compile.rs.

## C31 — Respect inherited associated-type constraints

Category: `BUGFIX`. PRs: [#4720](https://github.com/BoundaryML/baml/pull/4720).

The type checker respects associated-type pins inherited through an interface’s `requires` constraints and consistently diagnoses invalid implementation targets.

Evidence: PR #4720.

## C32 — Check closure returns against the closure

Category: `BUGFIX`. PRs: [#4721](https://github.com/BoundaryML/baml/pull/4721).

An early `return` inside a closure is checked against that closure’s return type, rather than the enclosing function’s type. Closure return inference also accounts for explicit and tail returns.

Evidence: PR #4721; B-1682.

## C33 — Preserve effects of discarded expressions

Category: `BUGFIX`. PRs: [#4759](https://github.com/BoundaryML/baml/pull/4759).

Discarding an expression’s value no longer discards its required effects. Conditional mutations on the right of `&&` or `||` still run, and unused division or indexing still raises catchable panics when it fails. The fixes cover O0, O1, and O2.

Evidence: PR #4759 discarded-expression regressions.

## C34 — Bootstrap on older glibc and musl Linux systems

Category: `BUGFIX`. PRs: [#4632](https://github.com/BoundaryML/baml/pull/4632).

The Linux installer selects a compatible GNU wrapper and falls back to musl. Wrapper GNU builds target glibc 2.17 on x86_64 and 2.28 on ARM64; Debian Bookworm and Alpine were covered in the PR. This is an installer/wrapper update delivered alongside the language toolchain, with its own version.

Evidence: PR #4632; GitHub issue #4624.

## C35 — Keep built-in frames out of user tracebacks

Category: `BUGFIX`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

User-facing tracebacks omit standard-library frames, including built-ins implemented in BAML.

Evidence: baml_language/crates/bex_vm_types/src/errors.rs; SDK error renderers.

## C36 — Return empty buffers for non-positive reads

Category: `BUGFIX`. PRs: [#4725](https://github.com/BoundaryML/baml/pull/4725).

Native and web read operations return empty buffers for non-positive byte counts.

Evidence: baml_language/crates/sys_native/src/io_impls.rs; bridge_wasm/src/wasm_io.rs.
