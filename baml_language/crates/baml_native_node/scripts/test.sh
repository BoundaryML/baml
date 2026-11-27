#!/usr/bin/env bash
# Run today's type_shapes tests, the argument-parity tests and the native
# bridge's own tests against the native bindings: with the defaults, with the
# eager turn, with the two other ways of building instances, and with a
# collection on every allocation. `scripts/stage.sh` makes the package first.
#
#   scripts/test.sh [vitest args...]
set -euo pipefail
cd "$(dirname "$0")/../../../target/native-node/native"
pnpm exec tsc -p tsconfig.dist.json
cp node/baml_sdk/libtype_shapes.dylib dist/baml_sdk/
status=0
run() {
  echo "==> $*"
  env "$@" pnpm exec vitest run --config vitest.node.config.ts ${ARGS[@]+"${ARGS[@]}"} || status=$?
}
ARGS=("$@")
run BAML_NATIVE_TURN=microtask BAML_NATIVE_BUILD=positional
run BAML_NATIVE_TURN=eager
run BAML_NATIVE_BUILD=props
run BAML_NATIVE_BUILD=init
run BAML_GC_STRESS=1
exit $status
