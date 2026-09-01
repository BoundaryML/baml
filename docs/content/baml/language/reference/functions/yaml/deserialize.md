---
title: "yaml.deserialize"
description: "Function yaml.deserialize from the generated baml package reference."
---

Parses YAML text and decodes it to `T`, honoring user-defined `baml.FromJson`
overrides (via `baml.json.from_json<T>`). The YAML counterpart of
`baml.json.deserialize<T>`. Parsing throws `YamlParseError`; decoding the
resulting `json` into `T` throws `JsonDecodeError` (decoding never re-parses,
so no `JsonParseError` arises here).

```baml
function yaml.deserialize<T>(s: string) -> T throws baml.yaml.YamlParseError | baml.json.JsonDecodeError
```

_Source: `<builtin>/baml/ns_yaml/yaml.baml:1001`_
