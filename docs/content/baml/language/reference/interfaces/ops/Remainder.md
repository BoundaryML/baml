---
title: "ops.Remainder"
description: "Interface ops.Remainder from the generated baml package reference."
---

`self % rhs`

```baml
interface ops.Remainder<Rhs extends baml.Concrete>
```

## Associated types

### Output

```baml
type Output
```

No description is available yet.

## Required methods

### rem

```baml
function rem(self: Self, rhs: Rhs) -> (Self as baml.ops.Remainder<Rhs>).Output
```

The `%` operator.

_Source: `<builtin>/baml/ns_ops/math.baml:2272`_
