# Task

Implement `perft` as specified in `SPEC.md`.

## Required interface

```baml
function perft(
    fen: string,
    depth: int,
) -> int throws baml.errors.ParseError
```

## Constraints

- Write the implementation entirely in BAML.
- Keep the `perft` signature unchanged.
- Do not edit `SPEC.md`, `baml.toml`, `baml_src/public_tests.baml`,
  `baml_src/corpus_tests.baml`, or files under `testdata`.
- Do not invoke another chess engine or external process.
- Treat this workspace as an isolated evaluation. Do not inspect sibling copies
  of this benchmark or prior trial implementations.
- You may replace `baml_src/chess_perft.baml` and add supporting files under
  `baml_src`.
- Prefer native BAML tests for any additional coverage.
- Treat the installed BAML CLI as authoritative. Use `baml describe` instead of
  guessing APIs.

## Verification

The public-domain corpus is loaded from JSON and registered as individual native
BAML tests. Use `baml test --list` to see their IDs and `baml test -i "CASE"` to
run one case while developing.

```bash
baml fmt baml_src
baml check
baml test
```
