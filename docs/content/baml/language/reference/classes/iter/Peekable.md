---
title: "iter.Peekable"
description: "Class iter.Peekable from the generated baml package reference."
---

No description is available yet.

```baml
class iter.Peekable<T, E>
```

## Fields

### source

```baml
source: baml.iter.Iterator<Error = E, Item = T>
```

No description is available yet.

### buffer

```baml
buffer: T | baml.iter.Done
```

No description is available yet.

### has_buffer

```baml
has_buffer: bool
```

No description is available yet.

## Methods

### peek

```baml
function peek(self: baml.iter.Peekable<T, E>) -> T | baml.iter.Done throws E
```

No description is available yet.

_Source: `<builtin>/baml/ns_iter/iter.baml:20092`_
