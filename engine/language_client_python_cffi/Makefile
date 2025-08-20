# Makefile for BAML Python CFFI Client

# Default BAML library path
BAML_LIBRARY_PATH ?= /Users/greghale/code/baml-4/engine/target/debug/libbaml_cffi.dylib

.PHONY: sync test format lint typecheck clean all dev

# Default target
all: sync test

# Sync dependencies
sync:
	@echo "Syncing dependencies..."
	@uv sync --all-extras

# Run tests
test:
	@echo "Running tests..."
	@BAML_LIBRARY_PATH=$(BAML_LIBRARY_PATH) uv run --extra dev pytest tests/ -v

# Run tests with coverage
test-cov:
	@echo "Running tests with coverage..."
	@BAML_LIBRARY_PATH=$(BAML_LIBRARY_PATH) uv run --extra dev pytest tests/ --cov=baml_py_cffi --cov-report=html -v

# Format code
format:
	@echo "Formatting code..."
	@uv run --extra dev black baml_py_cffi tests

# Lint code
lint:
	@echo "Linting code..."
	@uv run --extra dev ruff check baml_py_cffi tests

# Type check
typecheck:
	@echo "Type checking..."
	@uv run --extra dev mypy baml_py_cffi

# Development workflow: format, lint, typecheck, test
dev: format lint typecheck test

# Clean up
clean:
	@echo "Cleaning up..."
	@rm -rf .venv
	@rm -rf .pytest_cache
	@rm -rf .mypy_cache
	@rm -rf .ruff_cache
	@rm -rf htmlcov
	@rm -rf __pycache__
	@rm -rf baml_py_cffi/__pycache__
	@rm -rf tests/__pycache__
	@find . -name "*.pyc" -delete
	@find . -name "*.pyo" -delete

# Install pre-commit hooks (if using pre-commit)
install-hooks:
	@echo "Installing pre-commit hooks..."
	@uv run --extra dev pre-commit install

# Show help
help:
	@echo "Available targets:"
	@echo "  make sync       - Sync dependencies with uv"
	@echo "  make test       - Run tests"
	@echo "  make test-cov   - Run tests with coverage"
	@echo "  make format     - Format code with black"
	@echo "  make lint       - Lint code with ruff"
	@echo "  make typecheck  - Type check with mypy"
	@echo "  make dev        - Run full development workflow"
	@echo "  make clean      - Clean up generated files"
	@echo "  make help       - Show this help message"