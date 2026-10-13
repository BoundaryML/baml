# Task

Implement `exec_ere` as specified in `SPEC.md`.

## Constraints

- Write the implementation entirely in BAML.
- Keep `Capture`, `EreMatch`, and the `exec_ere` signature unchanged.
- Do not edit `SPEC.md`, `baml.toml`, `baml_src/public_tests.baml`,
  `baml_src/corpus_tests.baml`, or files under `testdata`.
- Do not invoke another regex engine or external process.
- Treat this workspace as an isolated evaluation. Do not inspect sibling copies
  of this benchmark or prior trial implementations.
- You may replace `baml_src/ere.baml` and add supporting files under `baml_src`.
- Prefer native BAML tests for any additional coverage.
- Treat the installed BAML CLI as authoritative. Use `baml describe` instead of guessing APIs.

## Verification

The compatible cases in the unified public corpus are registered as individual
native BAML tests.
Use `baml test --list` to see their canonical IDs and `baml test -i "CASE"` to
run a single failing case while developing.

```bash
baml fmt baml_src
baml check
baml test
```
