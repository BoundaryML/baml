#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
WORKSPACE_ROOT="$(cd ../../.. && pwd)"
BRIDGE_WEB="$WORKSPACE_ROOT/sdks/web/bridge_web"
export npm_config_store_dir="$WORKSPACE_ROOT/target/pnpm-store"
mkdir -p "$npm_config_store_dir"

(cd "$BRIDGE_WEB" && pnpm install --ignore-workspace --ignore-scripts)
(cd "$BRIDGE_WEB" && pnpm build:debug)

for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    (cd "$fixture_dir" && pnpm install --force --ignore-workspace --ignore-scripts)
    (cd "$fixture_dir" && pnpm update @boundaryml/baml-bridge-web --force --ignore-workspace --ignore-scripts)
done

if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_TYPESCRIPT_WEB_SETUP=1" >> "$NEXTEST_ENV"
fi
