---
title: "spawn.CancelToken$stream"
description: "Class spawn.CancelToken$stream from the generated baml package reference."
---

A cooperative, one-shot cancellation handle backed by a runtime
cancellation token. Passing one via `options(cancel = ...)` links it into a
spawned task's effective token: once fired, the task's next `await` throws
`baml.panics.Cancelled`. Once cancelled, a token stays cancelled forever.

```baml
class spawn.CancelToken$stream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

_Source: `<builtin>/baml/ns_spawn/spawn.baml:0`_
