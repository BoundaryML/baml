# Web/Wasm bridge and generated fixture setup for sdk_test_typescript_web.

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$WorkspaceRoot = (Resolve-Path '..\..\..').Path
$BridgeTypescriptWeb = Join-Path $WorkspaceRoot 'sdks\typescript\bridge_typescript_web'

$env:npm_config_store_dir = Join-Path $WorkspaceRoot 'target\pnpm-store'
New-Item -ItemType Directory -Force -Path $env:npm_config_store_dir | Out-Null

Write-Host '==> pnpm install in sdks/typescript/bridge_typescript_web'
Push-Location $BridgeTypescriptWeb
try {
    pnpm install
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    pnpm build:debug
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

Get-ChildItem -Directory | ForEach-Object {
    $Generated = Join-Path $_.FullName 'generated'
    if (Test-Path $Generated) {
        Write-Host "==> pnpm install in $($_.Name)/generated"
        Push-Location $Generated
        try {
            pnpm install --force --ignore-workspace --ignore-scripts
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
            pnpm update @boundaryml/baml-bridge-web --force --ignore-workspace --ignore-scripts
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
    }
}

$FirstGenerated = Get-ChildItem -Directory | ForEach-Object {
    $Generated = Join-Path $_.FullName 'generated'
    if (Test-Path $Generated) { $Generated }
} | Select-Object -First 1
if ($FirstGenerated) {
    Write-Host '==> playwright install chromium'
    Push-Location $FirstGenerated
    try {
        pnpm exec playwright install chromium
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    } finally {
        Pop-Location
    }
}

if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value 'SDK_TEST_TYPESCRIPT_WEB_SETUP=1'
}
