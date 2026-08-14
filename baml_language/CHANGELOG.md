# BAML Language changelog

This changelog covers the independent `baml_language` release line. It does not describe releases of the main BAML toolchain.

## [0.17.0](https://github.com/BoundaryML/baml/compare/baml-language-0.16.0...baml-language-source-a1ac4ae68e06213fb9050b53842dd4c3ebfc4dea) - 2026-08-14

Comparison range: [`baml-language-0.16.0`](https://github.com/BoundaryML/baml/releases/tag/baml-language-0.16.0) at [`2e021b3429db8769656e9b8646657c1863789169`](https://github.com/BoundaryML/baml/commit/2e021b3429db8769656e9b8646657c1863789169) through the verified release target [`baml-language-source-a1ac4ae68e06213fb9050b53842dd4c3ebfc4dea`](https://github.com/BoundaryML/baml/tree/baml-language-source-a1ac4ae68e06213fb9050b53842dd4c3ebfc4dea) at [`a1ac4ae68e06213fb9050b53842dd4c3ebfc4dea`](https://github.com/BoundaryML/baml/commit/a1ac4ae68e06213fb9050b53842dd4c3ebfc4dea). The target was also the head of `origin/canary` when these notes were prepared.

### User-facing features

- Added `baml toolchain pin <canary|nightly|version|path>` to select a project-local toolchain in the nearest `baml.toml`, with atomic edits that preserve comments and formatting; version-skew diagnostics now recommend the executable pin command and identify the relevant SDK package ecosystem. ([#4386](https://github.com/BoundaryML/baml/pull/4386)) - Sam Lijin
- Added arithmetic across `baml.time` values: add or subtract `Duration` values from temporal values, subtract compatible temporal values to produce a `Duration`, multiply or divide durations by `int` or `bigint`, calculate duration remainders, and negate durations. Plain-time arithmetic wraps across midnight and zoned-date-time arithmetic preserves timezone information. ([#4366](https://github.com/BoundaryML/baml/pull/4366)) - 2kai2kai2
- Added `assert.is_type<T>(value)`, which validates and returns a value of the requested type, and improved assertion failure formatting for strings, numbers, equality checks, and approximate comparisons. ([#4388](https://github.com/BoundaryML/baml/pull/4388)) - 2kai2kai2
- Added integer and bigint bitwise operators (`&`, `|`, `^`, `<<`, and `>>`), including mixed `int`/`bigint` widening, and added generic `baml.ops.Index` support for arrays, string-keyed maps, and `uint8array`. ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend
- Enabled advertised LSP code-action support so spec-compliant editors can request BAML code actions, including “Open in Playground.” ([#3358](https://github.com/BoundaryML/baml/pull/3358)) - Miguel F. Serna

### Breaking and compatibility changes

- Removed legacy Jinja prompt rendering, Jinja prompt type checking, and `template_string` declarations. Migrate `#"..."#` LLM prompts to backtick prompts with `${...}` interpolation, and migrate `template_string` declarations to functions that return backtick strings. Raw hash strings outside LLM prompts remain supported, and the compiler emits targeted migration diagnostics for both removed forms. ([#4367](https://github.com/BoundaryML/baml/pull/4367)) - Avery Townsend
- LLM function fields now require colon-delimited `client:`, `prompt:`, and `tools:` syntax. The same parser change removes `type_builder` blocks and `dynamic class`/`dynamic enum` test definitions; `dynamic` is now an ordinary identifier rather than a reserved keyword. ([#4317](https://github.com/BoundaryML/baml/pull/4317)) - Vaibhav Mittal
- Replaced the legacy TIR type checker with the new `baml_compiler2_hir_ty` inference engine. The new engine implements the documented interface, generic, associated-type, operator, pattern-narrowing, and throws/effect rules more consistently; projects that relied on previously accepted unsound interface implementations, missing associated-type bindings, ambiguous projections, or incompatible throws contracts can now receive compile errors, and diagnostic text and source spans can differ. ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend

### Fixes

- Preserved authored escapes, leading and trailing whitespace, blank lines, and relative indentation in backtick strings by dedenting raw source before escape decoding. In particular, a trailing `\n` is no longer silently removed. ([#4365](https://github.com/BoundaryML/baml/pull/4365)) - hellovai
- Stopped expression functions from being misclassified as LLM functions when their bodies reference parameters named `prompt` or `client`; only explicit colon-delimited LLM fields select the LLM grammar. ([#4317](https://github.com/BoundaryML/baml/pull/4317)) - Vaibhav Mittal
- Improved B1129 diagnostics for invalid postfix `!` syntax so the suggested optional-unwrapping alternatives are valid BAML. ([#4383](https://github.com/BoundaryML/baml/pull/4383)) - 2kai2kai2
- Stopped the LSP server from advertising `textDocumentSync.willSave`, which it accepts only as a no-op, while retaining full-document synchronization and `didSave`. ([#3366](https://github.com/BoundaryML/baml/pull/3366)) - Miguel F. Serna
- Restored cross-namespace LLM calls in playground workflow graphs by resolving relative call paths to canonical package-qualified names before graph expansion. ([#4246](https://github.com/BoundaryML/baml/pull/4246)) - Sam Lijin
- Made `baml run -e` evaluate standard-library-only expressions even when unrelated project files do not compile, while preserving project-context fallback for expressions that reference project declarations. ([#4401](https://github.com/BoundaryML/baml/pull/4401)) - Sam Lijin
- Made `//#` header comments parse consistently: they are preserved in expression functions, executable blocks, match arms, and catch arms, and rejected elsewhere with a targeted diagnostic instead of causing cascading parser failures. ([#4406](https://github.com/BoundaryML/baml/pull/4406)) - Sam Lijin
- Corrected the no-project CLI diagnostic to recommend the supported `baml test --project <DIR>` option instead of the nonexistent `--file` option. ([#4410](https://github.com/BoundaryML/baml/pull/4410)) - Sam Lijin
- Prevented required interface methods from being treated as callable default bodies and hardened recursive type normalization against stack overflows as part of the type-engine cutover. ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend

### Performance

- Replaced the quadratic insertion sort behind `Array.sort_by` and `Array.sort_by_key` with a stable O(n log n) merge sort. Key extraction remains once per element from left to right, comparator failures leave the original array unchanged, and large sorts no longer amplify profiling writes quadratically. ([#4382](https://github.com/BoundaryML/baml/pull/4382)) - Sam Lijin

### Internal changes

- Added Python integration coverage proving generated-bytecode/toolchain version mismatches are diagnosed before bytecode deserialization and retain complete repair guidance across the PyO3 boundary. ([#4380](https://github.com/BoundaryML/baml/pull/4380)) - Sam Lijin
- Stabilized `pack_e2e` under nextest by building `baml-pack-host` once in a filtered setup step on Unix and Windows while preserving the plain-`cargo test` fallback. ([#4387](https://github.com/BoundaryML/baml/pull/4387)) - Sam Lijin
- Corrected the internal `BexMulitProject` type name to `BexMultiProject` without changing behavior or public APIs. ([#4389](https://github.com/BoundaryML/baml/pull/4389)) - Sam Lijin
- Added the `hir_ty` specification harness and removed the superseded TIR implementation after the inference-engine cutover. ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend
- Temporarily disabled the flaky aarch64 macOS Cargo test and its cache purge while retaining the other Apple release builds and tests; restoration is tracked by B-1545. ([#4418](https://github.com/BoundaryML/baml/pull/4418)) - Sam Lijin
- Embedded the source commit in `bridge_wasm`, exposed it as `commitHash()`, and added a compiler gate for the deployed Prompt Fiddle demo. ([#4420](https://github.com/BoundaryML/baml/pull/4420)) - Avery Townsend
- Upgraded the baml_language binary-size baseline refresh workflow to `peter-evans/create-pull-request@v8`. ([#4424](https://github.com/BoundaryML/baml/pull/4424)) - Sam Lijin
