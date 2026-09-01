---
title: "spawn.CancelToken"
description: "Class spawn.CancelToken from the generated baml package reference."
---

A cooperative, one-shot cancellation handle backed by a runtime
cancellation token. Passing one via `options(cancel = ...)` links it into a
spawned task's effective token: once fired, the task's next `await` throws
`baml.panics.Cancelled`. Once cancelled, a token stays cancelled forever.

```baml
class spawn.CancelToken
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### any

```baml
function any(tokens: baml.spawn.CancelToken[]) -> baml.spawn.CancelToken
```

No description is available yet.

### cancel

```baml
function cancel(self: baml.spawn.CancelToken) -> int
```

No description is available yet.

### is_cancelled

```baml
function is_cancelled(self: baml.spawn.CancelToken) -> bool
```

No description is available yet.

### new

```baml
function new() -> baml.spawn.CancelToken
```

No description is available yet.

_Source: `<builtin>/baml/ns_spawn/spawn.baml:3252`_
