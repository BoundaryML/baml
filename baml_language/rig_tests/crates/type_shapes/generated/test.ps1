$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
$env:UV_CACHE_DIR = Join-Path $PSScriptRoot ".uv-cache"
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Error "Error: uv is not installed"
    exit 1
}
$BridgePythonDir = (Resolve-Path "../../../../languages/python/rust/bridge_python").Path
Write-Host "==> uv sync"
uv sync
Write-Host "==> maturin develop (builds bridge_python's PyO3 extension into .venv)"
uv run maturin develop --manifest-path (Join-Path $BridgePythonDir "Cargo.toml")
Write-Host "==> Running Python syntax check..."
$pythonFiles = Get-ChildItem -Recurse -Include *.py,*.pyi | ForEach-Object { $_.FullName }
if ($pythonFiles) {
    foreach ($file in $pythonFiles) {
        uv run python -m py_compile $file
    }
}
Write-Host "==> Running ruff lint..."
uv run ruff check --config pyproject.toml baml_sdk
Write-Host "==> Running pytest..."
uv run pytest -v
Write-Host "==> All checks passed!"
