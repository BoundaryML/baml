---
title: "sys.StderrMode"
description: "Enum sys.StderrMode from the generated baml package reference."
---

Where a child process's standard error goes.

```baml
enum sys.StderrMode
```

## Variants

### Inherit

The child writes to this process's stderr. This is the default.

### Pipe

The child's stderr becomes `Process.stderr`. The caller must drain it
concurrently with stdout to prevent the child from blocking.

### Discard

The child's stderr is discarded.

_Source: `<builtin>/baml/ns_sys/sys.baml:724`_
