---
title: "spawn.options"
description: "Function spawn.options from the generated baml package reference."
---

The built-in config transformer for a `spawn ... with` clause: sets the
provided fields, leaves everything else (including `body`) untouched, so
types are unchanged. `cancel` links a cancel token into the spawn's
effective token; `detach = true` opts the spawn out of the parent→child
cancel cascade and routes its unhandled errors to the root task instead
of the spawner; `group` enrolls the spawn in a `TaskGroup`, so it parks
until the group admits it (FIFO).

```baml
function spawn.options<T, E>(group: baml.spawn.TaskGroup | null, cancel: baml.spawn.CancelToken | null, detach: bool | null) -> (baml.spawn.SpawnParams<T, E>) -> baml.spawn.SpawnParams<T, E> throws never
```

_Source: `<builtin>/baml/ns_spawn/spawn.baml:4646`_
