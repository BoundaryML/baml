# bridge_python

PyO3 Python bindings for `baml_language`. Wraps `bridge_cffi` → `bex_engine` and exposes a `baml_py` Python module.

## Build & Test

```bash
cd baml_language/crates/bridge_python
uv run maturin develop --uv
uv run pytest tests/ -v
```

## What's implemented

- `BamlRuntime.from_files()` / `call_function()` / `call_function_sync()` — full Python → PyO3 → bridge_cffi → bex_engine pipeline
- `BamlCtxManager` with `@trace_fn` decorator, `upsert_tags`, `flush` (stubs)
- `FunctionResult`, `HostSpanManager` (stub)

## Next steps

- BamlSpan: `start_call` / `finish_call` on BexEngine to record trace events
- Trace file output (JSONL with `function_start` / `function_end` events)
- Tag propagation in HostSpanManager
- `flush()` implementation
- Streaming (`stream_function`)
