---
title: "errors.UnknownError"
description: "Class errors.UnknownError from the generated baml package reference."
---

Universal wrapper for errors that do not implement a known capability channel.

```baml
class errors.UnknownError
```

## Fields

### data

```baml
data: unknown
```

No description is available yet.

### message

```baml
message: string[]
```

No description is available yet.

## Methods

### _preserve_context

```baml
function _preserve_context(source: unknown, target: unknown) -> null
```

No description is available yet.

### from

```baml
function from<T>(data: unknown) -> T | baml.errors.UnknownError
```

Preserve a known `T`, recover a wrapped `T`, pass an existing
`UnknownError` through, or wrap an otherwise unknown value.

### with_message

```baml
function with_message<T>(data: unknown, message: string) -> T | baml.errors.UnknownError
```

As with `from`, but append a breadcrumb while the value remains
unknown.

_Source: `<builtin>/baml/ns_errors/unknown_error.baml:83`_
