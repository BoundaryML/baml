# Per-fixture pnpm setup for the sdk_test_typescript crate — Windows.
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
$BridgeTypescript = Join-Path $WorkspaceRoot 'sdks\typescript\bridge_typescript'

# Shared pnpm store under target/ so per-fixture installs hardlink from
# one location rather than fetching N copies.
$env:npm_config_store_dir = Join-Path $WorkspaceRoot 'target\pnpm-store'
New-Item -ItemType Directory -Force -Path $env:npm_config_store_dir | Out-Null

# 1. Workspace deps. bridge_typescript is a member of the repo-root
#    pnpm-workspace.yaml, and `pnpm build:debug` needs that workspace
#    install to have populated bridge_typescript/node_modules.
Write-Host '==> pnpm install at repo root'
Push-Location $RepoRoot
try {
    pnpm install
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# 2. Bridge-local deps. The bridge package has its own lockfile and build
#    scripts invoke tools from bridge_typescript/node_modules (for example
#    node_modules/protobufjs-cli/bin/pbjs), so make that install explicit.
Write-Host '==> pnpm install in sdks/typescript/bridge_typescript'
Push-Location $BridgeTypescript
try {
    pnpm install --ignore-workspace --ignore-scripts
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# 3. Native `.node` addon.
Write-Host '==> pnpm build:debug in sdks/typescript/bridge_typescript'
Push-Location $BridgeTypescript
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
#    when bridge_typescript build output changes.
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
            pnpm update @boundaryml/baml-bridge --force --ignore-workspace --ignore-scripts
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
    }
}

# Per-run breadcrumb for the `setup_guard::ran` test. See the
# "setup.sh guard" section of ..\..\README.md for the format and
# rationale. Keep the var name in sync with SETUP_ENV_VAR in
# harness_setup/src/typescript.rs.
if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value 'SDK_TEST_TYPESCRIPT_SETUP=1'
}
