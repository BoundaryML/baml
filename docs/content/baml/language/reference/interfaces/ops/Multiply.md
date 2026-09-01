---
title: "ops.Multiply"
description: "Interface ops.Multiply from the generated baml package reference."
---

`self * rhs`

```baml
interface ops.Multiply<Rhs extends baml.Concrete>
```

## Associated types

### Output

```baml
type Output
```

No description is available yet.

## Required methods

### mul

```baml
function mul(self: Self, rhs: Rhs) -> (Self as baml.ops.Multiply<Rhs>).Output
```

The `*` operator.

_Source: `<builtin>/baml/ns_ops/math.baml:1840`_
