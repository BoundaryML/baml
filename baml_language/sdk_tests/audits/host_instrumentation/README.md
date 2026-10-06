# Local host instrumentation audit

This audit runs generated SDK functions through the real Python and Node
bridges, awaits runtime shutdown, then asserts exact `baml query` results in
both `spans` and `span_announcements`. Each language gets a disposable home,
configuration directory and recordings. `BOUNDARY_API_KEY=local` disables
cloud selection; every BAML function used here is deterministic and local.

## Reproduce

Run from `baml_language` in an isolated checkout. Generate and build all parts
from the same revision. The normal SDK setup scripts also perform these steps;
the commands below install only the tools and fixture this audit needs.

```sh
cargo build -p baml_cli -p bridge_python -p bridge_typescript -p sdk_test_codegen
target/debug/sdk_test_codegen python_pydantic2 typescript

cd sdks/python
uv sync --group dev --no-install-project
VIRTUAL_ENV="$PWD/.venv" .venv/bin/maturin develop
cd ../typescript/bridge_typescript
pnpm install --ignore-workspace --ignore-scripts
pnpm build:debug
cd ../../../sdk_tests/crates/typescript/function_calls/generated
pnpm install --force --ignore-workspace --ignore-scripts
pnpm update @boundaryml/baml-bridge --force --ignore-workspace --ignore-scripts
cd ../../../../..

sdks/python/.venv/bin/python -m pytest -o addopts= \
  sdk_tests/audits/host_instrumentation/test_recordings.py \
  sdk_tests/audits/host_instrumentation/test_common_types.py \
  --basetemp target/host-instrumentation-evidence
cargo test -p baml_query_btel --lib --test engine --test context --test host_inputs
```

The Node case is copied into the generated SDK by `sdk_test_codegen`. After
editing that case, rerun codegen or copy
`sdk_tests/crates/typescript/function_calls/customizable/host_recording_audit.test.ts`
to the generated fixture's `node/` directory. Vitest terminates workers without
the ordinary Node `beforeExit` event, so the independent recording audit awaits
the existing private native shutdown API. Normal SDK test runs do not do this.

To compare a preserved baseline CLI with the fixed CLI, set `BAML_AUDIT_CLI`
to its absolute path and run the same pytest command with a distinct
`--basetemp`. The directory contains execution stdout/stderr and each SQL
query's full JSON result, diagnostics and recorded extents. A failing query
keeps its evidence; the test requires sealed recordings and complete outcomes.

## Canary audit result

Baseline: `c3a349b05193ac07cecee0d19239a191026929fe`.

| Case | Observed behavior |
| --- | --- |
| Python named input capture | Captured map was unreadable in SQL: `NULL`, `capture_root_mismatch`, exit 1. After the two catalog fixes it renders `{"value":7,"flag":true,"_baml":"application-data"}`; `input_args['value']` is 7, state `present`, exit 0. |
| Node positional input capture | Captured list had the same decoding failure. After the fix it renders `[7,true]`; `input_args[0]` is 7, state `present`, exit 0. |
| BAML argument slots | Existing name and positional navigation still work; the ordinary query regression explicitly verifies `{"x":7}`, `input_args['x']` and `input_args[0]`. |
| Custom `instrument` names | Both bridges retain custom `display_name`. Public SQL `span_name` intentionally exposes FQN, documented in the catalog. Display-label querying is a feature gap, outside this fix. |
| Sync and async generators | Both reject native generator functions explicitly. The host tracing spec defers generator lifetime support. Ordinary functions returning iterators preserve their iterators and record an opaque output. |
| Methods and receivers | Decorated Python instance methods, classmethods and staticmethods work. Python rejects decoration of an already bound method through either decorator form; ordinary functions/coroutines are the documented supported shapes. Node preserves `this` and accepts a bound ordinary method. |
| Per-call context and marker reuse | Concurrent callback calls retain separate distinct IDs and metadata. Per-call overwrite/removal and inherited keys work. A later direct call uses the marker defaults. Each callback has one adopted host span, under the callback future and BAML caller. |
| Explicit error capture | Python preserves exception identity and captures `ValueError` type/args. Node preserves thrown-object identity and captures a plain object's fields. Native Node `Error` objects are opaque host values under the conservative capture policy; the error outcome is still `user_error`. |

The final bridge audit has six tests: baseline **four fail, two pass**;
fixed **six pass**. The existing Python host suite has **26 passes**, and the
Node host suite plus the added execution case has **21 passes**. Query library,
context, engine and host-input regressions have **46 passes**. Re-querying the
exact baseline recordings with the fixed CLI succeeds in both views without
changing their bytes or regenerating them. No streaming source or fixture is
part of this audit.

The fix changes only how the two public SQL views choose a snapshot root for
`input_args`: host definitions (`definition_key` beginning `host:`) use a value
root; BAML functions retain their `FunctionArgs` root. No bridge behavior,
recording schema or capture format changes.

## Common host capture

The follow-up common-type audit records generated BAML classes and generic
classes, Python generated enums, ordinary Pydantic models, Python `date` and
`datetime`, Node `Date`, and passed, returned, and raised exceptions. SQL
equality checks compare host classes, generic arguments, Python enums, and
callback inputs/outputs with their native BAML counterparts. Nominal captures
resolve names against the engine's actual declaration tags; the existing
snapshot transport already represents classes and enums.

Python Pydantic defaults copy stored declared fields using serialization aliases,
without invoking `model_dump`, computed properties, or field serializers. Dates
use ISO strings; Python keeps timezone offsets and naive datetimes have no
offset. A custom Python timezone stays opaque unless registered. Python
exceptions expose type and args; Node errors expose type, message, and an
optional cause. Node enum members are ordinary strings and remain strings.

For other application types, register a synchronous projection:

```python
@trace.capture_for(MyType)
def capture_my_type(value):
    return {"id": value.id}

# Equivalent: trace.register_capture(MyType, capture_my_type)
```

```typescript
trace.registerCapture(MyType, value => ({ id: value.id }));
// Equivalent: trace.captureFor(MyType)(value => ({ id: value.id }));
```

Registrations apply to subclasses, prefer the nearest registered base, and
replace an earlier registration for the same type. Generated BAML types and
builtin scalars/containers take precedence; registrations can customize the
default Pydantic, date, and exception adapters. Handler results recursively
use the same capture policy. These projections affect explicitly requested
trace captures and do not replace function arguments, returned values, or
escaping exceptions. Capture remains bounded to depth 8, 512 values, and 64 KiB
of text; cycles and unsupported objects stay opaque, and budget exhaustion
produces a truncation marker. Projection failures produce an opaque observation
and a bounded diagnostic. The audit includes hostile serializer/getter hooks,
custom handler failures and cycles, native/builtin precedence, asynchronous
functions, and value/byte budgets.

The Node common-type execution case is
`sdk_tests/crates/typescript/function_calls/customizable/host_common_types.test.ts`;
copy it to the generated fixture's `node/` directory when editing without
rerunning codegen. This follows the same isolated recording and shutdown
workflow as the original catalog audit above.
