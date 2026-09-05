# JMESPath BAML benchmark

Implement the complete JMESPath language and JSON source loading behind one
interface:

```baml
function jmespath_search(
    expression: string,
    source: JsonSource,
) -> JmesPathResult
    throws baml.errors.Io | baml.errors.Timeout | baml.json.JsonParseError
```

The benchmark includes the pinned official JMESPath specification and all 892
correctness cases from the official compliance corpus. The JSON corpus is loaded
dynamically and each case is registered as a native BAML test.

See `SPEC.md` for the benchmark contract and `THIRD_PARTY_NOTICES.md` for exact
provenance and licensing.

```bash
baml check
baml test --list
baml test
```
