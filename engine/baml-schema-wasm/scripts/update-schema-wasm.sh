#!/usr/bin/env bash

set -euo pipefail

## Dirs
baml_schema_wasm_dir="./baml-schema-wasm"
dist_dir="../typescript/baml-schema-wasm/dist"

[ ! -d "$baml_schema_wasm_dir" ] && echo "baml_schema_wasm was not found in the current directory" && exit 1

## Check if dist directory exists, if not create it
[ ! -d "$dist_dir" ] && mkdir -p "$dist_dir"

## Script
printf '%s\n' "Starting build :: baml-schema-wasm"
cargo build --release --target=wasm32-unknown-unknown --manifest-path=$baml_schema_wasm_dir/Cargo.toml

printf '%s\n' "Generating node module"
out=$baml_schema_wasm_dir/nodejs $baml_schema_wasm_dir/scripts/install.sh

printf '%s\n' "Moving generated baml-schema-wasm :: engines -> language-tools"
cp $baml_schema_wasm_dir/nodejs/src/baml_schema_build{_bg.wasm,_bg.wasm.d.ts,.d.ts,.js} $dist_dir
