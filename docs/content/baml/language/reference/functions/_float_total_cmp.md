---
title: "_float_total_cmp"
description: "Function _float_total_cmp from the generated baml package reference."
---

Three-way compares two floats by IEEE 754 `totalOrder` (`f64::total_cmp`).

Internal shim backing `Comparable for float` (above) and kept bit-exact
with the float comparator inside the native `_rust_sort`; not part of the
public `Comparable`/`Sortable` surface.

```baml
function _float_total_cmp(a: float, b: float) -> int
```

_Source: `<builtin>/baml/comparable.baml:5626`_
