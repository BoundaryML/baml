#!/bin/bash

# Run tests for CI

set -euxo pipefail

# uv sync
uv run maturin develop --uv --manifest-path ../../engine/language_client_python/Cargo.toml
uv run baml-cli generate --from ../baml_src

# These tests are excluded because they require credentials.
uv run pytest "$@" \
    --ignore=tests/test_functions.py \
    --ignore=tests/test_errors.py \
    --ignore=tests/test_collector.py \
    --ignore=tests/test_with_options.py \
    --ignore=tests/test_pydantic_video.py \
    --ignore=tests/test_modular_api.py \
    --ignore=tests/test_logger.py \
    --ignore=tests/test_typebuilder.py \
    --ignore=tests/test_vm_async_runtime.py \
    --ignore=tests/test_ontick.py \
    --ignore=tests/test_abort_handlers.py \
    --ignore=tests/test_abort_handlers_simple.py \
    --ignore=tests/test_emit.py \
