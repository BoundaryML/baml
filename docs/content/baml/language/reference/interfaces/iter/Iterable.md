---
title: "iter.Iterable"
description: "Interface iter.Iterable from the generated baml package reference."
---

Implemented by types that can be iterated over.

```baml
interface iter.Iterable
```

## Associated types

### Item

```baml
type Item
```

No description is available yet.

### Error

```baml
type Error
```

No description is available yet.

## Required methods

### iter

```baml
function iter(self: Self) -> baml.iter.Iterator<Error = (Self as baml.iter.Iterable).Error, Item = (Self as baml.iter.Iterable).Item>
```

Creates an iterator over the value.

_Source: `<builtin>/baml/ns_iter/iter.baml:169`_
