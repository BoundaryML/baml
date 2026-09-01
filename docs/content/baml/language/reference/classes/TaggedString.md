---
title: "TaggedString"
description: "Class TaggedString from the generated baml package reference."
---

The structured value a tagged template literal produces.

BEP-049 §10. A `tag` ` `…` ` call site lowers to `tag(body = () -> TaggedString { … })`;
when `body()` runs it returns this struct. The invariant is
`parts.length == values.length + 1` (literals and values alternate, starting and
ending with a literal — empty strings fill the leading/trailing slot when the
template begins or ends with an interpolation).

```baml
class TaggedString
```

## Fields

### parts

```baml
parts: string[]
```

Literal text segments between interpolations, in source order.

### values

```baml
values: unknown[]
```

Interpolated values, in source order. Heterogeneous: each entry has the
type of its `${expr}` site; tags inspect element types at runtime.

_Source: `<builtin>/baml/core.baml:792`_
