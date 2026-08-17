# BAML Language changelog

This changelog covers the independent `baml_language` release line. It does not describe releases of the main BAML toolchain.

## [0.17.0](https://github.com/BoundaryML/baml/compare/baml-language-0.16.0...baml-language-source-c0153b20108bc428b77a53128b9a59ddeb4e2b42) - 2026-08-14

### Features

- [review] Added runtime reflection and type construction, including compiling and mounting packages, invoking reflected callables, and isolating dynamic work in sessions. ([#4325](https://github.com/BoundaryML/baml/pull/4325)) - Antonio Sarosi
- [review] Added native BAML clients for OpenAI, Anthropic, Google, Vertex AI, AWS Bedrock, Azure, Ollama, OpenRouter, and Vercel AI Gateway, including typed streaming, media outputs, AWS SigV4, and Google Cloud authentication. ([#4430](https://github.com/BoundaryML/baml/pull/4430)) - aaronvg
- Added `baml.sys.pid()`, `baml.fs.chmod()`, and `baml.fs.symlink()`. ([#4427](https://github.com/BoundaryML/baml/pull/4427)) - 2kai2kai2
- Added `baml.crypto.*` APIs for SHA-256 hashing, authenticated encryption, and key generation. ([#4431](https://github.com/BoundaryML/baml/pull/4431)) - 2kai2kai2
- Added `baml toolchain pin <canary|nightly|version|path>` to select a project-local toolchain in the nearest `baml.toml` ([#4386](https://github.com/BoundaryML/baml/pull/4386)) - Sam Lijin
- Added arithmetic across `baml.time` values ([#4366](https://github.com/BoundaryML/baml/pull/4366)) - 2kai2kai2
- Added `assert.is_type<T>(value)` and improved assertion failure formatting  ([#4388](https://github.com/BoundaryML/baml/pull/4388)) - 2kai2kai2
- Added integer and bigint bitwise operators (`&`, `|`, `^`, `<<`, and `>>`), including mixed `int`/`bigint` widening, and added generic `baml.ops.Index` support for arrays, string-keyed maps, and `uint8array`. ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend
- Enabled advertised LSP code-action support so spec-compliant editors can request BAML code actions, including “Open in Playground.” ([#3358](https://github.com/BoundaryML/baml/pull/3358)) - Miguel F. Serna

### Breaking and compatibility changes

- [review] Renamed `openai.OpenAiClient` to `openai.ResponsesClient`. ([#4430](https://github.com/BoundaryML/baml/pull/4430)) - aaronvg
- Improved the string stdlib APIs: renamed `String.char_at` to `String.at`, kept negative indexing, and made out-of-range access return `null`; made `String.code_point_at` return `null` out of range; removed `String.matches` in favor of `String.includes`; renamed `String.substring` to `String.slice`; and added `String.last_index_of`. ([#4433](https://github.com/BoundaryML/baml/pull/4433)) - 2kai2kai2
- Drop support for Jinja templates: BAML now has TS-style template literals that are much easier to use. ([#4367](https://github.com/BoundaryML/baml/pull/4367)) - Avery Townsend
- LLM function fields now require colon-delimited `client:`, `prompt:`, and `tools:` syntax. ([#4317](https://github.com/BoundaryML/baml/pull/4317)) - Vaibhav Mittal
- Rewrote the type checker to enable more powerful type inference ([#4301](https://github.com/BoundaryML/baml/pull/4301)) - Avery Townsend

### Fixes

- [review] Rejected runtime-checked arguments on indirect calls with E0010; release builds had previously compiled them while silently omitting the check. ([#4460](https://github.com/BoundaryML/baml/pull/4460)) - Antonio Sarosi
- [review] Enabled runtime package compilation from `baml run` scripts and expressions. ([#4451](https://github.com/BoundaryML/baml/pull/4451)) - Antonio Sarosi
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
-  Make `Array.sort_by` and `Array.sort_by_key` a stable O(n log n) merge sort. ([#4382](https://github.com/BoundaryML/baml/pull/4382)) - Sam Lijin

### Performance

- [review] Restored and improved compiler and VM performance after BEP-066, reducing overhead in package inference, calls, arrays, interface dispatch, and string operations. ([#4448](https://github.com/BoundaryML/baml/pull/4448); [#4450](https://github.com/BoundaryML/baml/pull/4450)) - Antonio Sarosi
