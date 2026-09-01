---
title: "iter.Repeat"
description: "Class iter.Repeat from the generated baml package reference."
---

An iterator that returns the same value a specified number of times.

```baml
class iter.Repeat<T>
```

## Fields

### value

```baml
value: T
```

No description is available yet.

### count

```baml
count: int
```

No description is available yet.

## Methods

### new

```baml
function new(value: T, count: int) -> baml.iter.Repeat<T>
```

Creates a new repeating iterator.
If `count` is negative (or not specified), it will repeat forever.

_Source: `<builtin>/baml/ns_iter/iter.baml:13913`_
