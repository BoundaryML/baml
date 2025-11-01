# BAML Compiler Tests

This crate contains all tests for the BAML compiler. Tests are automatically generated from BAML projects in the `projects/` directory.

## Adding Tests

1. Create a new folder in `projects/`:
```bash
mkdir projects/my_test
```

2. Add BAML files:
```bash
echo "class Foo {}" > projects/my_test/main.baml
```

3. Run tests:
```bash
cargo test -p baml_tests my_test::
```

That's it! No test code to write.

## How It Works

The build script (`build.rs`) automatically:
1. Discovers all folders in `projects/`
2. Finds all `.baml` files in each folder
3. Generates comprehensive tests for each compiler phase
4. Creates snapshots for all outputs

## Running Tests

```bash
# Run all tests
cargo test -p baml_tests

# Run specific project tests
cargo test -p baml_tests simple_function::

# Update snapshots
cargo insta test --accept

# Review snapshots
cargo insta review
```

## Project Structure

```
projects/
├── simple_function/     # Each folder is a test project
│   └── main.baml
├── complex_types/
│   ├── types.baml      # Can have multiple files
│   └── functions.baml
└── nested/
    └── models/         # Can have nested directories
        └── user.baml
```

## What Gets Tested

For each project, we automatically test:
- **Lexer**: Token generation for each file
- **Parser**: Syntax tree for each file
- **HIR**: Name resolution for the whole project
- **THIR**: Type checking for the whole project
- **Diagnostics**: All errors and warnings
- **Codegen**: Bytecode generation

All results are captured in snapshots for easy review.

## Benchmarks

This crate also includes comprehensive performance benchmarks. See [BENCHMARKS.md](BENCHMARKS.md) for details.

```bash
# Run benchmarks
cargo bench --bench compiler_benchmark

# Run specific benchmark
cargo bench --bench compiler_benchmark bench_incremental_add_user_field
```