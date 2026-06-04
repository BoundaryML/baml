# Per-fixture pnpm setup for the sdk_test_typescript_node crate — Windows.
#
# Windows counterpart of setup.sh. Invoked automatically by
# `cargo nextest run` via the setup-script binding in
# `baml_language/.config/nextest.toml` (host = cfg(windows)).
# Run the suite with `cargo nextest run`, not `cargo test`: plain
# `cargo test` skips this script and can't pass `setup_guard::ran`
# (see ..\..\README.md).
#
# See setup.sh for the rationale on `pnpm build:debug` + per-fixture
# `pnpm install --ignore-workspace`; the steps are identical.

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$WorkspaceRoot = (Resolve-Path '..\..\..').Path
$RepoRoot = (Resolve-Path (Join-Path $WorkspaceRoot '..')).Path
$BridgeNodejs = Join-Path $WorkspaceRoot 'sdks\nodejs\bridge_nodejs'

# Shared pnpm store so per-fixture installs hardlink from one location.
# Defaults under target/; CI overrides via BAML_SDK_PNPM_STORE_DIR to a
# path OUTSIDE target/ so it survives rust-cache's pre-save target cleanup
# and can be cached (build-caching/04 D-fix).
if ($env:BAML_SDK_PNPM_STORE_DIR) {
    $env:npm_config_store_dir = $env:BAML_SDK_PNPM_STORE_DIR
} else {
    $env:npm_config_store_dir = Join-Path $WorkspaceRoot 'target\pnpm-store'
}
New-Item -ItemType Directory -Force -Path $env:npm_config_store_dir | Out-Null

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
Write-Host '==> pnpm install in sdks/nodejs/bridge_nodejs'
Push-Location $BridgeNodejs
try {
    pnpm install --ignore-workspace --ignore-scripts
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# 3. Native `.node` addon.
Write-Host '==> pnpm build:debug in sdks/nodejs/bridge_nodejs'
Push-Location $BridgeNodejs
try {
    pnpm build:debug
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# 4. Per-fixture `pnpm install`. `--ignore-workspace` is required so pnpm
#    doesn't walk up to the repo-root workspace and skip the install.
#    `--force` is load-bearing: pnpm otherwise treats the fixture lockfile as
#    current and may keep a stale packed copy of the local `file:` dependency
#    when bridge_nodejs build output changes.
Get-ChildItem -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName 'generated'
    if (Test-Path $generated) {
        Write-Host "==> pnpm install in $($_.Name)/generated"
        Push-Location $generated
        try {
            # --ignore-scripts: protobufjs is the only dep with a build
            # script and needs none for our pure-JS usage; without it pnpm 9+
            # exits non-zero (ERR_PNPM_IGNORED_BUILDS). See setup.sh.
            pnpm install --force --ignore-workspace --ignore-scripts
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
    }
}

# Per-run breadcrumb for the `setup_guard::ran` test. See the
# "setup.sh guard" section of ..\..\README.md for the format and
# rationale. Keep the var name in sync with SETUP_ENV_VAR in
# harness_setup/src/typescript_node.rs.
if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value 'SDK_TEST_TYPESCRIPT_NODE_SETUP=1'
}
