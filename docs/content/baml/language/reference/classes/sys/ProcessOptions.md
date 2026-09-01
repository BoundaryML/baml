---
title: "sys.ProcessOptions"
description: "Class sys.ProcessOptions from the generated baml package reference."
---

Options for `exec`, `start_process`, and `shell`.

```baml
class sys.ProcessOptions
```

## Fields

### cwd

```baml
cwd: string | null
```

Working directory for the child process.

### env

```baml
env: map<string, string> | null
```

Environment variables to set for the child process.

### timeout_ms

```baml
timeout_ms: int | null
```

Maximum time in milliseconds to wait for the process to complete.

### stdin

```baml
stdin: string | null
```

The child process's complete stdin. `start_process` writes it and then
closes the pipe. Leave it unset to drive `Process.stdin` incrementally.

### stderr

```baml
stderr: baml.sys.StderrMode | null
```

Where the child's stderr goes. `null` means `StderrMode.Inherit`, and
only `StderrMode.Pipe` populates `Process.stderr`.

_Source: `<builtin>/baml/ns_sys/sys.baml:54`_
