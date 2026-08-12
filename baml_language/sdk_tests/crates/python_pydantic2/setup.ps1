# Per-fixture uv setup for the sdk_test_python_pydantic2 crate - Windows.
#
# Windows counterpart of setup.sh. Invoked automatically by
# `cargo nextest run` via the setup-script binding in
# `baml_language/.config/nextest.toml` (host = cfg(windows)).
# For plain `cargo test` (no nextest), run this manually after
# `cargo test --no-run` populates each fixture's `generated/pyproject.toml`.
#
# See setup.sh for the baseline rationale. The Windows CI path uses the
# build-caching/04 approach B optimization: build baml_bridge once as a
# wheel, then install that wheel into every fixture venv. This avoids the
# redundant per-fixture editable build that `uv sync` can trigger before
# the shared extension rebuild.

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$WorkspaceRoot = (Resolve-Path '..\..\..').Path
$SdkPy = Join-Path $WorkspaceRoot 'sdks\python'

# Shared uv cache under target/, matching the UV_CACHE_DIR the emitted
# tests thread through (run_test_cmd / CACHE_ENV_VAR in harness_setup).
$env:UV_CACHE_DIR = Join-Path $WorkspaceRoot 'target\uv-cache'
New-Item -ItemType Directory -Force -Path $env:UV_CACHE_DIR | Out-Null

# `uv` may not be on PATH directly; fall back to `mise which uv`, the
# same fallback run_test_cmd uses for the test-time `uv run` calls.
$UvBin = 'uv'
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    $UvBin = (mise which uv).Trim()
}

# 1. Build the shared baml_bridge wheel once. `sdks/python/.venv` supplies
#    maturin at a stable path, keeping pyo3 fingerprints stable.
$SdkPyVenv = Join-Path $SdkPy '.venv'
$MaturinExe = Join-Path $SdkPyVenv 'Scripts\maturin.exe'
$WheelDir = Join-Path $WorkspaceRoot 'target\wheels'
# Must exist before the first uv command when CI exports UV_FIND_LINKS.
New-Item -ItemType Directory -Force -Path $WheelDir | Out-Null
Write-Host "==> uv sync (dev) in $SdkPy"
Push-Location $SdkPy
try {
    $env:UV_PROJECT_ENVIRONMENT = $SdkPyVenv
    & $UvBin sync --group dev --no-install-project
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
Write-Host '==> maturin build (shared baml_bridge wheel)'
Push-Location $SdkPy
try {
    $env:VIRTUAL_ENV = $SdkPyVenv
    & $MaturinExe build --out $WheelDir
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# 2. Per fixture: install deps + the prebuilt baml_bridge wheel. Strip the
#    editable `[tool.uv.sources] baml_bridge` block so baml_bridge resolves
#    as a normal dependency from UV_FIND_LINKS.
if (-not $env:UV_FIND_LINKS) { $env:UV_FIND_LINKS = $WheelDir }
Get-ChildItem -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName 'generated'
    if (Test-Path $generated) {
        $pyproject = Join-Path $generated 'pyproject.toml'
        $lines = Get-Content $pyproject
        $out = New-Object System.Collections.Generic.List[string]
        $skip = $false
        foreach ($line in $lines) {
            if ($line -match '^\[tool\.uv\.sources\]$') {
                $skip = $true
                continue
            }
            if ($skip) {
                if ($line -match '^\s*$') { $skip = $false }
                continue
            }
            $out.Add($line)
        }
        Set-Content -Path $pyproject -Value $out

        Write-Host "==> uv sync in $($_.Name)/generated"
        Push-Location $generated
        try {
            & $UvBin sync
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
    }
}

# Per-run breadcrumb for the in-test guard. nextest reads $NEXTEST_ENV
# after this script and injects these vars into the matched tests'
# processes - so `setup_guard::ran` (see harness_runner) can prove this
# script ran *this* run. Plain `cargo test` has no $NEXTEST_ENV, so the
# var stays unset and the guard fails with a helpful message. Keep the
# var name in sync with SETUP_ENV_VAR in
# harness_setup/src/python_pydantic2.rs.
if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value 'SDK_TEST_PYTHON_PYDANTIC2_SETUP=1'
}
