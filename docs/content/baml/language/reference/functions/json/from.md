---
title: "json.from"
description: "Function json.from from the generated baml package reference."
---

Serialize any BAML value `value` of type `T` to its `json` representation,
honoring user-defined `baml.ToJson` overrides on classes at every depth.

The json analog of `string.from`: a structural conversion that renders
primitives, lists, maps, class instances (as `{"field": value}`), enums (as
their variant name), and media (as the tagged form) to their natural json
shape, routing any value whose runtime class `implements baml.ToJson` through
that override. Resolution is on the value's *runtime* class (via the
`baml._to_json_shim` native), for the same package-boundary reason
`string.from` uses `_to_string_shim` rather than the literal `is baml.ToJson`
match form.

Throws `JsonSerializationError` for non-representable values (`uint8array`
without explicit encoding, function values, etc.).

```baml
function json.from<T>(value: T) -> baml.json.json throws baml.json.JsonSerializationError
```

_Source: `<builtin>/baml/ns_json/json.baml:3285`_
