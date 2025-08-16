# Research Documents

## Architecture & Implementation Research

### [Abort Handlers Architecture](./research/2025-01-16_13-11-09_abort_handlers_architecture.md)
**Date**: 2025-01-16  
**Status**: Complete  
**Tags**: `research`, `codebase`, `abort-handlers`, `streaming`, `runtime`, `typescript`, `python`, `go`

Research on adding AbortController support to the BAML runtime across all language clients. Covers orchestration-level integration points, cross-language FFI bridges, and unified Rust-based cancellation token implementation.

Key topics:
- Orchestrator-level abort integration strategy
- Cross-language client architecture (TypeScript, Python, Go)
- Streaming cancellation patterns
- Rust AbortController design for unified behavior

**Implementation Plan**: [Abort Handlers Implementation](./plans/abort_handlers_implementation.md)

### [WASM FFI TypeScript Support](./research/2025-01-16_20-07-19_wasm_ffi_typescript.md)
**Date**: 2025-01-16  
**Status**: Complete  
**Tags**: `research`, `codebase`, `wasm`, `ffi`, `typescript`, `protobuf`, `language-client`

Research on compiling the BAML FFI runtime to WASM with a TypeScript-WASM layer for browser support. Analyzes current CFFI architecture and proposes unified compilation strategy.

Key topics:
- Unified CFFI compilation for native and WASM targets
- TypeScript interface layer design
- Protobuf serialization patterns from Go client
- WASM-specific challenges (async runtime, memory management, crypto)

**Implementation Plan**: [WASM FFI TypeScript Client Implementation](./plans/wasm_ffi_typescript_implementation.md)