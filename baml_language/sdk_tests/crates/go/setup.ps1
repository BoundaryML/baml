$ErrorActionPreference = "Stop"

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = (Resolve-Path (Join-Path $scriptDir "../../..")).Path

Push-Location $workspaceRoot
try {
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
