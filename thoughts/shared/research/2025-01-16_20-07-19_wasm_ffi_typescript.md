---
date: 2025-08-16T20:07:19+0000
researcher: dex
git_commit: 4d80a47208a98378037c6abce7efa45e32707841
branch: canary
repository: baml
topic: "WASM Support for BAML - Compiling FFI Runtime to WASM with TypeScript-WASM Layer"
tags: [research, codebase, wasm, ffi, typescript, protobuf, language-client]
status: complete
last_updated: 2025-01-16
last_updated_by: dex
last_updated_note: "Added specific spawn_local locations and clarified crypto is already handled"
---

# Research: WASM Support for BAML - Compiling FFI Runtime to WASM with TypeScript-WASM Layer

**Date**: 2025-08-16T20:07:19+0000
**Researcher**: dex
**Git Commit**: 4d80a47208a98378037c6abce7efa45e32707841
**Branch**: canary
**Repository**: baml

## Research Question
How to make WASM work in BAML by compiling the core BAML FFI runtime into WASM and creating a TypeScript-WASM layer similar to language-client-go that handles encode/decode and protobuf operations to support calling BAML from TypeScript via WASM.

## Important Note
**IGNORE `baml-schema-wasm`**: The existing `engine/baml-schema-wasm/` implementation is deprecated and should NOT be used as a reference for the new WASM FFI implementation. It represents an old approach that is no longer relevant. This research focuses on creating a clean WASM compilation of the FFI runtime from scratch.

## Summary
The existing C FFI (`language_client_cffi`) uses Protocol Buffers for serialization and can be adapted to compile for both native and WASM targets. Since the core BAML runtime already compiles to WASM, the main task is making the CFFI layer itself compile to WASM with appropriate conditional compilation. The Go client provides an excellent architectural blueprint for the TypeScript-WASM layer, with clear patterns for protobuf encode/decode, type mapping, and FFI calling conventions.

## Detailed Findings

### Current Architecture Overview

#### Existing FFI Runtime (`language_client_cffi`)
- **Location**: `engine/language_client_cffi/`
- **Core Functions**:
  - Runtime management: `create_baml_runtime()`, `destroy_baml_runtime()`
  - Function execution: `call_function_from_c()`, `call_function_stream_from_c()`
  - Object lifecycle: `call_object_constructor()`, `call_object_method()`
- **Serialization**: Protocol Buffers via `types/cffi.proto`
- **Memory Management**: Reference counting with `Arc<T>` and raw pointer wrappers
- **Build Target**: C dynamic library (`cdylib`)

### Key Implementation Patterns from Go Client

#### Protobuf Message Structure
```protobuf
// Core value system - cffi.proto
message CFFIValueHolder {
  CFFIFieldTypeHolder type = 1;
  oneof value {
    string string_value = 2;
    int64 int_value = 3;
    double float_value = 4;
    bool bool_value = 5;
    // ... complex types
  }
}
```

#### Encoding Pattern (Go → Protobuf)
```go
// engine/language_client_go/baml_go/serde/encode.go
func encodeValue(value any) (*cffi.CFFIValueHolder, error) {
    // 1. Determine type information
    // 2. Handle custom serializers first
    // 3. Handle primitives via reflection
    // 4. Attach type info to all values
}
```

#### Decoding Pattern (Protobuf → Go)
```go
// engine/language_client_go/baml_go/serde/decode.go
func Decode(holder *cffi.CFFIValueHolder, typeMap TypeMap) reflect.Value {
    // 1. Handle null values
    // 2. Decode primitives
    // 3. Resolve custom types via TypeMap
    // 4. Handle complex types recursively
}
```

### WASM-Specific Challenges and Solutions

#### 1. Async Runtime Adaptation
- **Current State**: Tokio has limited, unstable WASM support (only for `wasm32-wasip1`, not browser `wasm32-unknown-unknown`)
- **Browser Solution**: Use `wasm_bindgen_futures::spawn_local` or alternative async runtimes
- **WASI Solution**: Can use Tokio with `tokio_unstable` flag but with networking limitations
- **Pattern**: Conditional compilation to use appropriate async runtime per target

##### Specific CFFI Code Requiring `spawn_local`
Only **4 locations** in `engine/language_client_cffi/src/ffi/functions.rs` need adaptation:

1. **Global Runtime** (lines 15-16):
```rust
static RUNTIME: Lazy<Arc<tokio::runtime::Runtime>> =
    Lazy::new(|| Arc::new(tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")));
```

2. **Function Call Spawning** (lines 79-80):
```rust
rt.spawn(async move {
    runtime.call_function(/* args */).await
});
```

3. **Parse Function Spawning** (lines 169-170):
```rust
rt.spawn(async move {
    runtime.parse_llm_response(/* args */)
});
```

4. **Stream Function Spawning** (lines 273-284):
```rust
RUNTIME.spawn(async move {
    stream.run(/* callbacks */).await
});
```

The adaptation pattern is straightforward:
```rust
#[cfg(target_arch = "wasm32")]
wasm_bindgen_futures::spawn_local(async move { /* same logic */ });

#[cfg(not(target_arch = "wasm32"))]
rt.spawn(async move { /* existing logic */ });
```

#### 2. Memory Management
- **Challenge**: No shared memory threading in WASM
- **Solution**: Linear memory model with `Uint8Array` for binary transfers
- **Pattern**: Pass data as binary arrays through WASM boundary

#### 3. Crypto Operations
- **No CFFI changes needed**: The BAML runtime already compiles for WASM and handles all crypto operations internally with appropriate `cfg` switching
- **Existing solution**: `engine/baml-runtime/src/internal/wasm_jwt.rs` already implements browser Subtle.Crypto API for WASM targets

#### 4. Build System
- **Challenge**: Feature unification issues between native and WASM
- **Solution**: Separate build targets with conditional compilation
  ```rust
  #[cfg(target_arch = "wasm32")]
  // WASM-specific implementation
  ```

## Code References

### Critical Files for Implementation
- `engine/language_client_cffi/src/lib.rs:8-13` - FFI interface exports (needs WASM conditional exports)
- `engine/language_client_cffi/src/ffi/functions.rs` - **Primary WASM adaptation target** (4 spawn points)
- `engine/language_client_cffi/types/cffi.proto:1-351` - Protobuf schema (unchanged for WASM)
- `engine/language_client_go/baml_go/serde/encode.go:94-209` - Encoding patterns to replicate in TypeScript
- `engine/language_client_go/baml_go/serde/decode.go:454-501` - Decoding patterns to replicate in TypeScript

## Architecture Insights

### Proposed Architecture for TypeScript-WASM Client

#### 1. Unified CFFI Compilation Strategy
Rather than creating a separate crate, modify `language_client_cffi` to support both native and WASM targets:
```rust
// In language_client_cffi/Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]  // Support both C and WASM

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"

// Conditional exports in lib.rs
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn create_baml_runtime(...) { ... }
```

#### 2. TypeScript Interface Layer
```typescript
// Similar to Go's BamlSerializer interface
interface BamlSerializer {
  encode(): CFFIValueHolder;
  bamlTypeName(): string;
  bamlEncodeName(): CFFITypeName;
}

// Type mapping system
interface TypeMap {
  [key: string]: TypeConstructor; // "namespace.name" → constructor
}

// WASM FFI bridge
class WasmBamlRuntime {
  private wasmModule: WebAssembly.Module;

  async callFunction<T>(
    name: string,
    args: Record<string, any>,
    decoder: (holder: CFFIValueHolder) => T
  ): Promise<T> {
    const encoded = encodeArguments(args);
    const response = await this.wasmModule.callFunction(name, encoded);
    return decoder(CFFIValueHolder.decode(response));
  }
}
```

#### 3. Build Pipeline
1. Set up dual-target compilation in `language_client_cffi`
2. Add conditional compilation flags for WASM-specific code paths
3. Use `wasm-pack` to build WASM artifacts alongside native builds
4. Generate TypeScript protobuf types from existing `cffi.proto`
5. Create TypeScript wrapper layer with protobuf serialization (similar to Go client)

## Historical Context

The codebase shows a pragmatic evolution:
- **Native FFI for TypeScript**: Currently used for server-side performance via N-API
- **Protocol Buffers**: Chosen for cross-language compatibility in CFFI
- **Go Client Success**: Demonstrates viability of protobuf-based language clients

Known technical considerations:
- Tokio has limited WASM support (unstable for WASI, unavailable for browsers) - only 4 spawn points need adaptation
- Build complexity increases with multiple compilation targets
- Core runtime already compiles to WASM with crypto handling, so main focus is CFFI layer's async spawning

## Related Research
- Go client architecture: `engine/language_client_go/`
- Existing TypeScript native client: `engine/language_client_typescript/`
- Current CFFI implementation: `engine/language_client_cffi/`

## Design Decisions

Based on research and requirements:
1. **Coexistence**: WASM and native TypeScript clients will coexist for different use cases
2. **Streaming**: Should work transparently through the existing protobuf protocol
3. **Performance**: Protobuf serialization overhead is acceptable for the flexibility gained
4. **SharedArrayBuffer**: Should be supported when available for performance optimization
5. **Module initialization**: To be determined during implementation phase
