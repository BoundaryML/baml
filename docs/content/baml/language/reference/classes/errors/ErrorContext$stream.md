---
title: "errors.ErrorContext$stream"
description: "Class errors.ErrorContext$stream from the generated baml package reference."
---

The temporal context of a thrown error: the error value itself, where it
was thrown, and the error it superseded while the scope was unwinding (its
`cause`), if any. Bound by the second parameter of a `catch (e, ctx)`
handler.

The chain runs newest → oldest through `cause`; `root_cause` walks to the
original failure at the tail, and `to_string` renders the whole chain
Python-style ("During handling of the above error, another error occurred").

```baml
class errors.ErrorContext$stream
```

## Fields

### error

```baml
error: unknown
```

No description is available yet.

### stack_trace

```baml
stack_trace: baml.errors.StackTrace$stream | null
```

No description is available yet.

### cause

```baml
cause: baml.errors.ErrorContext$stream | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_errors/error_context.baml:0`_
