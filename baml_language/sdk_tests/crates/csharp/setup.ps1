$ErrorActionPreference = "Stop"

$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "../../..")).Path
Push-Location $WorkspaceRoot
try {
    cargo build --locked --release -p bridge_cffi
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}
finally {
    Pop-Location
}

$NativeLibrary = Join-Path $WorkspaceRoot "target/release/bridge_cffi.dll"
if (-not (Test-Path $NativeLibrary)) { throw "missing $NativeLibrary" }

if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_CSHARP_SETUP=1"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_RUNTIME_PATH=$NativeLibrary"
    Add-Content -Path $env:NEXTEST_ENV -Value "NuGetAudit=false"
}
