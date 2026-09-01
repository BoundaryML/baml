---
title: "Map"
description: "Class Map from the generated baml package reference."
---

An insertion-ordered collection of key-value pairs.

Iteration order is the order in which keys were first inserted: `keys()`
and `values()` follow it, replacing an existing key with `set` keeps the
key's original position, and `delete` preserves the order of the
remaining entries.

```baml
class Map<K, V>
```

## Methods

### clear

```baml
function clear(self: map<K, V>) -> null
```

No description is available yet.

### delete

```baml
function delete(self: map<K, V>, key: K) -> V | null
```

No description is available yet.

### get

```baml
function get(self: map<K, V>, key: K) -> V | null
```

No description is available yet.

### get_or_insert

```baml
function get_or_insert(self: map<K, V>, key: K, default: V) -> V
```

No description is available yet.

### has

```baml
function has(self: map<K, V>, key: K) -> bool
```

No description is available yet.

### keys

```baml
function keys(self: map<K, V>) -> K[]
```

No description is available yet.

### length

```baml
function length(self: map<K, V>) -> int
```

Returns the number of entries in the map.

### set

```baml
function set(self: map<K, V>, key: K, value: V) -> V | null
```

No description is available yet.

### values

```baml
function values(self: map<K, V>) -> V[]
```

Returns an array of all values in the map. Order matches `keys()`.

_Source: `<builtin>/baml/containers.baml:20749`_
