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

A single `Resume.transform()` call exercises every non-trivial type
conversion path in `09d-inbound-serialization.md` /
`09e-outbound-deserialization.md`:

- Pydantic class ↔ `Instance` (inbound encode + outbound decode)
- nested class field (`Resume.contact: PhoneNumber`)
- `Optional[str]` (`email`)
- `List[Class]` (`addresses`)
- `Dict[str, int]` (`scores`)
- `Enum` ↔ `Variant` (`sentiment`)
- sync + async fan-out (`transform` and `transform_async`)

Each `assert` in `app/app.py` pins one row of that matrix.

## What's deliberately *not* exercised

- **Cross-namespace references.** Initial sketch had `Resume.contact:
  root.ipsum.PhoneNumber` (cross-leaf). The codegen emits the type ref
  but doesn't emit the matching Python `from baml_sdk.ipsum import
  PhoneNumber`, leaving the generated module with an undefined name.
  Filed as a follow-up; demo keeps `PhoneNumber` in the same `lorem`
  namespace.
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
- **LLM round-trips.** The demo runs offline; no `OPENAI_API_KEY` is
  needed.
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
│   └── ns_lorem/types.baml  # Address, Sentiment, PhoneNumber, Resume{transform}
└── app/
    ├── pyproject.toml       # depends on baml (path ../../../crates/bridge_python)
    ├── app.py               # the user-shaped script
    └── baml_sdk/            # ← written by `baml-cli generate` (gitignored)
```
