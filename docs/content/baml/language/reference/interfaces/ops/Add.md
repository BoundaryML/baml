---
title: "ops.Add"
description: "Interface ops.Add from the generated baml package reference."
---

`self + rhs`

```baml
interface ops.Add<Rhs extends baml.Concrete>
```

## Associated types

### Output

```baml
type Output
```

No description is available yet.

## Required methods

### add

```baml
function add(self: Self, rhs: Rhs) -> (Self as baml.ops.Add<Rhs>).Output
```

The `+` operator.

_Source: `<builtin>/baml/ns_ops/math.baml:1411`_
