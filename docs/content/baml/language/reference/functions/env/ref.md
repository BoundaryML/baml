---
title: "env.ref"
description: "Function env.ref from the generated baml package reference."
---

Construct a late-bound reference to the environment variable `name`.

This is what `env.NAME` sugar desugars to. It performs no io: the variable
is read when the returned `Ref` is used, not here.

```baml
function env.ref(name: string) -> baml.env.Ref
```

_Source: `<builtin>/baml/ns_env/env.baml:2191`_
