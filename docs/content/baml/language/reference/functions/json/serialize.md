---
title: "json.serialize"
description: "Function json.serialize from the generated baml package reference."
---

Serialize `v` to JSON text, honoring user-defined `to_json` overrides on
classes (via [`to_json`]). Equivalent to [`to_string(v)`].

Use this from a host that wants to print a target's return value as JSON
while still respecting class-level `to_json` overrides.

HACK: this is a thin shim that only exists so `baml_exec::dispatch`
has a stable named entry point to call from Rust (via
`engine.call_function("baml.json.serialize", ...)`). The composition
`to_string(v)` is what callers would write inline anyway;
owning a wrapper function gives us one engine-side symbol to look up
instead of two, and a place to evolve the override semantics if they
drift. Remove this if the host gains a direct way to invoke generic
compositions without a named landing pad.

```baml
function json.serialize<T>(v: T) -> string throws baml.json.JsonSerializationError
```

_Source: `<builtin>/baml/ns_json/json.baml:12963`_
