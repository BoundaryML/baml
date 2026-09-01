---
title: "fs.chmod"
description: "Function fs.chmod from the generated baml package reference."
---

Changes the permissions of the file at `path` to `mode`.

`mode` is the POSIX (unix) mode octal permissions value:
a numeric bitmask typically using three octal digits `0oUGO` (permissions for user/group/other)
Where each octal digit represents bit flags:
- `0o4` = read
- `0o2` = write
- `0o1` = execute

An optional fourth leading digit carries the setuid (`0o4000`), setgid
(`0o2000`), and sticky (`0o1000`) bits on platforms where these are supported.
`mode` must therefore be in `0 ..= 0o7777`; anything outside that range throws
`InvalidArgument` rather than being silently masked down to it.

On Windows there is no mode as a file is either writable or read-only.
Only the owner-write bit (`0o200`) has any effect: setting it clears the
read-only attribute and clearing it sets it. Every other bit, including the
group and other digits, is accepted and ignored. `path` must exist on
Windows even when the resulting attribute is unchanged.

```baml
function fs.chmod(path: string, mode: int) -> void throws baml.errors.Io | baml.errors.InvalidArgument
```

_Source: `<builtin>/baml/ns_fs/fs.baml:6014`_
