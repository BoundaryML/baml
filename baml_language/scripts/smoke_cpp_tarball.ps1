# Smoke-test a baml-cpp tarball on Windows exactly as a user would consume it:
# extract, compile a program with MSVC against include/ and lib/ only, run it,
# and check that version() round-trips through the C ABI.
#
# Usage: smoke_cpp_tarball.ps1 -Tarball <path> [-MsvcArch amd64|arm64]
# BAML_EXPECTED_VERSION, when set, must match the printed version exactly.
param(
    [Parameter(Mandatory = $true)][string]$Tarball,
    [string]$MsvcArch = "amd64"
)
$ErrorActionPreference = "Stop"

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("baml-cpp-smoke-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
try {
    tar -xzf $Tarball -C $work
    if ($LASTEXITCODE -ne 0) { throw "tar extraction failed" }
    $root = (Get-ChildItem -Directory $work | Select-Object -First 1).FullName

    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $vsPath = & $vswhere -latest -products * -property installationPath
    if (-not $vsPath) { throw "no Visual Studio installation found via vswhere" }
    Import-Module (Join-Path $vsPath "Common7\Tools\Microsoft.VisualStudio.DevShell.dll")
    Enter-VsDevShell -VsInstallPath $vsPath -SkipAutomaticLocation `
        -DevCmdArguments "-arch=$MsvcArch -host_arch=$MsvcArch"

    $main = @'
#include <cstdio>
#include <string>

#include "baml_cffi.h"

int main() {
    Buffer buf = version();
    if (buf.ptr == nullptr || buf.len == 0) {
        std::fprintf(stderr, "version() returned an empty buffer\n");
        return 1;
    }
    std::string v(reinterpret_cast<const char*>(buf.ptr), buf.len);
    free_buffer(buf);
    std::printf("%s\n", v.c_str());
    return 0;
}
'@
    Set-Content -Path (Join-Path $work "main.cc") -Value $main

    Push-Location $work
    try {
        # /TP: cl does not recognize .cc as a C++ source extension on its own.
        cl /nologo /std:c++17 /EHsc /TP main.cc /Fesmoke.exe `
            /I "$root\include" /link "$root\lib\bridge_cffi.dll.lib"
        if ($LASTEXITCODE -ne 0) { throw "MSVC compile/link failed" }
        Copy-Item "$root\lib\bridge_cffi.dll" .

        $got = (& .\smoke.exe | Select-Object -First 1).Trim()
        if ($LASTEXITCODE -ne 0) { throw "smoke.exe exited with $LASTEXITCODE" }
    }
    finally {
        Pop-Location
    }

    $want = (Get-Content (Join-Path $root "VERSION") -Raw).Trim()
    if ($got -ne $want) {
        throw "smoke test FAILED: version() printed '$got' but VERSION says '$want'"
    }
    if ($env:BAML_EXPECTED_VERSION -and $got -ne $env:BAML_EXPECTED_VERSION) {
        throw "smoke test FAILED: version() printed '$got' but the release plan expects '$env:BAML_EXPECTED_VERSION'"
    }
    Write-Output "smoke test passed: version $got"
}
finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
