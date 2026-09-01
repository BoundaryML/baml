---
title: "sys.Process"
description: "Class sys.Process from the generated baml package reference."
---

A live child process created with `start_process`.

`stdin` and `stdout` are always caller-owned pipes. `stderr` is populated
only when `ProcessOptions.stderr` is `StderrMode.Pipe`.

```baml
class sys.Process
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

### stdin

```baml
stdin: baml.sys.WritePipe
```

No description is available yet.

### stdout

```baml
stdout: baml.sys.ReadPipe
```

No description is available yet.

### stderr

```baml
stderr: baml.sys.ReadPipe | null
```

No description is available yet.

## Methods

### close

```baml
function close(self: baml.sys.Process) -> null
```

Terminate an un-waited child and close its stdin, stdout, and stderr pipes.

Safe to use with `defer { process.close() }`.

### kill

```baml
function kill(self: baml.sys.Process) -> void throws baml.errors.Io
```

Request that the child terminate.

### wait

```baml
function wait(self: baml.sys.Process) -> baml.sys.ProcessExit throws baml.errors.Io | baml.errors.Timeout
```

Wait for the child to exit.

_Source: `<builtin>/baml/ns_sys/sys.baml:4126`_
