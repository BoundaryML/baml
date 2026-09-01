---
title: "panics.HostUnavailable$stream"
description: "Class panics.HostUnavailable$stream from the generated baml package reference."
---

A required host resource is unavailable — e.g. the system entropy source
(`getrandom`) is not accessible in a sandboxed/isolated environment.
`resource` identifies which subsystem is unavailable (`"entropy"`, etc.).

```baml
class panics.HostUnavailable$stream
```

## Fields

### resource

```baml
resource: string | null
```

No description is available yet.

### message

```baml
message: string | null
```

No description is available yet.

_Source: `<builtin>/baml/ns_panics/panics.baml:0`_
