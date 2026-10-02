# bridge_python

PyO3 Python bindings for `baml_language`. Wraps `bridge_cffi` → `bex_engine` and exposes the `baml_bridge.baml_py` native module.

## Build & Test

From the repository root:

```bash
cd baml_language/sdks/python
uv run maturin develop --uv
uv run pytest tests/ -v
```

The native module provides runtime initialization, synchronous and asynchronous function calls, cancellation, media and value handles, host callable dispatch, and runtime shutdown. The surrounding `baml_bridge` Python package provides encoding, decoding, generated function factories, and stream wrappers.
