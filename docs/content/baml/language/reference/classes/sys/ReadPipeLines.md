---
title: "sys.ReadPipeLines"
description: "Class sys.ReadPipeLines from the generated baml package reference."
---

A line-oriented view over a `ReadPipe`.

It reads through to the pipe on demand and holds only bytes past the last
newline. The final unterminated line is yielded at EOF. Invalid UTF-8 is
replaced with U+FFFD.

```baml
class sys.ReadPipeLines
```

## Fields

### pipe

```baml
pipe: baml.sys.ReadPipe
```

No description is available yet.

### _pending

```baml
_pending: uint8array
```

No description is available yet.

_Source: `<builtin>/baml/ns_sys/sys.baml:1825`_
