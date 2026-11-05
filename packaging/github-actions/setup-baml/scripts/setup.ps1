$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$InstallerUrl = "https://pkg.boundaryml.com/install.ps1"
$Helper = Join-Path $env:GITHUB_ACTION_PATH "lib/wrapper-output.mjs"
$BinDir = Join-Path $env:BAML_HOME "bin"
$Wrapper = Join-Path $BinDir "baml.exe"
$TempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$Temp = Join-Path $TempRoot ("setup-baml-" + [System.Guid]::NewGuid())
$Installer = Join-Path $Temp "install.ps1"

Remove-Item Env:BAML_MANIFEST_BASE_URL -ErrorAction SilentlyContinue
Remove-Item Env:BAML_VERSION -ErrorAction SilentlyContinue

New-Item -ItemType Directory -Path $Temp | Out-Null
try {
  Invoke-WebRequest -Uri $InstallerUrl -OutFile $Installer -UseBasicParsing
  $global:LASTEXITCODE = 0
  & $Installer -WrapperOnly -NoModifyPath -Yes
  if ($LASTEXITCODE -ne 0) {
    throw "setup-baml: official installer failed with exit code $LASTEXITCODE"
  }

  $Requested = if ($env:INPUT_TOOLCHAIN) { $env:INPUT_TOOLCHAIN } else { "" }
  $VersionOverride = ""
  if ($Requested) {
    $Selector = ($Requested | & node $Helper "validate-selector").Trim()
    if ($LASTEXITCODE -ne 0) {
      throw "setup-baml: invalid toolchain input"
    }
    $VersionOverride = $Selector
    $env:BAML_VERSION = $VersionOverride
  } else {
    $ListOutput = (& $Wrapper toolchain list | Out-String)
    if ($LASTEXITCODE -ne 0) {
      throw "setup-baml: the BAML wrapper could not inspect the active selector"
    }
    Write-Host $ListOutput.TrimEnd()
    $Selector = ($ListOutput | & node $Helper "selector").Trim()
    if ($LASTEXITCODE -ne 0) {
      throw "setup-baml: the BAML wrapper could not resolve the active selector"
    }
  }

  & $Wrapper toolchain use $Selector
  if ($LASTEXITCODE -ne 0) {
    throw "setup-baml: BAML toolchain installation failed with exit code $LASTEXITCODE"
  }
  $VersionOutput = (& $Wrapper --version | Out-String)
  if ($LASTEXITCODE -ne 0) {
    throw "setup-baml: the BAML wrapper could not report its version"
  }
  Write-Host $VersionOutput.TrimEnd()
  $Version = ($VersionOutput | & node $Helper "version").Trim()
  if ($LASTEXITCODE -ne 0) {
    throw "setup-baml: the BAML wrapper did not report a concrete version"
  }
  $ToolchainPath = Join-Path $env:BAML_HOME "toolchains/$Version/bin/baml-cli.exe"

  if (-not (Test-Path -LiteralPath $Wrapper -PathType Leaf) -or -not (Test-Path -LiteralPath $ToolchainPath -PathType Leaf)) {
    throw "setup-baml: wrapper or resolved toolchain binary is missing"
  }

  $BinDir | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
  "BAML_HOME=$($env:BAML_HOME)" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
  "BAML_VERSION=$VersionOverride" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
  "BAML_MANIFEST_BASE_URL=" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
  "version=$Version" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
  "path=$Wrapper" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
  "toolchain-path=$ToolchainPath" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
} finally {
  Remove-Item -LiteralPath $Temp -Recurse -Force -ErrorAction SilentlyContinue
}
