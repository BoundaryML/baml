$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$WorkspaceRoot = (Resolve-Path '..\..\..').Path
$BridgeWeb = Join-Path $WorkspaceRoot 'sdks\web\bridge_web'
$env:npm_config_store_dir = Join-Path $WorkspaceRoot 'target\pnpm-store'
New-Item -ItemType Directory -Force -Path $env:npm_config_store_dir | Out-Null

Push-Location $BridgeWeb
try {
    pnpm install --ignore-workspace --ignore-scripts
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    pnpm build:debug
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { Pop-Location }

Get-ChildItem -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName 'generated'
    if (Test-Path $generated) {
        Push-Location $generated
        try {
            pnpm install --force --ignore-workspace --ignore-scripts
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            pnpm update @boundaryml/baml-bridge-web --force --ignore-workspace --ignore-scripts
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally { Pop-Location }
    }
}

if ($env:NEXTEST_ENV) { Add-Content -Path $env:NEXTEST_ENV -Value 'SDK_TEST_TYPESCRIPT_WEB_SETUP=1' }
