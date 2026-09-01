---
title: "json.to_string"
description: "Function json.to_string from the generated baml package reference."
---

Serialize any BAML value to its canonical JSON string, dispatching on the
value's runtime type and honoring `baml.ToJson` overrides at every depth.

Throws `JsonSerializationError` for non-representable types
(`uint8array` without explicit encoding, function values, etc.).

```baml
function json.to_string(value: unknown) -> string throws baml.json.JsonSerializationError
```

_Source: `<builtin>/baml/ns_json/json.baml:1364`_
