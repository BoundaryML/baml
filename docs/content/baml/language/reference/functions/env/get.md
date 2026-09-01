---
title: "env.get"
description: "Function env.get from the generated baml package reference."
---

Returns the value of the environment variable `key`, or `null` if not set.

Throws `ParseError` if the variable is set but its value is not valid
UTF-8 (the native bridge surfaces `std::env::VarError::NotUnicode` as a
catchable parse error).

```baml
function env.get(key: string) -> string | null throws baml.errors.Io | baml.errors.ParseError
```

_Source: `<builtin>/baml/ns_env/env.baml:261`_
