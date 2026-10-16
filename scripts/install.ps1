param(
  [string]$Channel = "canary",
  [string]$Version = "",
  [switch]$WrapperOnly,
  [switch]$NoModifyPath,
  [switch]$Yes,
  [switch]$Help
)

if ($Help) {
  Write-Host "Usage: install.ps1 [-Channel canary|nightly] [-Version VERSION] [-WrapperOnly] [-NoModifyPath] [-Yes]"
  exit 0
}

if ($Channel -ne "canary" -and $Channel -ne "nightly") {
  Write-Error "Channel must be canary or nightly"
  exit 1
}

$ManifestBaseUrl = if ($env:BAML_MANIFEST_BASE_URL) { $env:BAML_MANIFEST_BASE_URL.TrimEnd("/") } else { "https://pkg.boundaryml.com/manifest/v1" }
$BamlHome = if ($env:BAML_HOME) { $env:BAML_HOME } else { Join-Path $env:USERPROFILE ".baml" }
$BinDir = Join-Path $BamlHome "bin"
$WrapperPath = Join-Path $BinDir "baml.exe"

switch ($env:PROCESSOR_ARCHITECTURE) {
  "AMD64" { $Target = "x86_64-pc-windows-msvc" }
  "ARM64" { $Target = "aarch64-pc-windows-msvc" }
  default {
    Write-Error "Unsupported Windows architecture $env:PROCESSOR_ARCHITECTURE"
    exit 2
  }
}

$Temp = Join-Path ([System.IO.Path]::GetTempPath()) ("baml-install-" + [System.Guid]::NewGuid())
New-Item -ItemType Directory -Path $Temp | Out-Null
try {
  $ManifestPath = Join-Path $Temp "wrapper.json"
  try {
    Invoke-WebRequest -Uri "$ManifestBaseUrl/wrapper.json" -OutFile $ManifestPath -UseBasicParsing
  } catch {
    Write-Error "Failed to download wrapper manifest: $_"
    exit 3
  }

  $Manifest = Get-Content $ManifestPath -Raw | ConvertFrom-Json
  $Artifact = $Manifest.artifacts.$Target
  if (-not $Artifact) {
    Write-Error "Wrapper manifest has no artifact for $Target"
    exit 5
  }

  $Archive = Join-Path $Temp "wrapper.zip"
  try {
    Invoke-WebRequest -Uri $Artifact.url -OutFile $Archive -UseBasicParsing
  } catch {
    Write-Error "Failed to download $($Artifact.url): $_"
    exit 3
  }

  $Actual = (Get-FileHash -Algorithm SHA256 -Path $Archive).Hash.ToLowerInvariant()
  if ($Actual -ne $Artifact.sha256.ToLowerInvariant()) {
    Write-Error "Checksum verification failed for wrapper archive"
    exit 4
  }

  $Extract = Join-Path $Temp "extract"
  Expand-Archive -Path $Archive -DestinationPath $Extract -Force
  $ExtractedWrapper = Join-Path $Extract "bin\baml.exe"
  if (-not (Test-Path $ExtractedWrapper)) {
    Write-Error "Wrapper archive must contain bin/baml.exe"
    exit 5
  }
  if ((Test-Path (Join-Path $Extract "bin\baml-cli.exe")) -or (Test-Path (Join-Path $Extract "bin\baml-pack-host.exe")) -or (Test-Path (Join-Path $Extract "assets\baml-vscode.vsix"))) {
    Write-Error "Wrapper archive layout is not safe"
    exit 5
  }

  New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
  $TmpWrapper = Join-Path $BinDir ".baml.exe.tmp"
  Copy-Item $ExtractedWrapper $TmpWrapper -Force
  Move-Item $TmpWrapper $WrapperPath -Force

  if (-not $NoModifyPath -and $Yes) {
    $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
    $Parts = @($UserPath -split ';' | Where-Object { $_ })
    if ($Parts -notcontains $BinDir) {
      [Environment]::SetEnvironmentVariable("Path", (($Parts + $BinDir) -join ';'), "User")
      Write-Host "Updated user PATH. Restart your terminal to pick up baml.exe."
    }
  }

  if (-not $WrapperOnly) {
    $Selector = if ($Version) { $Version } else { $Channel }
    & $WrapperPath toolchain use $Selector
    if ($LASTEXITCODE -ne 0) {
      Write-Error "Wrapper installed, but toolchain bootstrap failed. Retry: $WrapperPath toolchain use $Selector"
      exit 7
    }
  }

  Write-Host "BAML installed at $WrapperPath"
  if ($NoModifyPath -or -not $Yes) {
    $EscapedBinDir = $BinDir.Replace("'", "''")
    Write-Host ""
    Write-Host "Add BAML to your user PATH by running these commands in PowerShell:"
    Write-Host ""
    Write-Host "  `$BamlBin = '$EscapedBinDir'"
    Write-Host '  $UserPath = @([Environment]::GetEnvironmentVariable("Path", "User") -split ";" | Where-Object { $_ })'
    Write-Host '  if ($UserPath -notcontains $BamlBin) {'
    Write-Host '    [Environment]::SetEnvironmentVariable("Path", (($UserPath + $BamlBin) -join ";"), "User")'
    Write-Host '  }'
    Write-Host '  $env:Path = "$BamlBin;$env:Path"'
    Write-Host ""
    Write-Host "The last command also makes baml.exe available in this terminal."
  }
} finally {
  Remove-Item -Path $Temp -Recurse -Force -ErrorAction SilentlyContinue
}
