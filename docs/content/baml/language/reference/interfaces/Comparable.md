---
title: "Comparable"
description: "Interface Comparable from the generated baml package reference."
---

A total natural ordering for values of the implementing type.

`compare` follows the `sort_by` comparator convention: negative means
`self` orders before `other`, zero preserves relative order, positive
means `self` orders after `other`.

`CompareError` is the error a single comparison may throw. A total order that
never fails binds `type CompareError = never`; a fallible comparator binds
`type CompareError = MyError` and the error propagates to whoever compares (or
sorts) the values.

```
class Resume {
  name string
  implements Comparable {
    type CompareError = never
    function compare(self, other: Self) -> int throws never {
      self.name.compare(other.name)
    }
  }
}
```

Note: `CompareError` is intentionally *not* defaulted, so that every
implementor states its error type outright. Defaulting it would be sound —
a bare `T extends Comparable` bound pins nothing, so an implementor that
overrides the default still satisfies the bound and its error type flows
through — but a comparator's failure mode is worth writing down.

```baml
interface Comparable
```

## Associated types

### CompareError

```baml
type CompareError
```

No description is available yet.

## Required methods

### compare

```baml
function compare(self: Self, other: Self) -> int throws (Self as baml.Comparable).CompareError
```

No description is available yet.

_Source: `<builtin>/baml/comparable.baml:1162`_
