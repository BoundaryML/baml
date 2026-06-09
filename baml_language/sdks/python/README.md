# baml_core

Python bindings for the BAML runtime (powered by `bex_engine`).

`baml_core` is the bridge layer that generated `baml_sdk` packages
import at runtime: it provides the `BamlRuntime` singleton, the
protobuf encoder/decoder, the function/method factories, and the
`Collector` / `BamlCtxManager` observability primitives.

```python
from baml_core import BamlRuntime

rt = BamlRuntime.initialize_runtime(
    root_path=".",
    files={"main.baml": baml_source},
    sdk_root="my_sdk",
)
```

This package is generally consumed indirectly via the code generated
by `baml-cli generate --target python` — direct use is reserved for
runtime authors and bridge tests.

## Requirements

- Python 3.10+
- `protobuf >= 6.31.1`

## License

Apache-2.0
