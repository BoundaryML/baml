---
title: "fs.symlink"
description: "Function fs.symlink from the generated baml package reference."
---

Creates a symbolic link at `path` that points to `target`.

`target` is stored verbatim and need not exist: a dangling link is created
if it does not. A relative `target` is resolved by the operating system
against the directory containing `path`, not against the working directory.

Throws `Io` if `path` already exists.

On Windows the link is created as a directory link when `target` resolves to
an existing directory and as a file link otherwise (a dangling link is
always a file link), and creating one requires Developer Mode or
`SeCreateSymbolicLinkPrivilege`; without it the OS refuses and this throws
`Io`.

```baml
function fs.symlink(target: string, path: string) -> void throws baml.errors.Io
```

_Source: `<builtin>/baml/ns_fs/fs.baml:6818`_
