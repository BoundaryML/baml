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

> **Note:** The actual go-sdk lives in [../language_client_go](../language_client_go/pkg/lib.go). The CFFI layer is a thin wrapper around the go-sdk.

## Additional Information

For further details on configuration and advanced usage, please refer to the corresponding cargo-make tasks defined in your project configuration files or the Go project documentation in the integ-tests/go directory.
