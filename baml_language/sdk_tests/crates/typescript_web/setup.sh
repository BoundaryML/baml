#!/usr/bin/env bash
# Web/Wasm bridge and generated fixture setup for sdk_test_typescript_web.
# The canonical checked-in tests live in the sibling typescript crate and are
# copied into this crate's generated trees by build.rs.

set -euo pipefail

cd "$(dirname "$0")"

WORKSPACE_ROOT="$(cd ../../.. && pwd)"
BRIDGE_TYPESCRIPT_WEB="$WORKSPACE_ROOT/sdks/typescript/bridge_typescript_web"

export npm_config_store_dir="$WORKSPACE_ROOT/target/pnpm-store"
mkdir -p "$npm_config_store_dir"

echo "==> pnpm install in sdks/typescript/bridge_typescript_web"
(cd "$BRIDGE_TYPESCRIPT_WEB" && pnpm install)

echo "==> pnpm build:debug in sdks/typescript/bridge_typescript_web"
(cd "$BRIDGE_TYPESCRIPT_WEB" && pnpm build:debug)

for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    echo "==> pnpm install in $fixture_dir"
    (cd "$fixture_dir" && pnpm install --force --ignore-workspace --ignore-scripts)
    (cd "$fixture_dir" && pnpm update @boundaryml/baml-bridge-web --force --ignore-workspace --ignore-scripts)
done

for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    echo "==> playwright install chromium"
    (cd "$fixture_dir" && pnpm exec playwright install chromium)
    break
done

if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_TYPESCRIPT_WEB_SETUP=1" >> "$NEXTEST_ENV"
fi
