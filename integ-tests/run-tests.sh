#!/bin/bash
set -e

# TypeScript Tests
cd typescript
pnpm install
pnpm run build:debug
pnpm run generate
pnpm run integ-tests
cd ..

# Python Tests
cd python
uv sync
uv run maturin develop --uv --manifest-path ../../engine/language_client_python/Cargo.toml
uv run baml-cli generate --from ../baml_src
uv run pytest
cd ..

# Ruby Tests
cd ruby
bundle install
rake generate
rake test
cd ..
