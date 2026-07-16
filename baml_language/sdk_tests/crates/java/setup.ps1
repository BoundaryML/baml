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

# Gradle launcher: prefer PATH, fall back to mise.
$Gradle = $null
if (Get-Command gradle -ErrorAction SilentlyContinue) {
    $Gradle = @("gradle")
} elseif (Get-Command mise -ErrorAction SilentlyContinue) {
    $Gradle = @("mise", "exec", "--", "gradle")
} else {
    Write-Warning "gradle not found on PATH (or via mise); sdk_test_java tests will fail once un-ignored"
}

# 1. Native bridge library the fixtures load at runtime (Windows:
#    target\debug\bridge_java.dll; BAML_JAVA_BRIDGE_LIB must point at it).
Write-Host "==> cargo build -p bridge_java (native bridge library)"
Push-Location $WorkspaceRoot
try {
    rustup run 1.93.0 cargo build -p bridge_java
} finally {
    Pop-Location
}

# 2. baml_bridge runtime jar the fixtures link against.
if ($Gradle) {
    Write-Host "==> gradle jar (baml_bridge runtime library)"
    & $Gradle[0] ($Gradle[1..($Gradle.Length - 1)] + @("-p", (Join-Path $WorkspaceRoot "sdks\java\baml_bridge"), "jar"))
}

# Per-run breadcrumb for the `setup_guard::ran` test. Keep the var
# name in sync with SETUP_ENV_VAR in harness_setup/src/java.rs.
if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_JAVA_SETUP=1"
}
