---
title: "ops.Subtract"
description: "Interface ops.Subtract from the generated baml package reference."
---

`self - rhs`

```baml
interface ops.Subtract<Rhs extends baml.Concrete>
```

## Associated types

### Output

```baml
type Output
```

No description is available yet.

## Required methods

### sub

```baml
function sub(self: Self, rhs: Rhs) -> (Self as baml.ops.Subtract<Rhs>).Output
```

The `-` operator.

_Source: `<builtin>/baml/ns_ops/math.baml:1623`_
