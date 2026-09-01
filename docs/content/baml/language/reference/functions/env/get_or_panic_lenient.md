---
title: "env.get_or_panic_lenient"
description: "Function env.get_or_panic_lenient from the generated baml package reference."
---

Like `get_or_panic`, but tolerant of a missing variable when `lenient` is
true: a missing variable yields the empty string instead of panicking.

Used by generated client constructors so that offline spec prompt rendering
can build a client for its provider/role metadata without requiring
credentials. The direct network call constructs
with `lenient = false`, so a missing `api_key` env var still panics for it,
exactly as before.

```baml
function env.get_or_panic_lenient(key: string, lenient: bool) -> string throws baml.errors.Io | baml.errors.ParseError
```

_Source: `<builtin>/baml/ns_env/env.baml:2730`_
