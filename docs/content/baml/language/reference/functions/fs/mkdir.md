---
title: "fs.mkdir"
description: "Function fs.mkdir from the generated baml package reference."
---

Creates the directory at `path`. Pass `MkdirOptions { recursive: true }` to create parent directories.

```baml
function fs.mkdir(path: string, options: baml.fs.MkdirOptions) -> null throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_fs/fs.baml:4190`_
