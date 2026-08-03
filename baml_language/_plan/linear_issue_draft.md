# Linear issue draft (could not file: Linear connector not authorized in this session)

**Title:** `baml.json.to_string<T>` fails at runtime when `T = unknown`

**Labels:** bug, runtime, stdlib

**Body:**

`baml.json.to_string(v)` throws `cannot serialize unknown type` at runtime
when `v`'s static type is `unknown`, even though the dynamic value is
perfectly serializable — `baml.json.to_json(v)` walks the same dynamic
value fine.

Repro (toolchain 0.15.1-nightly.20260727, still present on canary as of
2026-08-03):

```baml
function repro() -> string {
    let f: baml.AnyFunction = some_tool;
    let out = reflect.call_any(f, { "x": 1 });   // out: unknown
    baml.json.to_string(out)                      // runtime error
}
```

Why it matters: this bites immediately when combining `reflect.call_any`
(returns `unknown`) with JSON-encoding tool results — the exact pattern of
any reflection-driven tool dispatcher (see the ai_agents reference impl in
`_plan/ai_agents/`, `ns_ai/toolbox.baml`).

Expected: either
1. `to_string<unknown>` falls back to the dynamic type of the value
   (matching `to_json`'s behavior), or
2. it is rejected at compile time with a clear message, not a runtime
   throw.

Option 1 preferred — `to_json` already proves the walk is well-defined.

Workaround: `baml.json.stringify(baml.json.to_json(v))`.
