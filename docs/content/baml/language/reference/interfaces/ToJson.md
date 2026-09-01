---
title: "ToJson"
description: "Interface ToJson from the generated baml package reference."
---

Opt-in JSON conversion.

A type implements `baml.ToJson` to control how `baml.json.from(value)`
renders it. This is the json analog of `baml.ToString`: `baml.json.from`
provides a default structural rendering for non-implementors (primitives
natural, lists/maps/instances structural), and an implementor customizes its
own json shape.

```
class Temp {
  celsius float
  implements baml.ToJson {
    function to_json(self) -> baml.json.json throws baml.json.JsonSerializationError {
      { "c": baml.json.from(self.celsius), "f": baml.json.from(self.celsius * 1.8 + 32.0) }
    }
  }
}
```

Unlike `to_string`, conversion can fail: a value with no json representation
(`uint8array` without explicit encoding, function values, ...) throws
`baml.json.JsonSerializationError`.

`to_json` has a default body — the same structural rendering `baml.json.from`
falls back to for non-implementors — so `implements baml.ToJson {}` with no
override is valid and renders identically to a non-implementor.

```baml
interface ToJson
```

## Default methods

### to_json

```baml
function to_json(self: Self) -> baml.json.json throws baml.json.JsonSerializationError
```

No description is available yet.

_Source: `<builtin>/baml/conversions.baml:4950`_
