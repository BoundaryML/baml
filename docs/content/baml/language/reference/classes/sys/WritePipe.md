---
title: "sys.WritePipe"
description: "Class sys.WritePipe from the generated baml package reference."
---

A system pipe for writing data, typically to another process.

```baml
class sys.WritePipe
```

## Fields

### _pipe

```baml
_pipe: $rust_type
```

No description is available yet.

## Methods

### close

```baml
function close(self: baml.sys.WritePipe) -> void throws baml.errors.Io
```

Close the write pipe, flushing pending data and delivering EOF.

_Source: `<builtin>/baml/ns_sys/sys.baml:5394`_
