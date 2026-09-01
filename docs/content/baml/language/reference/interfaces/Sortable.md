---
title: "Sortable"
description: "Interface Sortable from the generated baml package reference."
---

Sorting for collections whose elements have a natural order.

Implemented for `T[]` whenever `T implements Comparable`. `SortError` is
the element's `CompareError`: sorting a primitive array (`int[]`, `bigint[]`,
`string[]`, `float[]`) can never fail, and sorting a user type with a
fallible comparator propagates that comparator's error.

```baml
interface Sortable
```

## Associated types

### SortError

```baml
type SortError
```

No description is available yet.

## Required methods

### sort

```baml
function sort(self: Self) -> Self throws (Self as baml.Sortable).SortError
```

No description is available yet.

_Source: `<builtin>/baml/comparable.baml:2235`_
