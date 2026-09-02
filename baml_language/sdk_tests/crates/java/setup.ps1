# Per-fixture Gradle setup for the sdk_test_java crate — Windows.
# Unix uses the parallel `setup.sh`; keep the two in sync (same steps,
# host shell of each platform).

$ErrorActionPreference = "Stop"

Set-Location $PSScriptRoot  # baml_language\sdk_tests\crates\java

$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path

# Shared Gradle home under target/ — keep in sync with CACHE_SUBDIR /
# CACHE_ENV_VAR in harness_setup/src/java.rs.
$env:GRADLE_USER_HOME = Join-Path $WorkspaceRoot "target\gradle-home"
New-Item -ItemType Directory -Force -Path $env:GRADLE_USER_HOME | Out-Null

# Gradle launcher: prefer PATH, fall back to mise. Store the executable
# and any prefix args separately rather than as one array we later slice:
# PowerShell's range operator counts DOWN when start > end, so `1..0`
# is `1,0` (not empty), and slicing a one-element `@("gradle")` with
# `$g[1..($g.Length-1)]` would NOT drop the launcher from the tail.
$GradleExe = $null
$GradlePrefix = @()
if (Get-Command gradle -ErrorAction SilentlyContinue) {
    $GradleExe = "gradle"
} elseif (Get-Command mise -ErrorAction SilentlyContinue) {
    $GradleExe = "mise"
    $GradlePrefix = @("exec", "--", "gradle")
} else {
    Write-Warning "gradle not found on PATH (or via mise); sdk_test_java tests will fail once un-ignored"
}

# 1. Native bridge library the fixtures load at runtime (Windows:
#    target\debug\bridge_java.dll; BAML_JAVA_BRIDGE_LIB must point at it).
Write-Host "==> cargo build -p bridge_java (native bridge library)"
Push-Location $WorkspaceRoot
try {
    cargo build -p bridge_java
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# 2. baml_bridge runtime jar the fixtures link against.
if ($GradleExe) {
    Write-Host "==> gradle jar (baml_bridge runtime library)"
    $GradleArgs = $GradlePrefix + @("-p", (Join-Path $WorkspaceRoot "sdks\java\baml_bridge"), "jar")
    & $GradleExe @GradleArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

# Per-run breadcrumb for the `setup_guard::ran` test. Keep the var
# name in sync with SETUP_ENV_VAR in harness_setup/src/java.rs.
if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_JAVA_SETUP=1"
}
