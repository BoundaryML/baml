# Build the engine cdylib that baml_bridge's tests load at run time
# (baml_bridge is dylib-only - the engine is never linked in).
#
# Windows counterpart of build-engine.sh - see that file for the
# rationale. Invoked automatically by `cargo nextest run` via the
# setup-script binding in `baml_language/.config/nextest.toml`.

$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..\..\..')

Write-Host "==> cargo build -p bridge_cffi (engine cdylib for baml_bridge tests)"
cargo build -p bridge_cffi
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
