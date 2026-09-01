---
title: "Summable"
description: "Interface Summable from the generated baml package reference."
---

Summation for numeric arrays.

Implemented for `int[]` (returning `int`) and `float[]` (returning `float`).
`Sum` is the element's own numeric type, so `xs.sum()` stays in the array's
number domain with no implicit widening — `int` and `float` are distinct
types, and there is deliberately no `int[] -> float` sum. To sum integer
data as floats, map first: `ints.map((x: int) -> float { x * 1.0 }).sum()`.

The method form `xs.sum()` is the only public surface; it delegates to the
private native `_sum_int` / `_sum_float` helpers declared below.

```baml
interface Summable
```

## Associated types

### Sum

```baml
type Sum
```

No description is available yet.

## Required methods

### sum

```baml
function sum(self: Self) -> (Self as baml.Summable).Sum
```

No description is available yet.

_Source: `<builtin>/baml/containers.baml:24404`_
