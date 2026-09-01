---
title: "ops.ShiftLeft"
description: "Interface ops.ShiftLeft from the generated baml package reference."
---

`self << rhs`

```baml
interface ops.ShiftLeft<Rhs extends baml.Concrete>
```

## Associated types

### Output

```baml
type Output
```

No description is available yet.

## Required methods

### shl

```baml
function shl(self: Self, rhs: Rhs) -> (Self as baml.ops.ShiftLeft<Rhs>).Output
```

No description is available yet.

_Source: `<builtin>/baml/ns_ops/bitwise.baml:2217`_
