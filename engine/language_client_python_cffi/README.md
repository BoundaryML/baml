# BAML Python CFFI Client

A new Python client for BAML using CFFI (C Foreign Function Interface) that provides direct access to the BAML runtime.

## Overview

This client is designed to replace the existing pyo3-based Python client and follows the architecture of the Go CFFI client, providing:

- Direct FFI bindings to the BAML runtime
- Async/await support using Python's asyncio
- Type-safe function calls
- Streaming support
- Media type handling

## Installation

```bash
# For users
uv add baml-py-cffi

# For development
uv sync
```

## Development

This package uses `uv` for dependency management and is currently under development as part of the BAML project.

### Setting up the development environment

```bash
# Sync dependencies (creates venv automatically)
uv sync --all-extras

# The virtual environment is created at .venv automatically
# No need to activate it when using uv run
```

### Phase 1 Status
- ✅ Package structure created
- ✅ Library loading logic implemented
- ✅ Basic FFI bindings for version() function
- ✅ Package metadata configured

### Running Tests

```bash
# Using Makefile (recommended)
make test                    # Run tests
make test-cov               # Run tests with coverage
make dev                    # Format, lint, typecheck, and test

# Using shell script
./run_tests.sh

# Or manually with uv
uv sync --all-extras
BAML_LIBRARY_PATH=/path/to/libbaml_cffi.dylib uv run --extra dev pytest tests/
```

**Note**: The tests require a built BAML CFFI library. You can build it with:
```bash
cd ../../ && cargo build
```

### Code Quality

```bash
# Using Makefile
make format                 # Format code with black
make lint                   # Lint code with ruff  
make typecheck              # Type check with mypy
make dev                    # Run all checks and tests

# Or manually with uv
uv sync --all-extras
uv run --extra dev black baml_py_cffi tests
uv run --extra dev ruff check baml_py_cffi tests
uv run --extra dev mypy baml_py_cffi
```

### Available Make Commands

```bash
make help                   # Show all available commands
make sync                   # Sync dependencies
make test                   # Run tests
make test-cov              # Run tests with coverage
make format                # Format code
make lint                  # Lint code
make typecheck             # Type check
make dev                   # Full development workflow
make clean                 # Clean up generated files
```

## License

Apache-2.0