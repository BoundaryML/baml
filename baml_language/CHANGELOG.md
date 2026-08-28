# BAML Language changelog

## [0.18.0](https://github.com/BoundaryML/baml/compare/baml-language-0.17.0...baml-language-source-f3e1e25210b1ac551a4d1ffd0238db5d85824789) - 2026-08-27

### Features

- Add `on_event` to enable granular LLM call introspection ([#4570](https://github.com/BoundaryML/baml/pull/4570)) - aaronvg
- In Python, allow using `async for` to iterate over LLM streams ([#4604](https://github.com/BoundaryML/baml/pull/4604)) - aaronvg
- Implement `baml query` to allow querying  historical BAML program execution traces, and allow exploring them using the playground ([#4548](https://github.com/BoundaryML/baml/pull/4548); [#4563](https://github.com/BoundaryML/baml/pull/4563); [#4578](https://github.com/BoundaryML/baml/pull/4578)) - rossirpaulo
- Improve Python SDK compatibility with BAML v0: allow previewing prompts without requiring secrets, add `FinishReasonError`, expose OpenAI/Anthropic/Vertex-specific response metadata, and more ([#4459](https://github.com/BoundaryML/baml/pull/4459)) - aaronvg
- Add `baml.io.Read` and `baml.io.Write` interfaces and make all stdlib I/O abstractions implement them ([#4606](https://github.com/BoundaryML/baml/pull/4606)) - Avery Townsend
- Allow using `baml.json.to_string` and `to_json` with `unknown`. ([#4601](https://github.com/BoundaryML/baml/pull/4601)) - Antonio Sarosi
- Expanded reflection API: add `reflect.AnyClass`, `reflect.call_any`, allow `unreflect(expr)` in any type position ([#4491](https://github.com/BoundaryML/baml/pull/4491); [#4493](https://github.com/BoundaryML/baml/pull/4493); [#4519](https://github.com/BoundaryML/baml/pull/4519); [#4574](https://github.com/BoundaryML/baml/pull/4574); [#4600](https://github.com/BoundaryML/baml/pull/4600)) - Antonio Sarosi
- Allow truthiness conversions, e.g. `if (0)` is now allowed; also improve type narrowing in `if` blocks ([#4498](https://github.com/BoundaryML/baml/pull/4498)) - Avery Townsend
- Make more interface member projections work: `(Type as Interface).member` and `Interface.instance_method(instance)` ([#4500](https://github.com/BoundaryML/baml/pull/4500)) - 2kai2kai2
- Improve lambda type inference: allow using invocations of a lambda to infer its arg types, when the lambda is defined without them ([#4599](https://github.com/BoundaryML/baml/pull/4599)) - Avery Townsend
- Add `baml.errors.UnknownError` for wrapping errors while preserving original cause and stack trace ([#4441](https://github.com/BoundaryML/baml/pull/4441)) - Avery Townsend
- Add lazy iterator primitives: `take`, `skip`, `take_while`, and `skip_while`. ([#4510](https://github.com/BoundaryML/baml/pull/4510)) - Ritunjay
- Add an optional `rng` parameter to `int.random()`, `float.random()`, `bool.random()`, and `bigint.random()` ([#4135](https://github.com/BoundaryML/baml/pull/4135)) - 2kai2kai2
- Make `baml run` and `baml test` respect `BAML_LOG` and `--log <LEVEL>` ([#4408](https://github.com/BoundaryML/baml/pull/4408); [#4409](https://github.com/BoundaryML/baml/pull/4409)) - Sam Lijin

### Breaking and compatibility changes

- Continue stabilizing the `ai.Runner` interface ([#4604](https://github.com/BoundaryML/baml/pull/4604)) - aaronvg
- Reflection types and methods are now consolidated into `reflect.*`: we now have `reflect.Type`, `reflect.AnyClass`, `reflect.AnyFunction`, and everything from `baml.reflect.*` has also been migrated ([#4543](https://github.com/BoundaryML/baml/pull/4543); [#4580](https://github.com/BoundaryML/baml/pull/4580)) - Antonio Sarosi, 2kai2kai2
- Delete `baml.json.encode`, which is redundant with `baml.json.to_string<unknown>()` ([#4601](https://github.com/BoundaryML/baml/pull/4601)) - Antonio Sarosi
- Drop support for Jinja template literals `#"..."#`; LLM functions must now use BAML-native template literals`${ipsum()} dolor` instead ([#4565](https://github.com/BoundaryML/baml/pull/4565)) - Avery Townsend
- Drop support for BAML v0 test blocks `test ClassifyMessage { functions [Classify1, Classify2] ... }` ; tests must now be declared with BAML v1 expression bodies `test "my test case" { assert.equal(a, b) }`)  ([#4602](https://github.com/BoundaryML/baml/pull/4602)) - Avery Townsend
- `ctx.output_format` is now a callable API and must be written as `ctx.output_format()`. ([#4567](https://github.com/BoundaryML/baml/pull/4567)) - Avery Townsend
- `baml_sdk` is now generated in the same directory as `baml.toml`, instead of the parent directory ([#4522](https://github.com/BoundaryML/baml/pull/4522)) - Sam Lijin
- The generated C# client is now generated as `baml_sdk` instead of `baml_client` ([#4535](https://github.com/BoundaryML/baml/pull/4535)) - Sam Lijin

### Fixes

- Improve LLM response parsing behavior when an optional field is omitted ([#4612](https://github.com/BoundaryML/baml/pull/4612)) - Sam Lijin
- Make HTTP operations in the playground work again ([#4609](https://github.com/BoundaryML/baml/pull/4609)) - hellovai
- Preserve errors thrown by code executed by `eval()`([#4583](https://github.com/BoundaryML/baml/pull/4583)) - Antonio Sarosi
- Preserve type definition values at runtime, instead of dropping them or passing them around as strings ([#4501](https://github.com/BoundaryML/baml/pull/4501); [#4516](https://github.com/BoundaryML/baml/pull/4516); [#4536](https://github.com/BoundaryML/baml/pull/4536); [#4577](https://github.com/BoundaryML/baml/pull/4577)) - Antonio Sarosi
- Make `unreflect` safe: define its compile-time and runtime behavior, and emit compiler diagnostics in disallowed scenarios ([#4518](https://github.com/BoundaryML/baml/pull/4518); [#4530](https://github.com/BoundaryML/baml/pull/4530)) - Antonio Sarosi
- Fix stack-carry compiler optimizations ([#4508](https://github.com/BoundaryML/baml/pull/4508); [#4544](https://github.com/BoundaryML/baml/pull/4544)) - Sam Lijin, Antonio Sarosi
- Make match arms for `T[]` match correctly ([#4547](https://github.com/BoundaryML/baml/pull/4547)) - Avery Townsend
- Made literal patterns use type-membership semantics instead of value equality. ([#4478](https://github.com/BoundaryML/baml/pull/4478)) - Avery Townsend
- Prevent compiler crashes on `for` loops over joined map union arms and on `Iterable`-bounded generics. ([#4490](https://github.com/BoundaryML/baml/pull/4490)) - Avery Townsend
- Raise compile errors for empty arrays and maps whose element types cannot be inferred. ([#4573](https://github.com/BoundaryML/baml/pull/4573)) - Avery Townsend
- Raise compile errors for non-data LLM output schemas instead of crashing or silently erroring. ([#4470](https://github.com/BoundaryML/baml/pull/4470)) - Antonio Sarosi
- Bring`ctx.output_format()` to feature parity with BAML v0 ([#4567](https://github.com/BoundaryML/baml/pull/4567)) - Avery Townsend
- When mutating an object in a called function or a loop, make sure the mutation persists when exiting the context. ([#4467](https://github.com/BoundaryML/baml/pull/4467)) - Antonio Sarosi
- Fix `x?.m<T>()` syntax form: thread type args through optional chaining ([#4495](https://github.com/BoundaryML/baml/pull/4495)) - Antonio Sarosi
- Return a compiler diagnostic when attempting to resolve member variables or call member functions on `unknown`  ([#4466](https://github.com/BoundaryML/baml/pull/4466)) - Antonio Sarosi
- Return a reflection error, instead of silently failing, when attempting to resolve a generic function defined at runtime ([#4473](https://github.com/BoundaryML/baml/pull/4473)) - Antonio Sarosi
- Return a compiler diagnostic instead of crashing when type-checking functions with references to undefined types ([#4566](https://github.com/BoundaryML/baml/pull/4566)) - Avery Townsend
- Enforce runtime type checking of return values when calling functions with `reflect.call_any`. ([#4600](https://github.com/BoundaryML/baml/pull/4600)) - Antonio Sarosi
- Emit an error instead of crashing when `naming_convention = "language"` . ([#4526](https://github.com/BoundaryML/baml/pull/4526)) - Sam Lijin
- `baml fmt` now removes redundant parentheses. ([#4489](https://github.com/BoundaryML/baml/pull/4489); [#4541](https://github.com/BoundaryML/baml/pull/4541)) - Avery Townsend, aaronvg
- Make BAML compile faster: stdlib is now precompiled. ([#4453](https://github.com/BoundaryML/baml/pull/4453); [#4458](https://github.com/BoundaryML/baml/pull/4458); [#4461](https://github.com/BoundaryML/baml/pull/4461); [#4463](https://github.com/BoundaryML/baml/pull/4463)) - Antonio Sarosi
- LLM stream parsing is now faster: parsing is run on delta batches, instead of per-delta ([#4604](https://github.com/BoundaryML/baml/pull/4604)) - aaronvg

## [0.17.0](https://github.com/BoundaryML/baml/compare/baml-language-0.16.0...baml-language-source-c0153b20108bc428b77a53128b9a59ddeb4e2b42) - 2026-08-14

### Features

- Added runtime reflection and type construction, including compiling and mounting packages, invoking reflected callables, and isolating dynamic work in sessions. ([#4325](https://github.com/BoundaryML/baml/pull/4325)) - Antonio Sarosi
- Added native BAML clients for OpenAI, Anthropic, Google, Vertex AI, AWS Bedrock, Azure, Ollama, OpenRouter, and Vercel AI Gateway, including typed streaming, media outputs, AWS SigV4, and Google Cloud authentication. ([#4430](https://github.com/BoundaryML/baml/pull/4430)) - aaronvg
- Added `baml.sys.pid()`, `baml.fs.chmod()`, and `baml.fs.symlink()`. ([#4427](https://github.com/BoundaryML/baml/pull/4427)) - 2kai2kai2
- Added `baml.crypto.*` APIs for SHA-256 hashing, authenticated encryption, and key generation. ([#4431](https://github.com/BoundaryML/baml/pull/4431)) - 2kai2kai2
- Added `baml toolchain pin <canary|nightly|version|path>` to select a project-local toolchain in the nearest `baml.toml` ([#4386](https://github.com/BoundaryML/baml/pull/4386)) - Sam Lijin
- Added arithmetic across `baml.time` values ([#4366](https://github.com/BoundaryML/baml/pull/4366)) - 2kai2kai2
- Added `assert.is_type<T>(value)` and improved assertion failure formatting  ([#4388](https://github.com/BoundaryML/baml/pull/4388)) - 2kai2kai2
- Added integer and bigint bitwise operators (`&`, `|`, `^`, `<<`, and `>>`), including mixed `int`/`bigint` widening, and added generic `baml.ops.Index` support for arrays, string-keyed maps, and `uint8array`. ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend
- Enabled advertised LSP code-action support so spec-compliant editors can request BAML code actions, including “Open in Playground.” ([#3358](https://github.com/BoundaryML/baml/pull/3358)) - Miguel F. Serna

### Breaking and compatibility changes

- Renamed `openai.OpenAiClient` to `openai.ResponsesClient`. ([#4430](https://github.com/BoundaryML/baml/pull/4430)) - aaronvg
- Improved the string stdlib APIs: renamed `String.char_at` to `String.at`, kept negative indexing, and made out-of-range access return `null`; made `String.code_point_at` return `null` out of range; removed `String.matches` in favor of `String.includes`; renamed `String.substring` to `String.slice`; and added `String.last_index_of`. ([#4433](https://github.com/BoundaryML/baml/pull/4433)) - 2kai2kai2
- Drop support for Jinja templates: BAML now has TS-style template literals that are much easier to use. ([#4367](https://github.com/BoundaryML/baml/pull/4367)) - Avery Townsend
- LLM function fields now require colon-delimited `client:`, `prompt:`, and `tools:` syntax. ([#4317](https://github.com/BoundaryML/baml/pull/4317)) - Vaibhav Mittal
- Rewrote the type checker to enable more powerful type inference ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend

### Fixes

- Enabled runtime package compilation from `baml run` scripts and expressions. ([#4451](https://github.com/BoundaryML/baml/pull/4451)) - Antonio Sarosi
- Supported double-quoted LLM prompts as literal, non-interpolating prompts and fixed the segfault they could cause. ([#4432](https://github.com/BoundaryML/baml/pull/4432)) - 2kai2kai2
- Made `Array.join` stringify non-string elements instead of silently replacing them with empty strings. ([#4433](https://github.com/BoundaryML/baml/pull/4433)) - 2kai2kai2
- Fixed null-coalescing assignments, property shorthand scope, runtime type tests, and media kind validation. ([#4434](https://github.com/BoundaryML/baml/pull/4434)) - Avery Townsend
- Fixed Windows path, URI, and VFS handling. ([#4103](https://github.com/BoundaryML/baml/pull/4103)) - jubjub727
- Fixed method dispatch through optional interface receivers. ([#4435](https://github.com/BoundaryML/baml/pull/4435)) - Avery Townsend
- Preserved authored escapes, leading and trailing whitespace, blank lines, and relative indentation in backtick strings by dedenting raw source before escape decoding. In particular, a trailing `\n` is no longer silently removed. ([#4365](https://github.com/BoundaryML/baml/pull/4365)) - hellovai
- Improved B1129 diagnostics for invalid postfix `!` syntax so the suggested optional-unwrapping alternatives are valid BAML. ([#4383](https://github.com/BoundaryML/baml/pull/4383)) - 2kai2kai2
- Stopped the LSP server from advertising `textDocumentSync.willSave`, which it accepts only as a no-op, while retaining full-document synchronization and `didSave`. ([#3366](https://github.com/BoundaryML/baml/pull/3366)) - Miguel F. Serna
- Restored cross-namespace LLM calls in playground workflow graphs by resolving relative call paths to canonical package-qualified names before graph expansion. ([#4246](https://github.com/BoundaryML/baml/pull/4246)) - Sam Lijin
- Made `baml run -e` evaluate standard-library-only expressions even when unrelated project files do not compile, while preserving project-context fallback for expressions that reference project declarations. ([#4401](https://github.com/BoundaryML/baml/pull/4401)) - Sam Lijin
- Made `//#` header comments parse consistently: they are preserved in expression functions, executable blocks, match arms, and catch arms, and rejected elsewhere with a targeted diagnostic instead of causing cascading parser failures. ([#4406](https://github.com/BoundaryML/baml/pull/4406)) - Sam Lijin
- Corrected the no-project CLI diagnostic to recommend the supported `baml test --project <DIR>` option instead of the nonexistent `--file` option. ([#4410](https://github.com/BoundaryML/baml/pull/4410)) - Sam Lijin
- Prevented required interface methods from being treated as callable default bodies. ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend
- Make `Array.sort_by` and `Array.sort_by_key` a stable O(n log n) merge sort. ([#4382](https://github.com/BoundaryML/baml/pull/4382)) - Sam Lijin
- Restored and improved compiler and VM performance after BEP-066, reducing overhead in package inference, calls, arrays, interface dispatch, and string operations. ([#4448](https://github.com/BoundaryML/baml/pull/4448); [#4450](https://github.com/BoundaryML/baml/pull/4450)) - Antonio Sarosi
