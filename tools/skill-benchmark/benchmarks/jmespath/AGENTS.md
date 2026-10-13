# Task

Implement `jmespath_search` as specified in `SPEC.md` and the pinned official
JMESPath specification.

## Required interface

```baml
function jmespath_search(
    expression: string,
    source: JsonSource,
) -> JmesPathResult
    throws baml.errors.Io | baml.errors.Timeout | baml.json.JsonParseError
```

## Constraints

- Write the implementation entirely in BAML.
- Keep the public enums, classes, unions, and function signature unchanged.
- Do not edit `SPEC.md`, `baml.toml`, `baml_src/public_tests.baml`,
  `baml_src/corpus_tests.baml`, vendored specification files, or files under
  `testdata`.
- Do not invoke another JMESPath implementation, host callable, or external
  process.
- Treat this workspace as an isolated evaluation. Do not inspect sibling copies
  of this benchmark or prior trial implementations.
- You may replace `baml_src/jmespath.baml` and add supporting files under
  `baml_src`.
- Prefer native BAML tests for any additional coverage.
- Treat the installed BAML CLI as authoritative. Use `baml describe` instead of
  guessing APIs.

## Verification

All 892 correctness cases from the pinned official compliance corpus are loaded
from JSON and registered as individual native BAML tests.

```bash
baml fmt baml_src
baml check
baml test --list
baml test
```
