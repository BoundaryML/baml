# Native-build setup for the sdk_test_cpp crate (Windows counterpart of
# setup.sh; byte-for-byte equivalent responsibilities).
$ErrorActionPreference = "Stop"

Set-Location $PSScriptRoot
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "../../..")).Path

Write-Output "==> cargo build -p bridge_cffi (dev cdylib for cpp sdk tests)"
Push-Location $workspaceRoot
try {
    cargo build -p bridge_cffi --no-default-features --features ring-crypto,bundle-http
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}

if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_CPP_SETUP=1"
}
