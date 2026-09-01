---
title: "fs.remove"
description: "Function fs.remove from the generated baml package reference."
---

Removes the file at `path`. Throws `Io` if the file does not exist or cannot be deleted.

This handles regular files only. To delete a directory use `remove_dir`
(empty directories) or `remove_dir_all` (directory trees).

```baml
function fs.remove(path: string) -> null throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_fs/fs.baml:2718`_
