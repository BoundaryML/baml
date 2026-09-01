---
title: "sys.Process$stream"
description: "Class sys.Process$stream from the generated baml package reference."
---

A live child process created with `start_process`.

`stdin` and `stdout` are always caller-owned pipes. `stderr` is populated
only when `ProcessOptions.stderr` is `StderrMode.Pipe`.

```baml
class sys.Process$stream
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

### stdin

```baml
stdin: baml.sys.WritePipe$stream | null
```

No description is available yet.

### stdout

```baml
stdout: baml.sys.ReadPipe$stream | null
```

No description is available yet.

### stderr

```baml
stderr: baml.sys.ReadPipe$stream | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_sys/sys.baml:0`_
