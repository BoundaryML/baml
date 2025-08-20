# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

The baml-compiler is a Rust crate that compiles BAML (Basically a Made-up Language) source code into bytecode for execution by the BAML VM. It's part of a larger monorepo that includes the BAML runtime, language clients, and tooling.

## Common Development Commands

### Building and Testing the Compiler

```bash
# Build the compiler
cargo build

# Run compiler tests
cargo test

# Format code
cargo fmt

# Run linter
cargo clippy

# Build with watch mode (from repository root)
../../tools/build . --watch

# Run tests with watch mode
../../tools/build . --test
```

### Working with the Full BAML Stack

From the repository root (`/home/greghale/code/baml/`):

```bash
# Build all Rust components
cd engine && cargo build

# Run all Rust tests
cd engine && cargo test --features skip-integ-tests

# Build and test specific component
./tools/build engine/baml-compiler --test

# Run integration tests
cd integ-tests && ./run-tests.sh
```

## Architecture

### Compiler Structure

The baml-compiler transforms BAML source code → AST → Bytecode:

1. **Parser** (from parser-database) → AST
2. **Validation** (from baml-core) → Validated IR
3. **Compiler** (this crate) → VM Bytecode

Key files:
- `src/lib.rs` - Main compiler entry point and bytecode generation
- Dependencies on:
  - `internal-baml-core` - IR and validation
  - `internal-baml-parser-database` - Parsing and AST
  - `baml-vm` - Bytecode definitions

### Bytecode Instructions

The compiler generates stack-based bytecode with instructions like:
- `LoadConst`, `LoadVar`, `LoadGlobal` - Load values
- `Call`, `Return` - Function calls
- `Jump`, `JumpIfFalse` - Control flow
- `AllocArray`, `Pop` - Data structures

### Current Limitations

Several features are marked as `todo!()` in the implementation:
- Raw strings
- Maps
- Class constructors
- Lambdas
- Loops (for, while)

## Working with BAML Code

BAML is a domain-specific language for building reliable AI workflows. When modifying the compiler:

1. Changes to bytecode generation affect the VM execution
2. New language features require updates to:
   - Parser (in parser-database)
   - IR/validation (in baml-core)
   - Compiler (this crate)
   - VM (in baml-vm)

3. Test changes using integration tests in `/integ-tests/`

## Testing Individual Components

```bash
# Run a specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run only compiler tests (not workspace)
cargo test --package baml-compiler
```

## Related Components

When working on the compiler, you may need to understand:
- `baml-lib/parser-database/` - How BAML is parsed
- `baml-lib/baml-core/` - The IR and type system
- `baml-vm/` - How bytecode is executed
- `baml-runtime/` - How the runtime orchestrates execution