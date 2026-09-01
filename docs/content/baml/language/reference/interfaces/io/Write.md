---
title: "io.Write"
description: "Interface io.Write from the generated baml package reference."
---

No description is available yet.

```baml
interface io.Write
```

## Required methods

### write_some

```baml
function write_some(self: Self, data: uint8array) -> int throws baml.errors.Io
```

Attempt one write of `data` and return the number of bytes accepted.
A partial write is successful.

### flush

```baml
function flush(self: Self) -> void throws baml.errors.Io
```

Push data buffered by this writer toward its destination.

## Default methods

### write

```baml
function write(self: Self, data: string | uint8array) -> int throws baml.errors.Io
```

Write all of `data` to the destination.
Strings are encoded as UTF-8.
Returns the number of bytes written.

_Source: `<builtin>/baml/ns_io/write.baml:0`_
