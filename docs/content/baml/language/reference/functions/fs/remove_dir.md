---
title: "fs.remove_dir"
description: "Function fs.remove_dir from the generated baml package reference."
---

Removes the empty directory at `path`. Throws `Io` if `path` is not a
directory, is not empty, or does not exist. Mirrors Bun's `fs.promises.rmdir`.

```baml
function fs.remove_dir(path: string) -> null throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_fs/fs.baml:4456`_
