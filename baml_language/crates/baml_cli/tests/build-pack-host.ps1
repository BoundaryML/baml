# Build the native pack host once before nextest runs the pack_e2e cases in
# separate processes. Cargo has already built baml-cli by setup time, so its
# profile directory tells us which host artifact the tests need as a sibling.

$ErrorActionPreference = "Stop"

# CI builds tests AND both binaries together. The Rust helper validates the
# host beside CARGO_BIN_EXE; do not replace it with a default-feature build.
if ($env:BAML_PACK_HOST_PREBUILT -eq "1") {
    Write-Host "==> using pack host from the outer Cargo build"
    exit 0
}

$workspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
Set-Location $workspaceRoot

if ($env:CARGO_TARGET_DIR) {
    if ([System.IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) {
        $targetDir = $env:CARGO_TARGET_DIR
    } else {
        $targetDir = Join-Path $workspaceRoot $env:CARGO_TARGET_DIR
    }
} else {
    $targetDir = Join-Path $workspaceRoot "target"
}

$built = $false
if (Test-Path (Join-Path $targetDir "debug\baml-cli.exe")) {
    Write-Host "==> cargo build -p baml_pack_host (nextest pack_e2e setup)"
    cargo build -p baml_pack_host
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build -p baml_pack_host failed with exit code $LASTEXITCODE"
    }
    $built = $true
}
if (Test-Path (Join-Path $targetDir "release\baml-cli.exe")) {
    Write-Host "==> cargo build -p baml_pack_host --release (nextest pack_e2e setup)"
    cargo build -p baml_pack_host --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build -p baml_pack_host --release failed with exit code $LASTEXITCODE"
    }
    $built = $true
}

if (-not $built) {
    throw "baml-cli.exe was not found under $targetDir\debug or $targetDir\release"
}
if (-not $env:NEXTEST_ENV) {
    throw "nextest did not provide NEXTEST_ENV"
}

Add-Content -LiteralPath $env:NEXTEST_ENV -Value "BAML_PACK_HOST_PREBUILT=1"
