# bridge_python

PyO3 bindings behind the `baml_bridge.baml_py` extension module. The extension shares the process-global `bridge_cffi` runtime and exposes initialization, sync/async calls, cancellation, host-callable dispatch, media/handle wrappers, collectors, and host-span primitives to the Python package in `sdks/python/src/baml_bridge`.

## Build & Test

```bash
cd baml_language/sdks/python
uv run maturin develop --uv
uv run pytest tests/ -v
```

## What's implemented

- `BamlRuntime.initialize_runtime()` / `initialize_runtime_from_bytecode()` and sync/async function calls
- `BamlCallContext`, call IDs, and cancellation
- Host-callable registration, dispatch, typed errors, and exception rehydration
- `BamlPyHandle` plus image/audio/video/PDF wrappers
- `Collector`, `FunctionLog`, `FunctionResult`, and `HostSpanManager`

Higher-level factories, protobuf value conversion, `BamlStream`, tracing decorators, and the process-global runtime accessor live in the surrounding Python package.
