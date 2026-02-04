# Language Client FFI

Internal FFI for the language client.

## Prerequisites

- Cargo Make  
  Install with: `cargo install cargo-make`
- cbindgen  
  Install with: `cargo install cbindgen`

## Build Instructions

1. Build the CFFI Layer for the Go SDK:

    ```bash
    cd engine/language_client_cffi
    # default
    cargo make go-sdk
    # release build
    RELEASE_MODE=1 cargo make go-sdk
    ```

    This command:
    1. Builds the CFFI layer for the Go SDK.
        - Generates the C header file `baml_cffi_generated.h` for go-sdk
    2. Builds `baml-cli`
    3. Runs `baml-cli generate` in integ tests

2. Build integration tests:

    ```bash
    cd integ-tests/go
    go build
    ./integ-tests
    ```

## Local Development with Go Tests

When developing changes to the CFFI layer, Go tests will by default download and use a cached version of the library from GitHub releases. To test your local changes:

1. **Build the CFFI library** (debug mode is faster for iteration):

    ```bash
    cd engine/language_client_cffi
    cargo build  # or cargo build --release for optimized build
    ```

    This creates: `engine/target/debug/libbaml_cffi.dylib` (macOS) or `libbaml_cffi.so` (Linux)

2. **Run Go tests with your local library**:

    ```bash
    cd engine/generators/languages/go/generated_tests/dynamic_types  # or any test directory
    BAML_LIBRARY_PATH=/path/to/baml/engine/target/debug/libbaml_cffi.dylib go test -v
    ```

    The `BAML_LIBRARY_PATH` environment variable tells the Go SDK to use your locally built library instead of the cached version.

3. **Regenerate Go test code** (if you've changed code generation):

    ```bash
    cd engine/generators/languages/go
    cargo test --lib  # Regenerates all Go test projects
    ```

### Quick Development Loop

```bash
# 1. Make changes to Rust code
vim engine/language_client_cffi/src/ffi/functions.rs

# 2. Rebuild CFFI
cd engine/language_client_cffi && cargo build

# 3. Test with your changes
cd ../generators/languages/go/generated_tests/dynamic_types
BAML_LIBRARY_PATH=$PWD/../../../target/debug/libbaml_cffi.dylib go test -v
```

> **Note:** Without setting `BAML_LIBRARY_PATH`, Go tests will use the cached library at `~/.cache/baml/libs/{VERSION}/` or download from GitHub releases, which won't reflect your local changes.

> **Note:** The actual go-sdk lives in [../language_client_go](../language_client_go/pkg/lib.go). The CFFI layer is a thin wrapper around the go-sdk.

## Additional Information

For further details on configuration and advanced usage, please refer to the corresponding cargo-make tasks defined in your project configuration files or the Go project documentation in the integ-tests/go directory.
