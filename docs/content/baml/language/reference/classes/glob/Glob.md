---
title: "glob.Glob"
description: "Class glob.Glob from the generated baml package reference."
---

A compiled glob pattern. Create one with `baml.glob.new(pattern)`.

```baml
class glob.Glob
```

## Fields

### _handle

```baml
_handle: $rust_type
```

No description is available yet.

## Methods

### matches

```baml
function matches(self: baml.glob.Glob, path: string) -> bool throws baml.errors.Io
```

Returns `true` if `path` matches this glob pattern.

### scan

```baml
function scan(self: baml.glob.Glob, root: string | baml.glob.ScanOptions) -> string[] throws baml.errors.Io
```

Scans the filesystem and returns all paths matching this glob pattern.
Pass a `string` root path or a `ScanOptions` object for more control.

_Source: `<builtin>/baml/ns_glob/glob.baml:602`_
