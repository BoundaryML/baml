---
title: "env.get_or_panic"
description: "Function env.get_or_panic from the generated baml package reference."
---

Returns the value of the environment variable `key`. Panics if not set.

```baml
function env.get_or_panic(key: string) -> string throws baml.errors.Io | baml.errors.ParseError
```

_Source: `<builtin>/baml/ns_env/env.baml:448`_
