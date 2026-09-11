$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = (Resolve-Path (Join-Path $scriptDir "../../..")).Path

Push-Location $workspaceRoot
try {
    # Generate each fixture's baml_sdk/, go.mod and go.sum first: nothing else
    # produces them, and the `go mod tidy` loop below silently skips any
    # fixture whose generated/ is missing.
    Write-Output "==> sdk_test_codegen go (generate fixture SDKs)"
    cargo run --quiet -p sdk_test_codegen -- go
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    cargo build -p bridge_cffi
} finally {
    Pop-Location
}

$go = "go"
if (Get-Command mise -ErrorAction SilentlyContinue) {
    $go = (& mise which go).Trim()
}

Get-ChildItem -Path $scriptDir -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName "generated"
    if (Test-Path $generated) {
        Push-Location $generated
        try {
            Remove-Item Env:GOROOT -ErrorAction SilentlyContinue
            & $go mod tidy
            if ($LASTEXITCODE -ne 0) {
                throw "go mod tidy failed in $generated"
            }
        } finally {
            Pop-Location
        }
    }
}

if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_GO_SETUP=1"
}
