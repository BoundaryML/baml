---
title: "spawn.TaskGroup$stream"
description: "Class spawn.TaskGroup$stream from the generated baml package reference."
---

Caps how many spawns referencing it run concurrently. Excess spawns queue
FIFO and start as earlier ones settle; the `spawn` still returns its
`Future` immediately, so queueing is invisible. Two spawns share a limit iff
they reference the same `TaskGroup` value — there is no global registry.

```baml
class spawn.TaskGroup$stream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

_Source: `<builtin>/baml/ns_spawn/spawn.baml:0`_
