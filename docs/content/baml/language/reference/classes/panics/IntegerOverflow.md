---
title: "panics.IntegerOverflow"
description: "Class panics.IntegerOverflow from the generated baml package reference."
---

Raised when an `int` arithmetic operation (`+`, `-`, `*`, `/`, unary `-`)
overflows the representable range. `int` is a 63-bit signed integer
(`[-2^62, 2^62 - 1]`); operations that would fall outside it throw this
rather than silently wrapping. Use `bigint` for unbounded integers.

```baml
class panics.IntegerOverflow
```

## Fields

### message

```baml
message: string
```

No description is available yet.

_Source: `<builtin>/baml/ns_panics/panics.baml:2930`_
