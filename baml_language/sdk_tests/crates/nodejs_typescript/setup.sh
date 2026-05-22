#!/usr/bin/env bash
# Per-fixture pnpm setup for the sdk_test_nodejs_typescript crate.
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` — whenever the run
# selects any sdk_test_nodejs_typescript test. For plain `cargo test`
# (no nextest) run this manually after `cargo test --no-run` populates
# each fixture's `generated/package.json`.
#
# This script turns those generated/ dirs into a real `node_modules/`
# tree and builds the native `.node` addon each fixture `file:`-links
# to. Idempotent — re-runs are no-ops in steady state. Re-run after
# bridge_nodejs Rust changes (addon is stale) or after adding a new
# fixture.

set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/nodejs_typescript

WORKSPACE_ROOT="$(cd ../../.. && pwd)"
BRIDGE_NODEJS="$WORKSPACE_ROOT/sdks/nodejs/bridge_nodejs"

# Shared pnpm store under target/ so per-fixture installs hardlink from
# one location rather than fetching N copies.
export npm_config_store_dir="$WORKSPACE_ROOT/target/pnpm-store"
mkdir -p "$npm_config_store_dir"

# 1. Native `.node` addon. `pnpm build:debug` chains build:proto →
#    build:napi-debug → build:ts_build → build:tag-generated-files; all
#    incremental, no-op steady-state is cheap. Skipping this step makes
#    every per-fixture jest fail with "Cannot find module
#    './baml_node.<triple>.node'".
#
#    We do NOT `pnpm install` here — bridge_nodejs is a member of the
#    repo-root pnpm-workspace.yaml. `pnpm install --ignore-workspace`
#    inside it would wipe the workspace-installed `node_modules/` and
#    reinstall with broken peer-dep resolution. If `node_modules/` is
#    missing, `build:debug` fails with `pbjs: command not found` —
#    that's the cue to run `pnpm install` at the repo root once.
echo "==> pnpm build:debug in sdks/nodejs/bridge_nodejs"
(cd "$BRIDGE_NODEJS" && pnpm build:debug)

# 2. Per-fixture `pnpm install`. `--ignore-workspace` is required so pnpm
#    doesn't walk up to the repo-root workspace and skip the install.
#    The per-fixture `package.json` `file:`-points at bridge_nodejs, so
#    the install resolves the dev toolchain (jest, ts-jest, typescript)
#    plus the `@boundaryml/baml-node` runtime dep against the addon we
#    just built.
for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    echo "==> pnpm install in $fixture_dir"
    (cd "$fixture_dir" && pnpm install --ignore-workspace)
done
