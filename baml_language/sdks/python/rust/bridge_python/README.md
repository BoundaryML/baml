# bridge_python

PyO3 Python bindings for `baml_language`. Wraps `bridge_cffi` → `bex_engine` and exposes the `baml_bridge.baml_py` native module.

## Build & Test

From the repository root:

The tests of the bridge are part of the Python SDK tests
(`sdk_tests/crates/python_pydantic2/function_calls/customizable/bridge_tests`).
The setup script of that suite builds this module:

```bash
cd baml_language
cargo nextest run -p sdk_test_python_pydantic2 --all-features
```

The native module provides runtime initialization, synchronous and asynchronous function calls, cancellation, media and value handles, host callable dispatch, and runtime shutdown. The surrounding `baml_bridge` Python package provides encoding, decoding, generated function factories, and stream wrappers.
