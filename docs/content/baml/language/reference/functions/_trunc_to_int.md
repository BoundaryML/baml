---
title: "_trunc_to_int"
description: "Function _trunc_to_int from the generated baml package reference."
---

Truncates `value` toward zero and returns the integer part, **saturating**
to the `int` range and mapping NaN to `0` — it never throws.

Private helper (leading `_`; formerly the public `baml.math.trunc`) backing
internal retry-delay math. User code should prefer the
range-checked, throwing `float.itrunc()` (or the float-returning
`float.trunc()`); this saturating form is intentionally not public surface.

```baml
function _trunc_to_int(value: float) -> int
```

_Source: `<builtin>/baml/float.baml:17177`_
