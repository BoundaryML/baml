# HIR and Bytecode Golden Tests

This document explains how to use the HIR (High-level Intermediate Representation) and bytecode golden test infrastructure.

## Overview

The golden test infrastructure allows you to write tests where the expected output is stored as comments at the bottom of the test file. This follows the same pattern as the existing `validation_files` tests.

## Directory Structure

- `hir_files/` - Contains BAML files for testing HIR output
- `bytecode_files/` - Contains BAML files for testing bytecode generation

## Test File Format

Each test file should contain:
1. Valid BAML code at the top
2. Expected output in comments at the bottom

Example:
```baml
class Person {
  name string
  age int
}

// Expected HIR output goes here
// Each line of expected output should be prefixed with //
```

## Running Tests

```bash
# Run all tests
cargo test

# Run only HIR tests
cargo test hir_tests

# Run only bytecode tests
cargo test bytecode_tests

# Update expected output (golden snapshots)
UPDATE_EXPECT=1 cargo test
```

## Adding New Tests

1. Create a new `.baml` file in either `hir_files/` or `bytecode_files/`
2. Write your BAML code
3. Run `UPDATE_EXPECT=1 cargo test` to generate the initial expected output
4. Review the generated output to ensure it's correct
5. Commit the file with the expected output

## Implementation Notes

### Current Limitations

The test runners currently return placeholder messages because they require additional dependencies:
- HIR tests need the `baml_compiler` crate to access `hir::Program`
- Bytecode tests need both `baml_compiler` and `baml_vm` crates

To fully implement these tests, the following dependencies should be added to `Cargo.toml`:
```toml
[dev-dependencies]
baml_compiler = { path = "../baml-compiler" }
baml_vm = { path = "../baml-vm" }
```

### How It Works

1. **Build Script** (`build.rs`):
   - Scans the `hir_files/` and `bytecode_files/` directories
   - Generates test functions for each `.baml` file
   - Outputs to `OUT_DIR/hir_tests.rs` and `OUT_DIR/bytecode_tests.rs`

2. **Test Runners**:
   - `hir_tests.rs` - Parses BAML, generates HIR, and compares with expected output
   - `bytecode_tests.rs` - Compiles BAML to bytecode and compares with expected output

3. **Golden Test Pattern**:
   - Expected output is stored as comments at the end of each test file
   - When `UPDATE_EXPECT=1` is set, the test runner updates the expected output
   - Otherwise, it compares actual output with expected and fails if they don't match

## Example Test Workflow

1. Create a new test file:
   ```bash
   echo 'class User { id int }' > tests/hir_files/user_class.baml
   ```

2. Generate expected output:
   ```bash
   UPDATE_EXPECT=1 cargo test user_class
   ```

3. Verify the test passes:
   ```bash
   cargo test user_class
   ```

4. If the HIR/bytecode format changes, update all tests:
   ```bash
   UPDATE_EXPECT=1 cargo test
   ```