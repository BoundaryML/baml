#!/usr/bin/env bash
# Run today's type_shapes tests and the native bridge's own tests against the
# native bindings, then all again with a collection on every allocation.
# `scripts/stage.sh` makes the package first.
#
#   scripts/test.sh [vitest args...]
set -euo pipefail
cd "$(dirname "$0")/../../../target/native-node/native"
pnpm exec tsc -p tsconfig.dist.json
cp node/baml_sdk/libtype_shapes.dylib dist/baml_sdk/
status=0
pnpm exec vitest run --config vitest.node.config.ts "$@" || status=$?
echo "==> again with BAML_GC_STRESS=1"
BAML_GC_STRESS=1 pnpm exec vitest run --config vitest.node.config.ts "$@" || status=$?
exit $status
