# Regex test provenance

These are BAML-specific contract and regression tests for [BEP 52](https://beps.boundaryml.com/beps/52). The repository history does not identify an imported upstream regex conformance suite or external fixture dataset. The counts below describe this PR's tests, not independent upstream conformance coverage.

The search methods use the explicit names `match_once()` and `match_all()`, while `has_match()` communicates the boolean-only form and `match_full()` communicates whole-string matching.

## Where the cases came from

- The original `feat(stdlib): add baml.regex` commit by `hellovai`, co-authored by Claude Opus 5, introduced 37 tests in `regex.baml`, 19 in `replace.baml`, and 9 Rust tests in `sys_regex`. Its commit message and test comments describe BAML API contracts and engine integration; they do not record per-case external sources. This is the limit of the recorded provenance, rather than proof that no outside example influenced a case.
- Some inputs directly exercise examples in BEP 52: named date captures, `${1}mm` replacement, and opt-in lookaround/backreferences. Other small inputs cover the specified behavior: Unicode codepoint offsets, absent groups, stateless reuse, captured split delimiters, empty matches, and literal versus template replacement. Matching a BEP example does not establish that every original case was copied from the BEP.
- The original author's later parser fix added two BAML regressions for qualified `Regex.match` calls and required interface methods named `match`.
- During the Codex takeover, three BAML regressions were added for trailing extended-mode comments/captures in full matching, batched offsets for optional/nested groups, and optional/qualified interface calls. These were constructed for issues found while reviewing the implementation against BEP 52 and CodeRabbit's findings, rather than imported from another project's suite.
- The takeover also added three `sys_regex` regressions: full-match flags/comments/captures, consistent oversized-pattern classification, and `\K` resetting the reported match start. Their expected results assert BAML's wrapper contract around the pinned engines.

After adopting `new()`, `has_match()`, `match_once()`, `match_all()`, and `match_full()`, the qualified and optional regex access cases were retained under the new names; the unrelated interface-method case and parser keyword support were removed. This gives 45 BAML runtime tests and 12 `sys_regex` unit tests. The related string-search oracle lives in its owning Rust module; it exercises implementation details that a BAML runtime assertion cannot inspect directly.

## What the layers establish

| Location | Purpose |
| --- | --- |
| `regex.baml` and `replace.baml` | User-visible BEP behavior through the compiler, VM, and standard library. Invalid runtime patterns are built dynamically so constant validation does not intercept them. |
| `crates/sys_regex/src/lib.rs` | Compilation, error classification, exact-match wrapping, Unicode boundaries, and offset conversion at the engine boundary. |
| `crates/baml_tests/projects/diagnostic_errors/regex_constant_pattern/` | Constant-pattern errors reported at compile time (currently E0174). Includes regressions for CodeRabbit’s duplicate-argument finding: validation follows the call’s bound pattern/dialect, while duplicate bindings are rejected. Snapshots are generated from this fixture, not imported expected-output files. |
| `crates/bex_str/src/tests.rs` | Offset search compared with a straightforward string-search oracle, including Unicode and invalid start positions. |

Paths in the table are relative to `baml_language/`. These focused examples do not replace the engines' own test suites, constitute an exhaustive regex conformance suite, or benchmark the complete replacement pipeline. The takeover's temporary search-loop microbenchmark was a performance probe, not a source of correctness fixtures.
