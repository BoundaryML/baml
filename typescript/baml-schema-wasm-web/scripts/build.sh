#!/usr/bin/env bash

set -euo pipefail

cd ../../engine/baml-schema-wasm
# maybe use dev and not --release?
cargo build --target=wasm32-unknown-unknown --color=always --release
# engines dir
cd ..
echo "Path is: $(pwd)"
bash ./baml-schema-wasm/scripts/update-schema-wasm.sh web