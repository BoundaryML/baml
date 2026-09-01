---
title: "json.deserialize"
description: "Function json.deserialize from the generated baml package reference."
---

Parse JSON text into a value of type `T`, honoring user-defined
`from_json` overrides on classes (via [`from_json<T>`]). Equivalent to
`from_json<T>(parse(s))`.

Use this from a host that wants to coerce a JSON-shaped input (such as
`--json-args`) into a typed BAML value while respecting class-level
`from_json` overrides. The structural path is [`from_string<T>`], which
decodes structurally and bypasses overrides.

HACK: same story as [`serialize<T>`] — this wrapper exists so
`baml_exec::dispatch::build_args_from_signature` can resolve a single
stable name (`baml.json.deserialize`) to coerce `--json-args` payloads
through `from_json<T>(parse(s))`. Inlining the composition into the
Rust caller would mean threading two generic engine calls + their
throws-types instead of one, so the wrapper pays for itself.

```baml
function json.deserialize<T>(s: string) -> T throws baml.json.JsonParseError | baml.json.JsonDecodeError
```

_Source: `<builtin>/baml/ns_json/json.baml:13930`_
