# bridge_python e2e smoke demo

One-off, user-shaped walkthrough of the BEP-030 Python user journey:
write `.baml`, run `baml-cli generate`, import the generated SDK, call
a BAML function from typed Python, get a typed result back.

## Run it

```bash
./run.sh
```

`run.sh` builds `baml-cli`, generates `app/baml_sdk/`, builds the
`baml.baml_core` PyO3 extension via `maturin develop`, and runs
`app/app.py`. Idempotent — re-running regenerates the SDK from scratch.
Requires `uv` (https://docs.astral.sh/uv/) and a working Rust toolchain.

## What it proves

`app/app.py` exercises two paths end-to-end:

### Type round-trip (`Resume.transform()`)

A single instance method call covers every non-trivial type conversion
path in `09d-inbound-serialization.md` /
`09e-outbound-deserialization.md`:

- Pydantic class ↔ `Instance` (inbound encode + outbound decode)
- nested class field (`Resume.contact: PhoneNumber`)
- `Optional[str]` (`email`)
- `List[Class]` (`addresses`)
- `Dict[str, int]` (`scores`)
- `Enum` ↔ `Variant` (`sentiment`)
- sync + async fan-out (`transform` and `transform_async`)

### Modular LLM API (`ExtractResume__build_request(...)`)

`ExtractResume(text: string) -> Resume` is an LLM-backed function with
an OpenAI client (`api_key "sk-test"` — offline). The compiler
auto-synthesizes the `$build_request` companion, codegen exposes it as
`ExtractResume__build_request`, and we call it to confirm:

- the auto-synthesized companion routes through `define_function`,
- the returned `baml.http.Request` decodes into the stdlib Pydantic
  class (cross-namespace outbound `_resolve_type`),
- the rendered prompt embeds the caller's `text` argument, so
  `Jinja` templating + `baml.llm.build_request` ran end-to-end,
- async sibling (`ExtractResume__build_request_async`) round-trips too.

Each `assert` in `app/app.py` pins one row of these two matrices.

## What's deliberately *not* exercised

- **User namespace organization (`ns_lorem/`).** Earlier the demo
  scoped types under `ns_lorem/`. Two issues forced the flatten:
  (a) `sys_ops`'s `IoClassLlmClient::get_constructor` builds the
  resolve-function key as `format!("{}$new", client.name)` and only
  falls back to `user.{name}$new` — so `client<llm> StubClient` inside
  `ns_lorem/functions.baml` registers as `user.lorem.StubClient$new`
  but the runtime only looks up `StubClient$new` /
  `user.StubClient$new` and bails;
  (b) cross-namespace class refs in field types (e.g.
  `contact root.ipsum.PhoneNumber`) emit Python `ipsum.PhoneNumber`
  without the matching `from baml_sdk.ipsum import …`. Both filed as
  follow-ups; the demo keeps everything at root namespace.
- **Pure free-function bindings.** `baml_project::build_symbol_pool`
  filters non-LLM free functions out of the codegen pool
  (`client_codegen.rs:350-356`). The demo therefore exposes its
  expression function as `Resume.transform(self)`, which goes through
  the unfiltered method path.
- **Conditional logic on enum equality.** A `if (self.sentiment ==
  Sentiment.POSITIVE) { … } else { … }` body kept the input variant
  unchanged at runtime, suggesting bex_engine's variant-equality path
  doesn't fire for this expression shape. The demo hardcodes the flip
  instead so we cover the enum *round-trip* path without depending on
  the broken comparison. Filed as a follow-up.
- **Real LLM round-trips.** Stub client + `__build_request` only;
  nothing actually leaves the process. `ExtractResume(...)` itself
  would need a real `OPENAI_API_KEY`.
- **Streaming, handles, unions of mixed scalars/classes.** Out of
  scope for a smoke test.

## Bridge / engine fixes landed alongside the demo

Standing the demo up surfaced two pre-existing bugs that blocked the
class round-trip end-to-end:

1. **Inbound FQN prefix.** `bridge_python`'s
   `_subpath_to_baml_fqn` still emitted the BEP-030 spec prefix
   `root.*` for user types, but phase 12a collapsed engine FQNs onto
   `user.*`. Class lookups in the engine therefore panicked on every
   non-trivial Pydantic model. Fixed in
   `crates/bridge_python/python_src/baml/baml_core/proto.py` to emit
   `user.*` directly so the project-boundary coercion is no longer the
   only safety net.
2. **Class field types lost across namespaces.** In
   `baml_compiler2_emit/src/lib.rs`, class field type expressions were
   lowered with `lower_type_expr(...)` (`ns_context = []`), so
   sibling-namespace class refs (`addresses Address[]` inside a
   `lorem.Resume`) resolved to `Ty::Unknown` → `Ty::Void` and tripped
   the FFI encoder when the class was returned. Switched to
   `lower_type_expr_in_ns(..., &pkg_info.namespace_path, ...)`,
   matching the existing fix on the function-signature path.

## Layout

```
bridge_python_e2e/
├── run.sh                   # build + generate + maturin develop + run
├── baml_src/
│   ├── generators.baml      # output_type "python/pydantic", output_dir "../app"
│   ├── types.baml           # Address, Sentiment, PhoneNumber, Resume{transform}
│   └── functions.baml       # client<llm> StubClient + ExtractResume LLM fn
└── app/
    ├── pyproject.toml       # depends on baml (path ../../../crates/bridge_python)
    ├── app.py               # the user-shaped script
    └── baml_sdk/            # ← written by `baml-cli generate` (gitignored)
```
