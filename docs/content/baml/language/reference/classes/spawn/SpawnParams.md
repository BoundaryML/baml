---
title: "spawn.SpawnParams"
description: "Class spawn.SpawnParams from the generated baml package reference."
---

BEP-034 — spawn options and `with` middleware.

This namespace holds the values a `spawn` can be configured with: a
`CancelToken` for cooperative cancellation, a `TaskGroup` for rate
limiting, and the `detach` flag — plus `SpawnParams`, the value a
`spawn ... with` middleware pipeline transforms before the runtime
creates the future.
Every `spawn` implicitly constructs a `SpawnParams` from its name and
body; the `with` pipeline transforms it left-to-right before the runtime
creates the future. Each transformer is an ordinary function
`(SpawnParams<T, E>) -> SpawnParams<U, F>` — it may set config fields,
wrap `body` (retry, timing), or replace it entirely (type-changing
transformers like a fallback that erases the error type).

NOTE for the runtime: the engine reads these fields BY INDEX at the spawn
dispatch site (body=0, name=1, group=2, cancel=3, detach=4) — keep the
declaration order in sync with `bex_engine`'s `read_spawn_params`.

```baml
class spawn.SpawnParams<T, E>
```

## Fields

### body

```baml
body: () -> T throws E
```

No description is available yet.

### name

```baml
name: string | null
```

No description is available yet.

### group

```baml
group: baml.spawn.TaskGroup | null
```

No description is available yet.

### cancel

```baml
cancel: baml.spawn.CancelToken | null
```

No description is available yet.

### detach

```baml
detach: bool
```

No description is available yet.

_Source: `<builtin>/baml/ns_spawn/spawn.baml:1024`_
