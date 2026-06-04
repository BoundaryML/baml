#!/usr/bin/env bash
# Per-fixture pnpm setup for the sdk_test_typescript_node crate — Unix.
# Windows uses the parallel `setup.ps1`; keep the two in sync.
#
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` — whenever the run
# selects any sdk_test_typescript_node test. Run the suite with
# `cargo nextest run`, not `cargo test`: plain `cargo test` skips this
# script and can't pass `setup_guard::ran` (see ../../README.md).
#
# This script turns those generated/ dirs into a real `node_modules/`
# tree and builds the native `.node` addon each fixture `file:`-links
# to. Idempotent — re-runs are no-ops in steady state. Re-run after
# bridge_nodejs Rust changes (addon is stale) or after adding a new
# fixture.

set -euo pipefail

cd "$(dirname "$0")"  # baml_language/sdk_tests/crates/typescript_node

WORKSPACE_ROOT="$(cd ../../.. && pwd)"
REPO_ROOT="$(cd "$WORKSPACE_ROOT/.." && pwd)"
BRIDGE_NODEJS="$WORKSPACE_ROOT/sdks/nodejs/bridge_nodejs"

# Shared pnpm store so per-fixture installs hardlink from one location
# rather than fetching N copies. Defaults under target/; CI overrides via
# BAML_SDK_PNPM_STORE_DIR to a path OUTSIDE target/ so it survives
# rust-cache's pre-save target cleanup and can be cached (build-caching/04
# D-fix — under target/ it was wiped every run → pnpm install ~79s cold).
export npm_config_store_dir="${BAML_SDK_PNPM_STORE_DIR:-$WORKSPACE_ROOT/target/pnpm-store}"
mkdir -p "$npm_config_store_dir"

# bridge_nodejs is self-contained: its own pnpm-lock.yaml, no `workspace:`
# deps, and fixtures depend on it via a `file:` path — so the SDK tests need
# nothing from the repo-root pnpm workspace. We deliberately do NOT run a
# repo-root `pnpm install`: that pulled all 39 workspace projects (~2928 pkgs:
# canvas, turbo, vercel, the vscode-ext, the codemirror grammar build, … none
# used here) and added ~80s. The bridge-local install below populates
# bridge_nodejs/node_modules standalone — all `pnpm build:debug` needs.

# 1. Bridge-local deps. The bridge package has its own lockfile and build
#    scripts invoke tools from bridge_nodejs/node_modules (for example
#    node_modules/protobufjs-cli/bin/pbjs), so make that install explicit.
echo "==> pnpm install in sdks/nodejs/bridge_nodejs"
(cd "$BRIDGE_NODEJS" && pnpm install --ignore-workspace --ignore-scripts)

# 3. Native `.node` addon. `pnpm build:debug` chains build:proto →
#    build:napi-debug → build:ts_build → build:tag-generated-files; all
#    incremental, no-op steady-state is cheap. Skipping this step makes
#    every per-fixture vitest fail with "Cannot find module
#    './baml_node.<triple>.node'".
echo "==> pnpm build:debug in sdks/nodejs/bridge_nodejs"
(cd "$BRIDGE_NODEJS" && pnpm build:debug)

# 4. Per-fixture `pnpm install`. `--ignore-workspace` is required so pnpm
#    doesn't walk up to the repo-root workspace and skip the install.
#    The per-fixture `package.json` `file:`-points at bridge_nodejs, so
#    the install resolves the dev toolchain (vitest, typescript)
#    plus the `@boundaryml/baml-core-node` runtime dep against the addon we
#    just built. `--force` is load-bearing: pnpm otherwise treats the fixture
#    lockfile as current and may keep a stale packed copy of the local `file:`
#    dependency when bridge_nodejs build output changes.
for fixture_dir in */generated; do
    [[ -d "$fixture_dir" ]] || continue
    echo "==> pnpm install in $fixture_dir"
    # `--ignore-scripts`: the only dependency with a build script is
    # protobufjs, which needs no build for our pure-JS usage (we ship a
    # pre-generated ESM `baml_cffi.js` at
    # runtime). Without this flag pnpm 9+ exits non-zero with
    # ERR_PNPM_IGNORED_BUILDS, which `set -e` would treat as a fatal abort.
    (cd "$fixture_dir" && pnpm install --force --ignore-workspace --ignore-scripts)
done

# Per-run breadcrumb for the `setup_guard::ran` test. See the
# "setup.sh guard" section of ../../README.md for the format and
# rationale. Keep the var name in sync with SETUP_ENV_VAR in
# harness_setup/src/typescript_node.rs.
if [[ -n "${NEXTEST_ENV:-}" ]]; then
    echo "SDK_TEST_TYPESCRIPT_NODE_SETUP=1" >> "$NEXTEST_ENV"
fi
