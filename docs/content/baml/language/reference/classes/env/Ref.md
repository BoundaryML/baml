---
title: "env.Ref"
description: "Class env.Ref from the generated baml package reference."
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
class env.Ref
```

## Fields

### name

```baml
name: string
```

The environment variable's name. Never its value.

## Methods

### get

```baml
function get(self: baml.env.Ref) -> string | null throws baml.errors.Io | baml.errors.ParseError
```

Read the variable now, or `null` when it is not set.

### get_or_panic

```baml
function get_or_panic(self: baml.env.Ref) -> string throws baml.errors.Io | baml.errors.ParseError
```

Read the variable now. Panics, naming the variable, when it is not set.

### or

```baml
function or(self: baml.env.Ref, fallback: string) -> string throws baml.errors.Io | baml.errors.ParseError
```

Read the variable now, falling back to `fallback` when it is not set.

_Source: `<builtin>/baml/ns_env/env.baml:1400`_
