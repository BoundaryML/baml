#!/usr/bin/env bash

set -euo pipefail

cd ../../engine/baml-schema-wasm
cargo build --color=always
# engines dir
cd ..
echo "Path is: $(pwd)"
bash ./baml-schema-wasm/scripts/update-schema-wasm.sh node