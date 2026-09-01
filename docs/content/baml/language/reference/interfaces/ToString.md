---
title: "ToString"
description: "Interface ToString from the generated baml package reference."
---

Opt-in human-readable string conversion.

A type implements `baml.ToString` to control how `string.from(value)`
renders it. Conversion is total — `to_string` never throws.

```
class Point {
  x int
  y int
  implements baml.ToString {
    function to_string(self) -> string throws never {
      "(" + string.from(self.x) + ", " + string.from(self.y) + ")"
    }
  }
}
```

Unlike a magic method, `to_string` only exists on types that opt in.
`string.from` provides a default structural rendering for everything else
(see [`_to_string_default`]).

`to_string` has a default body — the same structural rendering `string.from`
falls back to for non-implementors — so `implements baml.ToString {}` with no
override is valid and renders identically to a non-implementor.

```baml
interface ToString
```

## Default methods

### to_string

```baml
function to_string(self: Self) -> string
```

No description is available yet.

_Source: `<builtin>/baml/conversions.baml:865`_
