# BAML C++ SDK

`baml-cli generate` emits a self-contained C++ source tree (`baml_sdk/`):
the typed API, the embedded BAML bytecode, the vendored bridge runtime
headers, and generated protobuf bindings for the wire schema. There is no
C++ package to install; the bridge `dlopen`s the shared BAML runtime at
first use.

## Usage

```cmake
add_subdirectory(baml_sdk)
target_link_libraries(app PRIVATE baml::sdk)
```

```sh
BAML_RUNTIME_PATH=/path/to/libbridge_cffi.dylib ./app
```

The SDK's wire layer uses protobuf-lite, built from source at a pinned
version by `baml_sdk/cmake/fetch_protobuf.cmake` (FetchContent; first
configure clones protobuf + abseil, first build adds ~30-60s, cached
thereafter). Building from source with your compiler and C++ standard is
deliberate: it removes every protobuf ABI/version-mismatch failure mode,
and the lite runtime has no global descriptor pool, so it cannot collide
with a host application's own protobuf.

Offline / air-gapped builds use standard FetchContent overrides:

```sh
cmake -DFETCHCONTENT_SOURCE_DIR_PROTOBUF=/path/to/protobuf-31.1 \
      -DFETCHCONTENT_SOURCE_DIR_ABSL=/path/to/abseil-cpp ...
```

Requirements: CMake 3.16+, clang/gcc/MSVC with C++17 or later.

## Bound specs and streaming

An authored LLM function exposes its ordinary call plus a bound spec. The
spec owns prompt rendering, request building, parsing, and calls. Streaming
uses the flat host shortcut backed by the compiler-private `Fn@stream` entry:

```cpp
auto spec = baml_sdk::lorem::Extract_spec(input);
auto prompt = spec.prompt();
auto request =
    spec.build_request<baml_sdk::baml::http::Request>();

// This invokes the compiler-private Extract@stream projection.
auto stream = baml_sdk::lorem::Extract_stream(input);
for (;;) {
  auto item = stream.next();
  if (item.done()) break;
  const auto& partial = item.value();
  // Consume the typed partial value.
}
auto result = stream.final_();
```

`FunctionSpec<Final>` carries the bound recipe's output type, while
`Stream<Partial, Final>` retains the PPIR partial and final types. PPIR's
partial-output models are generated under
`baml_sdk::stream_types`; their `$stream` spelling is only a BAML wire type
identity. No callable `$spec`, `$stream`, `$parse`, `$render_prompt`, or
`$build_request` declarations are generated.

The flat C++ stream shortcut has synchronous and asynchronous forms and sends
the authored function FQN with the Stream boundary operation. The C++ surface
currently uses the private stream projection's client and callback defaults;
explicit `client` / `on_event` controls await host representations for
streaming-client interfaces and optional callbacks.

## Runtime resolution

At the first BAML call the bridge locates the shared runtime
(`libbridge_cffi.dylib` / `libbridge_cffi.so` / `bridge_cffi.dll`) in this
order:

1. `baml::SetRuntimePath(path)` (programmatic, before first use)
2. `BAML_RUNTIME_PATH` (compatibility alias: `BAML_LIBRARY_PATH`)
3. Next to the executable (application-bundled deployment)
4. The shared BAML cache:
   `~/.baml/runtimes/prod/<version>/abi-v1/<target>/<filename>`
   (roots overridable via `BAML_RUNTIME_CACHE_DIR` / `BAML_HOME`; the cache
   probe uses `BAML_RUNTIME_VERSION` when set)

The bridge itself **never downloads anything**. A resolution miss throws a
structured `baml::RuntimeError` (stable code, searched paths, remediation).
Provision the runtime with `baml runtime install`, bundle it with your
application, or set an explicit path.

The loader resolves a single symbol (`baml_get_api_v1`), validates the ABI
table, and registers the bridge (language `cpp`, the SDK's canonical BAML
version) before initialization; a version mismatch between the generated
SDK and the loaded runtime fails closed with both versions named.

## Deployment

Ship the runtime library with your application (copy it next to the binary
or set `BAML_RUNTIME_PATH`). In containers, install it at image-build time.
The runtime artifact for each target is published with every BAML release.

## Errors

Runtime-loading failures carry stable codes (`BAML_RUNTIME_NOT_FOUND`,
`BAML_RUNTIME_LOAD_FAILED`, `BAML_RUNTIME_ABI_MISMATCH`,
`BAML_RUNTIME_VERSION_MISMATCH`, `BAML_RUNTIME_CONFIG_CONFLICT`, ...) on
`baml::RuntimeError::code()`. BAML-level failures surface as
`baml::BamlError` / `baml::BamlPanic` / `baml::BamlCancelled` with typed
payload access (`is<T>()` / `get<T>()`).

## Layout

| Path | Purpose |
|---|---|
| `bridge_cpp/include/baml/` | Header-only bridge runtime (vendored into generated SDKs) |
| `sdkgen_cpp/` | The C++ code generator |
| `STYLE.md` | C++ style guide (Google style + documented carve-outs) |

Tests live in `sdk_tests/crates/cpp/` (fixture parity suites) and
`bridge_cpp/tests/` (bridge-core smoke).
