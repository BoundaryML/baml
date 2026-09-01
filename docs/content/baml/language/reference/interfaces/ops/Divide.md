---
title: "ops.Divide"
description: "Interface ops.Divide from the generated baml package reference."
---

`self / rhs`

```baml
interface ops.Divide<Rhs extends baml.Concrete>
```

## Associated types

### Output

```baml
type Output
```

No description is available yet.

## Required methods

### div

```baml
function div(self: Self, rhs: Rhs) -> (Self as baml.ops.Divide<Rhs>).Output
```

The `/` operator.

_Source: `<builtin>/baml/ns_ops/math.baml:2057`_
