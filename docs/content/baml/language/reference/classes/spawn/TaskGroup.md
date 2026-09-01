---
title: "spawn.TaskGroup"
description: "Class spawn.TaskGroup from the generated baml package reference."
---

Caps how many spawns referencing it run concurrently. Excess spawns queue
FIFO and start as earlier ones settle; the `spawn` still returns its
`Future` immediately, so queueing is invisible. Two spawns share a limit iff
they reference the same `TaskGroup` value — there is no global registry.

```baml
class spawn.TaskGroup
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### active_count

```baml
function active_count(self: baml.spawn.TaskGroup) -> int
```

No description is available yet.

### cancel

```baml
function cancel(self: baml.spawn.TaskGroup, pending: bool | null, active: bool | null) -> int
```

No description is available yet.

### limit

```baml
function limit(self: baml.spawn.TaskGroup) -> int
```

No description is available yet.

### name

```baml
function name(self: baml.spawn.TaskGroup) -> string | null
```

No description is available yet.

### new

```baml
function new(limit: int, name: string | null) -> baml.spawn.TaskGroup
```

No description is available yet.

### queued_count

```baml
function queued_count(self: baml.spawn.TaskGroup) -> int
```

No description is available yet.

### set_limit

```baml
function set_limit(self: baml.spawn.TaskGroup, limit: int) -> void
```

No description is available yet.

_Source: `<builtin>/baml/ns_spawn/spawn.baml:1478`_
