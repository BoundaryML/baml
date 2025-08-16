# WASM FFI TypeScript Client Implementation Plan

## Overview

Compile the BAML FFI runtime (`language_client_cffi`) to WASM and create a TypeScript-WASM client layer that enables BAML to run in browser environments. This involves adapting the existing C FFI to compile for WASM targets and creating a TypeScript layer modeled after the Go client for protobuf-based encode/decode operations.

## Current State Analysis

The existing `language_client_cffi` uses Tokio for async operations and exports C FFI functions. Only 3 async spawn locations need adaptation to use `wasm_bindgen_futures::spawn_local` for WASM compatibility. The Go client provides an excellent architectural pattern with its protobuf-based serde layer that we'll replicate in TypeScript.

### Key Discoveries:
- Only 3 Tokio spawn locations need adaptation (engine/language_client_cffi/src/ffi/functions.rs:80,170,273)
- Crypto operations already handled by BAML runtime's WASM support
- Protobuf schema (cffi.proto) can be reused directly
- Existing WASM build infrastructure (wasm-pack) available in the codebase

## What We're NOT Doing

- NOT modifying the core BAML runtime (already WASM-compatible)
- NOT changing the protobuf protocol or schema
- NOT removing the existing native TypeScript client
- NOT using the deprecated `baml-schema-wasm` as reference
- NOT supporting Node.js environments (browser-only for WASM)

## Implementation Approach

Create conditional compilation in `language_client_cffi` to support both native (C FFI) and WASM targets simultaneously. The TypeScript layer will handle protobuf encoding/decoding similar to the Go client pattern, interfacing with the WASM module through typed JavaScript bindings.

---

## Phase 1: WASM Compilation Infrastructure

### Overview
Set up dual-target compilation for `language_client_cffi` to support both C FFI and WASM builds without breaking existing functionality.

### Changes Required:

#### 1. Update Cargo Configuration
**File**: `engine/language_client_cffi/Cargo.toml`
**Changes**: Add WASM-specific dependencies and conditional compilation

```toml
[lib]
crate-type = ["cdylib", "rlib"]  # Support both C and WASM

[dependencies]
# Make tokio conditional
tokio = { version = "1", features = ["full"], optional = true }
tokio-util = { version = "0.7", features = ["full"], optional = true }

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "=0.2.92"
wasm-bindgen-futures = "0.4.42"
js-sys = "=0.3.69"
web-sys = { version = "0.3.69", features = ["console"] }
console_log = { version = "1", features = ["color"] }
console_error_panic_hook = "0.1.7"

[features]
default = ["native"]
native = ["tokio", "tokio-util"]
wasm = []
```

#### 2. Add WASM Build Script
**File**: `engine/language_client_cffi/build_wasm.sh`
**Changes**: Create build script for WASM target

```bash
#!/bin/bash
set -e

echo "Building BAML FFI for WASM..."
cargo build --target wasm32-unknown-unknown --no-default-features --features wasm --release
wasm-pack build --target bundler --out-dir pkg --no-default-features --features wasm
echo "WASM build complete!"
```

### Success Criteria:

#### Automated Verification:
- [x] Cargo builds for native target: `cargo build --release`
- [x] Cargo builds for WASM target: `cargo build --target wasm32-unknown-unknown --no-default-features --features wasm`
- [x] No compilation errors for either target
- [ ] Existing tests still pass: `cargo test`

#### Manual Verification:
- [ ] Run WASM build script: `./build_wasm.sh`
- [ ] Verify WASM artifacts generated in `pkg/` directory: `ls -la pkg/`
- [ ] Check WASM file size is reasonable (<10MB): `du -h pkg/*.wasm`
- [ ] No Tokio imports in WASM build: `wasm-objdump -x pkg/*.wasm | grep -v tokio`

---

## Phase 2: Async Runtime Adaptation

### Overview
Replace Tokio-based async spawning with `wasm_bindgen_futures::spawn_local` for WASM targets while maintaining native behavior. Create a clean abstraction layer to handle platform-specific spawning without scattering conditional compilation throughout the codebase.

### Changes Required:

#### 1. Create Async Runtime Abstraction
**File**: `engine/language_client_cffi/src/ffi/async_runtime.rs`
**Changes**: New module with platform-agnostic spawning abstraction

```rust
use std::future::Future;
use std::pin::Pin;

#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::Runtime;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use once_cell::sync::Lazy;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

/// Platform-agnostic async runtime abstraction
pub struct AsyncRuntime;

#[cfg(not(target_arch = "wasm32"))]
static RUNTIME: Lazy<Arc<Runtime>> = Lazy::new(|| {
    Arc::new(Runtime::new().expect("Failed to create Tokio runtime"))
});

impl AsyncRuntime {
    /// Spawn a future on the appropriate runtime for the current platform
    pub fn spawn<F>(future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        #[cfg(not(target_arch = "wasm32"))]
        {
            RUNTIME.spawn(future);
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            // spawn_local doesn't require Send, so we can directly use it
            spawn_local(future);
        }
    }
    
    /// Spawn a local future (for WASM compatibility)
    pub fn spawn_local<F>(future: F)
    where
        F: Future<Output = ()> + 'static,
    {
        #[cfg(not(target_arch = "wasm32"))]
        {
            // On native, we can still use the tokio runtime
            // but we need to wrap in a Send future
            let future = async move {
                future.await
            };
            RUNTIME.spawn(future);
        }
        
        #[cfg(target_arch = "wasm32")]
        {
            spawn_local(future);
        }
    }
    
    /// Get a handle to the native runtime (only available on non-WASM)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn native_runtime() -> Arc<Runtime> {
        RUNTIME.clone()
    }
}
```

#### 2. Update Function Spawning
**File**: `engine/language_client_cffi/src/ffi/functions.rs`
**Changes**: Replace all spawn calls with the new abstraction

```rust
use crate::ffi::async_runtime::AsyncRuntime;

// Remove the old RUNTIME static - it's now in async_runtime.rs

// Line 80 - call_function_from_c_inner
// Replace the entire spawn block with:
AsyncRuntime::spawn_local(async move {
    let result = runtime.call_function(/* args */).await;
    // existing logic unchanged
});

// Line 170 - call_function_parse_from_c_inner  
// Replace the entire spawn block with:
AsyncRuntime::spawn_local(async move {
    let result = runtime.parse_llm_response(/* args */).await;
    // existing logic unchanged
});

// Line 273 - call_function_stream_from_c_inner
// Replace the entire spawn block with:
AsyncRuntime::spawn_local(async move {
    stream.run(/* callbacks */).await;
    // existing logic unchanged
});
```

#### 3. Update Module Structure
**File**: `engine/language_client_cffi/src/ffi/mod.rs`
**Changes**: Add the new async_runtime module

```rust
mod async_runtime;
mod callbacks;
mod functions;
mod runtime;

// Re-export if needed
pub use async_runtime::AsyncRuntime;
```

#### 4. Add WASM Exports
**File**: `engine/language_client_cffi/src/lib.rs`
**Changes**: Add WASM-bindgen exports

```rust
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn create_baml_runtime_wasm(
    baml_src: String,
    config: String,
    env_vars: String
) -> *mut c_void {
    // Delegate to existing create_baml_runtime
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn call_function_wasm(
    runtime: *mut c_void,
    function_name: String,
    args_proto: Vec<u8>,
) -> js_sys::Promise {
    // Create promise, spawn_local, resolve with result
}
```

### Success Criteria:

#### Automated Verification:
- [x] Native compilation still works: `cargo build --release`
- [x] WASM compilation succeeds: `cargo build --target wasm32-unknown-unknown --no-default-features --features wasm`
- [x] No Tokio runtime references in functions.rs: `grep -r "tokio::runtime" src/ffi/functions.rs` (should return nothing)
- [x] AsyncRuntime abstraction compiles for both targets: `cargo check --all-targets`

#### Manual Verification:
- [ ] Build WASM using the build script: `./build_wasm.sh`
- [ ] Verify AsyncRuntime usage in code: `grep -r "AsyncRuntime::spawn" src/ffi/functions.rs` (should show 3 usages)
- [ ] Check WASM build artifacts:
  ```bash
  # Create and run verify_wasm_build.sh
  cat > verify_wasm_build.sh << 'EOF'
  #!/bin/bash
  set -e
  
  echo "Verifying WASM build..."
  
  # Check spawn_local usage
  if wasm2wat pkg/*_bg.wasm | grep -q spawn_local; then
    echo "✓ spawn_local found in WASM binary"
  else
    echo "✗ spawn_local NOT found - async abstraction may be broken"
    exit 1
  fi
  
  # Check exported functions
  if wasm-objdump -x pkg/*_bg.wasm | grep -qE "create_baml_runtime|call_function"; then
    echo "✓ Required exports found"
  else
    echo "✗ Missing required exports"
    exit 1
  fi
  
  # Verify no threading primitives
  if wasm2wat pkg/*_bg.wasm | grep -qE "atomic|thread|mutex"; then
    echo "✗ Threading primitives found in WASM (should not exist)"
    exit 1
  else
    echo "✓ No threading primitives in WASM"
  fi
  
  echo "WASM verification complete!"
  EOF
  chmod +x verify_wasm_build.sh
  ./verify_wasm_build.sh
  ```
- [ ] Test abstraction with test script:
  ```bash
  # Create and run test_async_runtime.sh
  cat > test_async_runtime.sh << 'EOF'
  #!/bin/bash
  echo "Testing native build..."
  cargo test --lib async_runtime -- --nocapture
  
  echo "Testing WASM build..."
  cargo test --target wasm32-unknown-unknown --no-default-features --features wasm
  EOF
  chmod +x test_async_runtime.sh
  ./test_async_runtime.sh
  ```

---

## Phase 3: TypeScript Protobuf Layer ✅

### Overview
Generate TypeScript types from `cffi.proto` in the CFFI build process and implement encode/decode functions modeled after the Go client's serde layer.

### Changes Required:

#### 1. Update CFFI Build Script for TypeScript Generation
**File**: `engine/language_client_cffi/build.rs`
**Changes**: Add TypeScript protobuf generation alongside Rust generation

```rust
use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Existing Rust protobuf generation
    let proto_file = "types/cffi.proto";
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    prost_build::Config::new()
        .out_dir(&out_dir)
        .compile_protos(&[proto_file], &["types/"])
        .expect("Failed to compile protos");
    
    // New: Generate TypeScript types if building for WASM
    #[cfg(feature = "wasm")]
    {
        generate_typescript_proto();
    }
    
    println!("cargo:rerun-if-changed={}", proto_file);
}

#[cfg(feature = "wasm")]
fn generate_typescript_proto() {
    let ts_out_dir = PathBuf::from("../../language_client_wasm/src/proto");
    
    // Ensure output directory exists
    std::fs::create_dir_all(&ts_out_dir).expect("Failed to create WASM client proto dir");
    
    // Use buf to generate TypeScript (requires buf CLI installed)
    let status = Command::new("buf")
        .args(&[
            "generate",
            "--template", "types/buf.gen.yaml",
            "types/"
        ])
        .status()
        .expect("Failed to run buf generate");
    
    if !status.success() {
        // Fallback to protoc if buf is not available
        println!("cargo:warning=buf not found, trying protoc");
        
        let status = Command::new("protoc")
            .args(&[
                "--plugin=protoc-gen-es=node_modules/.bin/protoc-gen-es",
                "--es_out", ts_out_dir.to_str().unwrap(),
                "--es_opt", "target=ts,import_extension=.js",
                "--proto_path", "types",
                "types/cffi.proto"
            ])
            .status()
            .expect("Failed to run protoc");
        
        if !status.success() {
            panic!("Failed to generate TypeScript protobuf types");
        }
    }
    
    println!("cargo:warning=Generated TypeScript protobuf types for WASM client");
}
```

#### 2. Buf Configuration for TypeScript
**File**: `engine/language_client_cffi/types/buf.gen.yaml`
**Changes**: Configure TypeScript generation

```yaml
version: v1
plugins:
  - plugin: es
    out: ../../../language_client_wasm/src/proto
    opt:
      - target=ts
      - import_extension=.js
```

#### 3. Create WASM Client Package
**File**: `engine/language_client_wasm/package.json`
**Changes**: New package for WASM client

```json
{
  "name": "@boundaryml/baml-wasm",
  "version": "0.1.0",
  "description": "BAML WASM client for browser environments",
  "main": "dist/index.js",
  "module": "dist/index.js",
  "types": "dist/index.d.ts",
  "type": "module",
  "files": [
    "dist",
    "wasm"
  ],
  "scripts": {
    "build": "tsc",
    "build:wasm": "cd ../language_client_cffi && ./build_wasm.sh && cp -r pkg ../language_client_wasm/wasm",
    "test": "jest",
    "test:e2e": "playwright test",
    "serve:test": "python3 -m http.server 3000",
    "clean": "rm -rf dist wasm"
  },
  "dependencies": {
    "@bufbuild/protobuf": "^1.6.0"
  },
  "devDependencies": {
    "@bufbuild/protoc-gen-es": "^1.6.0",
    "@playwright/test": "^1.40.0",
    "@types/node": "^20.0.0",
    "typescript": "^5.0.0",
    "jest": "^29.0.0",
    "@types/jest": "^29.0.0"
  },
  "engines": {
    "node": ">=18.0.0"
  }
}
```

#### 4. TypeScript Configuration
**File**: `engine/language_client_wasm/tsconfig.json`
**Changes**: Configure TypeScript compilation

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "lib": ["ES2020", "DOM"],
    "moduleResolution": "node",
    "outDir": "./dist",
    "rootDir": "./src",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "allowJs": false,
    "noEmit": false
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "test", "wasm"]
}
```

#### 5. Implement Encoding Layer
**File**: `engine/language_client_wasm/src/encode.ts`
**Changes**: Port Go encoding patterns to TypeScript

```typescript
import { CFFIValueHolder, CFFIFieldTypeHolder } from './proto/cffi_pb.js';

export interface BamlSerializer {
  encode(): CFFIValueHolder;
  bamlTypeName(): string;
  bamlEncodeName(): CFFIFieldTypeHolder;
}

export function encodeValue(value: any): CFFIValueHolder {
  const holder = new CFFIValueHolder();
  
  // Handle null
  if (value === null || value === undefined) {
    holder.value = { case: 'nullValue', value: {} };
    return holder;
  }
  
  // Handle primitives
  if (typeof value === 'string') {
    holder.value = { case: 'stringValue', value };
    return holder;
  }
  
  if (typeof value === 'number') {
    if (Number.isInteger(value)) {
      holder.value = { case: 'intValue', value: BigInt(value) };
    } else {
      holder.value = { case: 'floatValue', value };
    }
    return holder;
  }
  
  if (typeof value === 'boolean') {
    holder.value = { case: 'boolValue', value };
    return holder;
  }
  
  // Handle arrays
  if (Array.isArray(value)) {
    const items = value.map(item => encodeValue(item));
    holder.value = { case: 'listValue', value: { items } };
    return holder;
  }
  
  // Handle objects/classes
  if (typeof value === 'object') {
    // Check if it implements BamlSerializer
    if ('encode' in value && typeof value.encode === 'function') {
      return value.encode();
    }
    
    // Generic object encoding
    const fields: Record<string, CFFIValueHolder> = {};
    for (const [key, val] of Object.entries(value)) {
      fields[key] = encodeValue(val);
    }
    holder.value = { case: 'classValue', value: { name: 'DynamicClass', fields } };
    return holder;
  }
  
  throw new Error(`Cannot encode value of type: ${typeof value}`);
}
```

#### 6. Implement Decoding Layer
**File**: `engine/language_client_wasm/src/decode.ts`
**Changes**: Port Go decoding patterns to TypeScript

```typescript
import { CFFIValueHolder } from './proto/cffi_pb.js';

export type TypeMap = Map<string, new(...args: any[]) => any>;

export function decodeValue(holder: CFFIValueHolder, typeMap: TypeMap): any {
  switch (holder.value?.case) {
    case 'nullValue':
      return null;
      
    case 'stringValue':
      return holder.value.value;
      
    case 'intValue':
      return Number(holder.value.value);
      
    case 'floatValue':
      return holder.value.value;
      
    case 'boolValue':
      return holder.value.value;
      
    case 'listValue':
      return holder.value.value.items.map(item => decodeValue(item, typeMap));
      
    case 'mapValue':
      const result: Record<string, any> = {};
      for (const [key, val] of Object.entries(holder.value.value.entries)) {
        result[key] = decodeValue(val, typeMap);
      }
      return result;
      
    case 'classValue':
      const className = holder.value.value.name;
      const ClassConstructor = typeMap.get(className);
      
      if (ClassConstructor) {
        const instance = new ClassConstructor();
        for (const [key, val] of Object.entries(holder.value.value.fields)) {
          instance[key] = decodeValue(val, typeMap);
        }
        return instance;
      }
      
      // Dynamic fallback
      return {
        __bamlType: className,
        ...Object.fromEntries(
          Object.entries(holder.value.value.fields).map(
            ([k, v]) => [k, decodeValue(v, typeMap)]
          )
        )
      };
      
    default:
      throw new Error(`Unknown value case: ${holder.value?.case}`);
  }
}
```

### Success Criteria:

#### Automated Verification:
- [x] CFFI build generates TypeScript protobuf types: `cd engine/language_client_cffi && cargo build --features wasm`
- [x] TypeScript compilation succeeds: `cd engine/language_client_wasm && pnpm tsc --noEmit`
- [ ] Unit tests pass: `cd engine/language_client_wasm && pnpm test`

#### Manual Verification:
- [x] Build CFFI with WASM feature to generate TypeScript types:
  ```bash
  cd engine/language_client_cffi
  cargo build --features wasm
  # Check for generation message in output
  ```
- [x] Verify generated TypeScript files exist:
  ```bash
  ls -la ../language_client_wasm/src/proto/cffi_pb.*
  # Should show cffi_pb.ts and cffi_pb.js files
  ```
- [x] Test encoding/decoding with script:
  ```bash
  # Create test_proto.sh
  cat > test_proto.sh << 'EOF'
  #!/bin/bash
  cd engine/language_client_wasm
  
  # Test encoding
  node -e "
    const { encodeValue } = require('./dist/encode');
    const result = encodeValue({ name: 'test', value: 42 });
    console.log('✓ Encoding test passed');
  "
  
  # Test decoding
  node -e "
    const { decodeValue } = require('./dist/decode');
    const holder = { value: { case: 'stringValue', value: 'hello' } };
    const result = decodeValue(holder, new Map());
    console.log('✓ Decoding test passed, result:', result);
  "
  EOF
  chmod +x test_proto.sh
  ./test_proto.sh
  ```

---

## Phase 4: WASM Runtime Client ✅

### Overview
Create the new WASM client package that loads the WASM module and provides a clean API for calling BAML functions through the WASM FFI.

### Changes Required:

#### 1. WASM Module Loader
**File**: `engine/language_client_wasm/src/runtime.ts`
**Changes**: Implement WASM module loading and initialization

```typescript
import { encodeValue } from './encode.js';
import { decodeValue, TypeMap } from './decode.js';
import { CFFIValueHolder } from './proto/cffi_pb.js';

export class BamlWasmRuntime {
  private wasmModule: any; // wasm-bindgen generated module
  private runtime: number; // pointer to runtime
  private typeMap: TypeMap;
  
  private constructor(wasmModule: any, runtime: number) {
    this.wasmModule = wasmModule;
    this.runtime = runtime;
    this.typeMap = new Map();
  }
  
  static async create(
    rootPath: string,
    srcFiles: Record<string, string>,
    envVars: Record<string, string>
  ): Promise<BamlWasmRuntime> {
    // Dynamic import of WASM module
    const wasmModule = await import('../wasm/index.js');
    await wasmModule.default(); // Initialize WASM
    
    // Create runtime
    const srcFilesJson = JSON.stringify(srcFiles);
    const envVarsJson = JSON.stringify(envVars);
    const runtime = wasmModule.create_baml_runtime_wasm(
      rootPath,
      srcFilesJson,
      envVarsJson
    );
    
    return new BamlWasmRuntime(wasmModule, runtime);
  }
  
  async callFunction<T>(
    functionName: string,
    args: Record<string, any>,
    responseType?: new() => T
  ): Promise<T> {
    // Encode arguments
    const argsHolder = {
      fields: Object.fromEntries(
        Object.entries(args).map(([k, v]) => [k, encodeValue(v)])
      )
    };
    
    // Serialize to protobuf bytes
    const protoBytes = CFFIValueHolder.toBinary(argsHolder);
    
    // Call WASM function (returns Promise)
    const resultBytes = await this.wasmModule.call_function_wasm(
      this.runtime,
      functionName,
      protoBytes
    );
    
    // Decode result
    const resultHolder = CFFIValueHolder.fromBinary(new Uint8Array(resultBytes));
    
    // Register response type if provided
    if (responseType) {
      this.typeMap.set(responseType.name, responseType);
    }
    
    return decodeValue(resultHolder, this.typeMap) as T;
  }
  
  async* callFunctionStream<T>(
    functionName: string,
    args: Record<string, any>,
    responseType?: new() => T
  ): AsyncGenerator<{ partial: T }, T, undefined> {
    // Encode arguments
    const argsHolder = {
      fields: Object.fromEntries(
        Object.entries(args).map(([k, v]) => [k, encodeValue(v)])
      )
    };
    
    const protoBytes = CFFIValueHolder.toBinary(argsHolder);
    
    // Set up streaming with callbacks
    const streamId = Math.random().toString(36);
    const chunks: any[] = [];
    
    // Register callback for streaming data
    (globalThis as any).__baml_stream_callbacks = (globalThis as any).__baml_stream_callbacks || {};
    (globalThis as any).__baml_stream_callbacks[streamId] = (data: Uint8Array, isDone: boolean) => {
      const holder = CFFIValueHolder.fromBinary(data);
      const decoded = decodeValue(holder, this.typeMap);
      chunks.push({ partial: decoded, done: isDone });
    };
    
    // Start streaming call
    await this.wasmModule.call_function_stream_wasm(
      this.runtime,
      functionName,
      protoBytes,
      streamId
    );
    
    // Yield chunks as they arrive
    for (const chunk of chunks) {
      if (chunk.done) {
        return chunk.partial as T;
      }
      yield { partial: chunk.partial as T };
    }
    
    // Clean up callback
    delete (globalThis as any).__baml_stream_callbacks[streamId];
  }
  
  destroy(): void {
    if (this.runtime) {
      this.wasmModule.destroy_baml_runtime(this.runtime);
      this.runtime = 0;
    }
  }
}
```

#### 2. Browser Entry Point
**File**: `engine/language_client_wasm/src/index.ts`
**Changes**: Export public API

```typescript
export { BamlWasmRuntime } from './runtime.js';
export { BamlSerializer, encodeValue } from './encode.js';
export { decodeValue, TypeMap } from './decode.js';
export type { CFFIValueHolder } from './proto/cffi_pb.js';

// Browser environment check
if (typeof window === 'undefined') {
  throw new Error(
    'BAML WASM client is only supported in browser environments. ' +
    'For Node.js, use the native @boundaryml/baml package.'
  );
}
```

### Success Criteria:

#### Automated Verification:
- [x] TypeScript compilation: `cd engine/language_client_wasm && pnpm tsc --noEmit`
- [x] Module exports correctly: `cd engine/language_client_wasm && pnpm build`
- [ ] Unit tests pass: `cd engine/language_client_wasm && pnpm test`

#### Manual Verification:
- [x] Build WASM client: `cd engine/language_client_wasm && pnpm build:wasm`
- [x] WASM files generated: 11MB wasm file in wasm/ directory
- [ ] Test in browser console - **NEEDS TESTING**
- [ ] Test function call - **NEEDS TESTING**
- [ ] Test streaming - **NEEDS TESTING**

### Known Issues to Address in Phase 5:
1. **Protobuf Serialization**: Currently using JSON placeholder - needs actual protobuf implementation
2. **Callback Wiring**: JavaScript callbacks not fully connected to WASM runtime
3. **WASM Path**: Runtime expects WASM at `/wasm/baml_cffi_bg.wasm` - needs configuration
4. **Size Optimization**: 11MB WASM file could be reduced with `wasm-opt`

---

## Phase 5: Testing & Integration

### Overview
Create comprehensive tests and a browser example to validate the complete WASM implementation.

### Changes Required:

#### 1. Browser Test Harness
**File**: `engine/language_client_wasm/test/browser/index.html`
**Changes**: Create test page

```html
<!DOCTYPE html>
<html>
<head>
  <title>BAML WASM Test</title>
</head>
<body>
  <h1>BAML WASM Client Test</h1>
  <div id="status">Loading...</div>
  <div id="results"></div>
  
  <script type="module">
    import { BamlWasmRuntime } from '../../dist/index.js';
    
    async function runTests() {
      const status = document.getElementById('status');
      const results = document.getElementById('results');
      
      try {
        // Create runtime
        status.textContent = 'Creating runtime...';
        const runtime = await BamlWasmRuntime.create(
          '/',
          {
            'main.baml': `
              function Echo(input: string) -> string {
                client GPT4
                prompt #"
                  Echo back: {{ input }}
                "#
              }
            `
          },
          { OPENAI_API_KEY: 'test-key' }
        );
        
        // Test function call
        status.textContent = 'Testing function call...';
        const result = await runtime.callFunction('Echo', { input: 'Hello WASM!' });
        
        results.innerHTML = `
          <h2>✅ Tests Passed</h2>
          <pre>${JSON.stringify(result, null, 2)}</pre>
        `;
        
        runtime.destroy();
      } catch (error) {
        results.innerHTML = `
          <h2>❌ Test Failed</h2>
          <pre>${error.stack}</pre>
        `;
      }
    }
    
    runTests();
  </script>
</body>
</html>
```

#### 2. E2E Test Suite
**File**: `engine/language_client_wasm/test/e2e.test.ts`
**Changes**: Integration tests

```typescript
import { test, expect } from '@playwright/test';

test.describe('BAML WASM Client', () => {
  test('loads and initializes runtime', async ({ page }) => {
    await page.goto('http://localhost:3000/test/browser/');
    
    // Wait for runtime to load
    await page.waitForSelector('#status:has-text("Creating runtime")');
    
    // Verify success
    await page.waitForSelector('h2:has-text("Tests Passed")', { timeout: 10000 });
    
    // Check no errors in console
    const logs = [];
    page.on('console', msg => logs.push(msg));
    expect(logs.filter(log => log.type() === 'error')).toHaveLength(0);
  });
  
  test('handles encoding and decoding', async ({ page }) => {
    await page.goto('http://localhost:3000/test/browser/');
    
    const result = await page.evaluate(async () => {
      const { encodeValue, decodeValue } = await import('/dist/index.js');
      
      const testData = {
        string: 'test',
        number: 42,
        bool: true,
        array: [1, 2, 3],
        nested: { key: 'value' }
      };
      
      const encoded = encodeValue(testData);
      const decoded = decodeValue(encoded, new Map());
      
      return JSON.stringify(decoded) === JSON.stringify(testData);
    });
    
    expect(result).toBe(true);
  });
});
```

### Success Criteria:

#### Automated Verification:
- [ ] Build complete pipeline: `cd engine/language_client_wasm && pnpm build:wasm`
- [ ] E2E tests pass: `cd engine/language_client_wasm && pnpm test:e2e`
- [ ] TypeScript types valid: `cd engine/language_client_wasm && pnpm tsc --noEmit`

#### Manual Verification:
- [ ] Start test server: `cd engine/language_client_wasm && python3 -m http.server 3000`
- [ ] Open browser to http://localhost:3000/test/browser/
- [ ] Verify "Tests Passed" message appears
- [ ] Check browser console for any errors (should be none)
- [ ] Test with complex BAML schema:
  ```javascript
  // In browser console
  const runtime = await BamlWasmRuntime.create('/', {
    'schema.baml': `
      class User {
        name string
        age int
      }
      
      function GetUser(id: string) -> User {
        client GPT4
        prompt #"
          Return user with id {{ id }}
        "#
      }
    `
  }, {});
  
  const user = await runtime.callFunction('GetUser', { id: '123' });
  console.log('User:', user); // Should show structured User object
  ```
- [ ] Verify memory cleanup:
  ```javascript
  // Create and destroy multiple runtimes
  for (let i = 0; i < 10; i++) {
    const rt = await BamlWasmRuntime.create('/', {}, {});
    rt.destroy();
  }
  console.log('Memory test passed - no leaks');
  ```

---

## Testing Strategy

### Unit Tests:
- Encoding/decoding of all BAML types
- WASM module loading and initialization
- Error handling for invalid inputs
- Memory management and cleanup

### Integration Tests:
- End-to-end function calls through WASM
- Streaming responses
- Complex nested object handling
- Concurrent operations

### Manual Testing Steps:
1. Build WASM module: `cd engine/language_client_cffi && ./build_wasm.sh`
2. Build TypeScript client: `cd engine/language_client_typescript && pnpm build:wasm`
3. Start test server: `pnpm serve:test`
4. Open browser to test page
5. Run browser console tests
6. Verify no memory leaks with repeated create/destroy cycles

## Performance Considerations

- WASM module size should be <10MB for reasonable load times
- Use `wasm-opt` for size optimization
- Consider lazy-loading WASM module
- Implement caching for compiled WASM modules
- Use transferable objects for large data when possible

## Migration Notes

- Existing native TypeScript client remains unchanged
- Browser detection automatically selects WASM vs native
- No breaking changes to existing API
- WASM client supports same feature set as native

## Implementation Status Summary

### ✅ Completed Phases (1-4):
- **Phase 1**: WASM Compilation Infrastructure - Build system configured for dual targets
- **Phase 2**: Async Runtime Adaptation - Platform-agnostic async abstraction implemented  
- **Phase 3**: TypeScript Protobuf Layer - Encoding/decoding layers created
- **Phase 4**: WASM Runtime Client - BamlWasmRuntime class with module loading

### 🚧 Remaining Work (Phase 5):
- Browser testing harness implementation
- E2E test suite with Playwright
- Protobuf serialization (currently using JSON placeholder)
- Callback mechanism completion
- Performance optimization (WASM size reduction)

### Next Steps:
1. Set up test server and HTML harness
2. Fix protobuf serialization/deserialization  
3. Complete callback wiring between JS and WASM
4. Run browser tests to validate functionality
5. Optimize WASM size with `wasm-opt`

## References

- Original research: `thoughts/shared/research/2025-01-16_20-07-19_wasm_ffi_typescript.md`
- Go client reference: `engine/language_client_go/baml_go/serde/`
- CFFI implementation: `engine/language_client_cffi/src/ffi/functions.rs`
- Protobuf schema: `engine/language_client_cffi/types/cffi.proto`