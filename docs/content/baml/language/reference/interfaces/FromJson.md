---
title: "FromJson"
description: "Interface FromJson from the generated baml package reference."
---

Opt-in JSON deserialization — the inverse of `baml.ToJson`.

A type implements `baml.FromJson` to control how `baml.json.to<T>(j)` (and
JSON deserialization generally) constructs it from a `json` value. Unlike
`to_json`, `from_json` has no `self`: it is an associated constructor that
takes a `json` and returns `Self`. `baml.json.to` provides a default
structural decode for non-implementors (per-field, honoring nested
`baml.FromJson` overrides), so a type need only implement `baml.FromJson`
when its JSON shape differs from its structural field layout.

```
class Temp {
  celsius float
  implements baml.FromJson {
    function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
      Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
    }
  }
}
```

`from_json` is a *required* method (no default body): a type either
implements `baml.FromJson` to customize decoding, or relies on the structural
decode that `baml.json.to` provides for non-implementors. (Unlike `to_json`,
it can't have a default body — an interface default method may not return
`Self`; the structural fallback lives in `baml.json.to` instead.)

```baml
interface FromJson
```

## Required methods

### from_json

```baml
function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError
```

No description is available yet.

_Source: `<builtin>/baml/conversions.baml:8032`_
