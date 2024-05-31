#!/usr/bin/env bash

set -euo pipefail


# Expecting first argument to be 'web' or 'node'
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <target>"
    echo "target: web | node"
    exit 1
fi

TARGET=$1

if [[ "$TARGET" != "web" && "$TARGET" != "node" ]]; then
    echo "Invalid target: $TARGET"
    echo "Target must be either 'web' or 'node'."
    exit 1
fi


## Dirs
baml_schema_wasm_dir="./baml-schema-wasm"
if [ "$TARGET" == "node" ]; then
    dist_dir="../typescript/baml-schema-wasm-node/dist"
else
    dist_dir="../typescript/baml-schema-wasm-web/dist"
fi

[ ! -d "$baml_schema_wasm_dir" ] && echo "baml_schema_wasm was not found in the current directory" && exit 1

## Check if dist directory exists, if not create it
[ ! -d "$dist_dir" ] && mkdir -p "$dist_dir"

## Script
printf '%s\n' "Starting build :: baml-schema-wasm"
cargo build --release --color=always --target=wasm32-unknown-unknown --manifest-path=$baml_schema_wasm_dir/Cargo.toml

## Build target specific steps
if [ "$TARGET" == "node" ]; then
    printf '%s\n' "Generating node module"
    out=$baml_schema_wasm_dir/nodejs target=nodejs $baml_schema_wasm_dir/scripts/install.sh

    printf '%s\n' "Moving generated baml-schema-wasm :: engines -> language-tools"
    cp $baml_schema_wasm_dir/nodejs/src/baml_schema_build{_bg.wasm,_bg.wasm.d.ts,.d.ts,.js} $dist_dir
elif [ "$TARGET" == "web" ]; then
    printf '%s\n' "Generating web module"
    out=$baml_schema_wasm_dir/web target=bundler $baml_schema_wasm_dir/scripts/install.sh

    printf '%s\n' "Moving generated baml-schema-wasm :: engines -> language-tools"
    cp $baml_schema_wasm_dir/web/src/baml_schema_build{_bg.js,_bg.wasm,_bg.wasm.d.ts,.d.ts,.js} $dist_dir
fi


