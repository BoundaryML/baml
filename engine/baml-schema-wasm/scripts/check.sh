#!/usr/bin/env bash

# Check that the build worked.

set -euo pipefail

echo -n '1. The final wasm file is not empty: '

EXPECTED_FINAL_WASM_FILE_PATH="$PRISMA_SCHEMA_WASM/src/prisma_schema_build_bg.wasm";
WASM_FILE_SIZE=$(wc -c "$EXPECTED_FINAL_WASM_FILE_PATH" | sed 's/ .*$//')

if [[ $WASM_FILE_SIZE == '0' ]]; then
    echo "Check phase failed: expected a non empty EXPECTED_FINAL_WASM_FILE_PAT"
    exit 1
fi

echo 'ok.'

# ~_~_~_~ #

echo '2. We can call the module directly and get back a valid result.'

REFORMATTED_MEOW=$($NODE -e "const prismaSchema = require('$PRISMA_SCHEMA_WASM'); console.log(prismaSchema.format('meow', '{}'))")

echo "REFORMATTED_MEOW=$REFORMATTED_MEOW"

if [[ $REFORMATTED_MEOW != 'meow' ]]; then
    echo "Check phase failed: expected the module version to be 'wasm', but got '$REFORMATTED_MEOW'"
    exit 1
fi

echo ' ok.'

# Signal to nix that the check is a success.
# shellcheck disable=SC2154
mkdir -p "$out"