#!/usr/bin/env bash
# Per-fixture pnpm setup for the sdk_test_typescript crate — Unix.
# Windows uses the parallel `setup.ps1`; keep the two in sync.
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` — whenever the run
# selects any sdk_test_typescript test. Run the suite with
# `cargo nextest run`, not `cargo test`: plain `cargo test` skips this
# script and can't pass `setup_guard::ran` (see ../../README.md).
#
# This script turns those generated/ dirs into a real `node_modules/`
# tree and builds only the native TypeScript bridge. Web/Wasm setup lives in
# the sibling `typescript_web` crate.

set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/typescript

WORKSPACE_ROOT="$(cd ../../.. && pwd)"
REPO_ROOT="$(cd "$WORKSPACE_ROOT/.." && pwd)"
BRIDGE_TYPESCRIPT="$WORKSPACE_ROOT/sdks/typescript/bridge_typescript"

# Shared pnpm store under target/ so per-fixture installs hardlink from
# one location rather than fetching N copies.
export npm_config_store_dir="$WORKSPACE_ROOT/target/pnpm-store"
mkdir -p "$npm_config_store_dir"

# 1. Workspace deps. bridge_typescript is a member of the repo-root
#    pnpm-workspace.yaml, and `pnpm build:debug` needs that workspace
#    install to have populated bridge_typescript/node_modules.
echo "==> pnpm install at repo root"
(cd "$REPO_ROOT" && pnpm install)

# 2. Bridge-local deps. The bridge package has its own lockfile and build
#    scripts invoke tools from bridge_typescript/node_modules (for example
#    node_modules/protobufjs-cli/bin/pbjs), so make that install explicit.
echo "==> pnpm install in sdks/typescript/bridge_typescript"
(cd "$BRIDGE_TYPESCRIPT" && pnpm install --ignore-workspace --ignore-scripts)

# 3. Native `.node` addon. `pnpm build:debug` chains build:proto →
#    build:napi-debug → build:ts_build → build:tag-generated-files; all
#    incremental, no-op steady-state is cheap. Skipping this step makes
#    every per-fixture vitest fail with "Cannot find module
#    './baml_node.<triple>.node'".
echo "==> pnpm build:debug in sdks/typescript/bridge_typescript"
(cd "$BRIDGE_TYPESCRIPT" && pnpm build:debug)

# 4. Per-fixture `pnpm install`. `--ignore-workspace` is required so pnpm
#    doesn't walk up to the repo-root workspace and skip the install.
#    The per-fixture `package.json` `file:`-points at bridge_typescript, so
#    the install resolves the dev toolchain (vitest, typescript)
#    plus the `@boundaryml/baml-bridge` runtime dep against the addon we
#    just built. `--force` is load-bearing: pnpm otherwise treats the fixture
#    lockfile as current and may keep a stale packed copy of the local `file:`
#    dependency when bridge_typescript build output changes.
for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    echo "==> pnpm install in $fixture_dir"
    # `--ignore-scripts`: the only dependency with a build script is
    # protobufjs, which needs no build for our pure-JS usage (we ship a
    # pre-generated ESM `baml_cffi.js` at
    # runtime). Without this flag pnpm 9+ exits non-zero with
    # ERR_PNPM_IGNORED_BUILDS, which `set -e` would treat as a fatal abort.
    (cd "$fixture_dir" && pnpm install --force --ignore-workspace --ignore-scripts)
    (cd "$fixture_dir" && pnpm update @boundaryml/baml-bridge --force --ignore-workspace --ignore-scripts)
done

# Per-run breadcrumb for the `setup_guard::ran` test. See the
# "setup.sh guard" section of ../../README.md for the format and
# rationale. Keep the var name in sync with SETUP_ENV_VAR in
# harness_setup/src/typescript.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_TYPESCRIPT_SETUP=1" >> "$NEXTEST_ENV"
fi
