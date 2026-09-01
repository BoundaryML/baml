---
title: "sys.ShellOutput"
description: "Class sys.ShellOutput from the generated baml package reference."
---

The result of running an external process with `exec` or `shell`.

```baml
class sys.ShellOutput
```

## Fields

### stdout

```baml
stdout: uint8array
```

No description is available yet.

### stderr

```baml
stderr: uint8array
```

No description is available yet.

### exit_code

```baml
exit_code: int
```

No description is available yet.

## Methods

### ok

```baml
function ok(self: baml.sys.ShellOutput) -> bool
```

Returns `true` if the process exited with code `0`.

_Source: `<builtin>/baml/ns_sys/sys.baml:1111`_
