---
title: "json.schema"
description: "Function json.schema from the generated baml package reference."
---

Lower a BAML `type` to ordinary, provider-neutral JSON Schema. Optional
fields are omitted from `required`, maps use schema-valued
`additionalProperties`, unions use `anyOf`, and recursive class graphs use
`$defs` and `$ref`.

This is the type-level counterpart to the value-level conversions above:
everything else here moves a *value* between BAML and `json`, while this
describes the *shape* a value of `t` would take.

Throws `root.errors.Unsupported` for constructs without a JSON Schema form.

```baml
function json.schema(t: reflect.Type) -> baml.json.json throws baml.errors.Unsupported
```

_Source: `<builtin>/baml/ns_json/json.baml:12050`_
