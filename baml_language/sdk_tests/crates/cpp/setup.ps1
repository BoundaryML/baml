# Native-build setup for the sdk_test_cpp crate (Windows counterpart of
# setup.sh; byte-for-byte equivalent responsibilities).
$ErrorActionPreference = "Stop"

Set-Location $PSScriptRoot
$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "../../..")).Path

Push-Location $workspaceRoot
try {
    # Generate each fixture's baml_sdk/ and test.sh first: nothing else
    # produces them, and every step below compiles against them.
    Write-Output "==> sdk_test_codegen cpp (generate fixture SDKs)"
    cargo run --quiet -p sdk_test_codegen -- cpp
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    Write-Output "==> cargo build -p bridge_cffi (dev cdylib for cpp sdk tests)"
    cargo build -p bridge_cffi --no-default-features --features ring-crypto,bundle-http
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}

if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_CPP_SETUP=1"
}
