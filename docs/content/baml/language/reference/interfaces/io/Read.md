---
title: "io.Read"
description: "Interface io.Read from the generated baml package reference."
---

No description is available yet.

```baml
interface io.Read
```

## Required methods

### read

```baml
function read(self: Self, limit: int) -> uint8array | null throws baml.errors.Io
```

Read up to `limit` bytes from the source.
Returns `null` after EOF. A non-null result is never empty.

## Default methods

### bytes

```baml
function bytes(self: Self) -> uint8array throws baml.errors.Io
```

Consume all remaining bytes through EOF.

### text

```baml
function text(self: Self) -> string throws baml.errors.Io | baml.errors.ParseError
```

Consume all remaining bytes through EOF and decode strict UTF-8.

_Source: `<builtin>/baml/ns_io/read.baml:0`_
