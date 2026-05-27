# Per-fixture pnpm setup for the sdk_test_nodejs_typescript crate — Windows.
#
# Windows counterpart of setup.sh. Invoked automatically by
# `cargo nextest run` via the setup-script binding in
# `baml_language/.config/nextest.toml` (host = cfg(windows)).
# For plain `cargo test` (no nextest), run this manually after
# `cargo test --no-run` populates each fixture's `generated/package.json`.
#
# See setup.sh for the rationale on `pnpm build:debug` + per-fixture
# `pnpm install --ignore-workspace`; the steps are identical.

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$WorkspaceRoot = (Resolve-Path '..\..\..').Path
$BridgeNodejs = Join-Path $WorkspaceRoot 'sdks\nodejs\bridge_nodejs'

# Shared pnpm store under target/ so per-fixture installs hardlink from
# one location rather than fetching N copies.
$env:npm_config_store_dir = Join-Path $WorkspaceRoot 'target\pnpm-store'
New-Item -ItemType Directory -Force -Path $env:npm_config_store_dir | Out-Null

# 1. Native `.node` addon. See setup.sh for why we do NOT `pnpm install`
#    inside bridge_nodejs (it's a member of the repo-root workspace).
Write-Host '==> pnpm build:debug in sdks/nodejs/bridge_nodejs'
Push-Location $BridgeNodejs
try {
    pnpm build:debug
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# 2. Per-fixture `pnpm install`. `--ignore-workspace` is required so pnpm
#    doesn't walk up to the repo-root workspace and skip the install.
Get-ChildItem -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName 'generated'
    if (Test-Path $generated) {
        Write-Host "==> pnpm install in $($_.Name)/generated"
        Push-Location $generated
        try {
            pnpm install --ignore-workspace
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
    }
}
