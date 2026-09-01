---
title: "env.Ref$stream"
description: "Class env.Ref$stream from the generated baml package reference."
---

A LATE-BOUND reference to an environment variable: pure data carrying the
variable's NAME, never its value.

`env.SOME_VAR` desugars to `baml.env.ref("SOME_VAR")`, which constructs one
of these. Nothing is read at construction time, so a `client Foo = ...`
declaration — evaluated during `$init`, which cannot run io sysops — can
name a variable without dying on an opaque `InitFailed`, and a host that
loads its secrets AFTER the runtime initializes still gets the fresh value:
the read happens at USE time, inside `get()`.

A `Ref` is also what keeps secrets out of constructed values. The name is
the only thing captured, so a client holding `api_key: env.OPENAI_API_KEY`
never has the key inside it — printing, serializing, or journaling that
client cannot leak the credential.

```baml
class env.Ref$stream
```

## Fields

### name

```baml
name: string | null
```

The environment variable's name. Never its value.

_Source: `<builtin>/baml/ns_env/env.baml:0`_
