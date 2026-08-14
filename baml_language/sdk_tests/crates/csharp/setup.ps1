$ErrorActionPreference = "Stop"

$CrateDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = (Resolve-Path (Join-Path $CrateDir "../../..")).Path

Push-Location $WorkspaceRoot
try {
    cargo build -p bridge_cffi
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build -p bridge_cffi failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $WorkspaceRoot "target" }
if (-not [System.IO.Path]::IsPathRooted($TargetDir)) {
    $TargetDir = Join-Path $WorkspaceRoot $TargetDir
}
$TargetDir = [System.IO.Path]::GetFullPath($TargetDir)
$NativeLibrary = Join-Path $TargetDir "debug/bridge_cffi.dll"

if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_CSHARP_SETUP=1"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_BRIDGE_CSHARP_NATIVE_LIBRARY=$NativeLibrary"
}
