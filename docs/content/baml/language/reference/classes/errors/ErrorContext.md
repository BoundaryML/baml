---
title: "errors.ErrorContext"
description: "Class errors.ErrorContext from the generated baml package reference."
---

The temporal context of a thrown error: the error value itself, where it
was thrown, and the error it superseded while the scope was unwinding (its
`cause`), if any. Bound by the second parameter of a `catch (e, ctx)`
handler.

The chain runs newest → oldest through `cause`; `root_cause` walks to the
original failure at the tail, and `to_string` renders the whole chain
Python-style ("During handling of the above error, another error occurred").

```baml
class errors.ErrorContext
```

## Fields

### error

```baml
error: unknown
```

No description is available yet.

### stack_trace

```baml
stack_trace: baml.errors.StackTrace
```

No description is available yet.

### cause

```baml
cause: baml.errors.ErrorContext | null
```

No description is available yet.

## Methods

### _to_string_impl

```baml
function _to_string_impl(self: baml.errors.ErrorContext) -> string
```

No description is available yet.

### root_cause

```baml
function root_cause(self: baml.errors.ErrorContext) -> baml.errors.ErrorContext
```

The original error at the tail of the cause chain — walks `cause` to the
deepest link. Returns `self` when nothing was superseded.

_Source: `<builtin>/baml/ns_errors/error_context.baml:482`_
