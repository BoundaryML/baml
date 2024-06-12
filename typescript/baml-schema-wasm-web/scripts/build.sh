#!/usr/bin/env bash

set -euo pipefail

cd ../../engine/baml-schema-wasm
# maybe use dev and not --release?
# add target=wasm32-unknown-unknown here?
cargo build --color=always --release
# engines dir
cd ..
echo "Path is: $(pwd)"
bash ./baml-schema-wasm/scripts/update-schema-wasm.sh web