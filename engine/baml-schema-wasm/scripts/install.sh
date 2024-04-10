#!/usr/bin/env bash

set -euo pipefail

printf '%s\n' "entering install.sh"

printf '%s\n' " -> Creating out dir..."
# shellcheck disable=SC2154
mkdir -p "$out"/src

printf '%s\n' " -> Generating $target package"
wasm-bindgen \
  --target "$target" \
  --out-dir "$out"/src \
  target/wasm32-unknown-unknown/release/baml_schema_build.wasm