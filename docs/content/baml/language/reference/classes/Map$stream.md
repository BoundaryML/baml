---
title: "Map$stream"
description: "Class Map$stream from the generated baml package reference."
---

An insertion-ordered collection of key-value pairs.

Iteration order is the order in which keys were first inserted: `keys()`
and `values()` follow it, replacing an existing key with `set` keeps the
key's original position, and `delete` preserves the order of the
remaining entries.

```baml
class Map$stream<K, V>
```

_Source: `<builtin>/baml/containers.baml:0`_
