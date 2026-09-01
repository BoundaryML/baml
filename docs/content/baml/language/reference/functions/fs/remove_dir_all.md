---
title: "fs.remove_dir_all"
description: "Function fs.remove_dir_all from the generated baml package reference."
---

Recursively removes the directory at `path` and all of its contents.
Idempotent: returns successfully if `path` does not exist. Like Bun's
`fs.promises.rm(path, { recursive: true, force: true })` for directory trees
and missing paths — but, unlike `rm -rf`, it targets directories only and
throws `Io` if `path` is a regular file.

```baml
function fs.remove_dir_all(path: string) -> null throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_fs/fs.baml:4900`_
