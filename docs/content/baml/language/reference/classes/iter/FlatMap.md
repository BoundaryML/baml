---
title: "iter.FlatMap"
description: "Class iter.FlatMap from the generated baml package reference."
---

No description is available yet.

```baml
class iter.FlatMap<T, R, E, E2, E3>
```

## Fields

### source

```baml
source: baml.iter.Iterator<Error = E, Item = T>
```

No description is available yet.

### func

```baml
func: (T) -> baml.iter.Iterable<Error = E3, Item = R> throws E2
```

No description is available yet.

### inner

```baml
inner: baml.iter.Iterator<Error = E3, Item = R> | baml.iter.Done
```

No description is available yet.

_Source: `<builtin>/baml/ns_iter/iter.baml:17143`_
