---
title: "glob.ScanOptions$stream"
description: "Class glob.ScanOptions$stream from the generated baml package reference."
---

Options for `Glob.scan`.

```baml
class glob.ScanOptions$stream
```

## Fields

### cwd

```baml
cwd: string | null
```

Working directory to resolve relative patterns from.

### dot

```baml
dot: bool | null
```

Include dotfiles (hidden files starting with `.`).

### absolute

```baml
absolute: bool | null
```

Return absolute paths instead of relative paths.

### follow_symlinks

```baml
follow_symlinks: bool | null
```

Follow symbolic links when scanning.

### throw_error_on_broken_symlink

```baml
throw_error_on_broken_symlink: bool | null
```

Throw an error if a broken symbolic link is encountered.

### only_files

```baml
only_files: bool | null
```

Only return files, not directories.

_Source: `<builtin>/baml/ns_glob/glob.baml:0`_
