---
title: "sys.ReadPipe"
description: "Class sys.ReadPipe from the generated baml package reference."
---

A system pipe for reading data, typically from another process.

```baml
class sys.ReadPipe
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
function close(self: baml.sys.ReadPipe) -> void throws baml.errors.Io
```

Close the read pipe, releasing any resources.

### lines

```baml
function lines(self: baml.sys.ReadPipe) -> baml.sys.ReadPipeLines
```

Return a line-oriented view over this pipe.

_Source: `<builtin>/baml/ns_sys/sys.baml:4817`_
