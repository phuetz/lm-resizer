param([Parameter(Mandatory = $true)][string]$ArchiveDir)
$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifest = Get-Content -LiteralPath (Join-Path $root "Cargo.toml") -Raw
if ($manifest -notmatch '(?m)^version = "([^"]+)"') { throw "Missing package version" }
$version = $Matches[1]
$archive = "lm-resizer-$version-windows-x86_64.zip"
$sourceDir = (Resolve-Path $ArchiveDir).Path
foreach ($name in @($archive, "$archive.sha256")) {
  if (-not (Test-Path -LiteralPath (Join-Path $sourceDir $name))) { throw "Missing $name" }
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("lm-resizer-smoke-" + [guid]::NewGuid().ToString("N"))
$release = Join-Path $tmp "release"
$bad = Join-Path $tmp "bad"
$wrongName = Join-Path $tmp "wrong-name"
$wrongVersion = Join-Path $tmp "wrong-version"
$installDir = Join-Path $tmp "bin"
New-Item -ItemType Directory -Path $release, $bad, $wrongName, $wrongVersion | Out-Null
$savedVersion = $env:LM_RESIZER_VERSION
$savedBase = $env:LM_RESIZER_RELEASE_BASE_URL
$savedDest = $env:LM_RESIZER_INSTALL_DIR
$savedSkip = $env:LM_RESIZER_SKIP_PATH_UPDATE
$savedProcessPath = $env:PATH
$savedUserPath = [Environment]::GetEnvironmentVariable("Path", "User")
try {
  foreach ($name in @($archive, "$archive.sha256")) {
    Copy-Item -LiteralPath (Join-Path $sourceDir $name) -Destination $release
    Copy-Item -LiteralPath (Join-Path $sourceDir $name) -Destination $bad
  }
  $env:LM_RESIZER_VERSION = $version
  $env:LM_RESIZER_RELEASE_BASE_URL = $release
  $env:LM_RESIZER_INSTALL_DIR = $installDir
  $env:LM_RESIZER_SKIP_PATH_UPDATE = "1"
  Get-Content -LiteralPath (Join-Path $root "install.ps1") -Raw | Invoke-Expression
  $binary = Join-Path $installDir "lm-resizer.exe"
  if (-not (Test-Path -LiteralPath $binary)) { throw "Installer did not create the binary" }
  $output = & $binary --version
  if ($output -notmatch [regex]::Escape("lm-resizer $version")) { throw "Wrong installed version: $output" }
  $before = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash

  # Reproduce a PowerShell 5.1 child inheriting an unusable module search path.
  & {
    function Get-FileHash { throw "Get-FileHash must not be required by the installer" }
    $savedModules = $env:PSModulePath
    try {
      $env:PSModulePath = Join-Path $tmp "no modules"
      Get-Content -LiteralPath (Join-Path $root "install.ps1") -Raw | Invoke-Expression
    } finally {
      $env:PSModulePath = $savedModules
    }
  }


  # Exercise the normal one-command PATH update, then restore it in finally.
  $env:LM_RESIZER_SKIP_PATH_UPDATE = "0"
  Get-Content -LiteralPath (Join-Path $root "install.ps1") -Raw | Invoke-Expression
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (@($userPath -split ';') -notcontains $installDir) { throw "User PATH was not updated" }
  if (@($env:PATH -split ';') -notcontains $installDir) { throw "Process PATH was not updated" }
  $env:LM_RESIZER_SKIP_PATH_UPDATE = "1"

  $repacked = Join-Path $tmp "repacked"
  Expand-Archive -LiteralPath (Join-Path $bad $archive) -DestinationPath $repacked
  if (-not (Test-Path -LiteralPath (Join-Path $repacked "docs\TOKEN-STATISTICS.md"))) {
    throw "release archive missing docs/TOKEN-STATISTICS.md"
  }
  Set-Content -LiteralPath (Join-Path $repacked "TAMPERED") -Value "tampered"
  Remove-Item -LiteralPath (Join-Path $bad $archive) -Force
  Compress-Archive -Path (Join-Path $repacked "*") -DestinationPath (Join-Path $bad $archive)
  $env:LM_RESIZER_RELEASE_BASE_URL = $bad
  $rejected = $false
  try { Get-Content -LiteralPath (Join-Path $root "install.ps1") -Raw | Invoke-Expression } catch {
    if ($_.Exception.Message -match 'SHA-256 mismatch') { $rejected = $true } else { throw }
  }
  if (-not $rejected) { throw "Installer accepted a tampered archive" }

  Copy-Item -LiteralPath (Join-Path $sourceDir $archive) -Destination $wrongName
  $originalHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $wrongName $archive)).Hash.ToLowerInvariant()
  Set-Content -LiteralPath (Join-Path $wrongName "$archive.sha256") -Value "$originalHash  WRONG.zip" -Encoding ASCII
  $env:LM_RESIZER_RELEASE_BASE_URL = $wrongName
  $rejected = $false
  try { Get-Content -LiteralPath (Join-Path $root "install.ps1") -Raw | Invoke-Expression } catch {
    if ($_.Exception.Message -match 'Invalid checksum file') { $rejected = $true } else { throw }
  }
  if (-not $rejected) { throw "Installer accepted a mismatched sidecar filename" }

  $otherVersion = "${version}-mismatch"
  $otherArchive = "lm-resizer-$otherVersion-windows-x86_64.zip"
  Copy-Item -LiteralPath (Join-Path $sourceDir $archive) -Destination (Join-Path $wrongVersion $otherArchive)
  Set-Content -LiteralPath (Join-Path $wrongVersion "$otherArchive.sha256") -Value "$originalHash  $otherArchive" -Encoding ASCII
  $env:LM_RESIZER_VERSION = $otherVersion
  $env:LM_RESIZER_RELEASE_BASE_URL = $wrongVersion
  $rejected = $false
  try { Get-Content -LiteralPath (Join-Path $root "install.ps1") -Raw | Invoke-Expression } catch {
    if ($_.Exception.Message -match 'Archive has unexpected binary version') { $rejected = $true } else { throw }
  }
  if (-not $rejected) { throw "Installer accepted a wrong binary version" }

  $after = (Get-FileHash -Algorithm SHA256 -LiteralPath $binary).Hash
  if ($before -ne $after) { throw "Failed installation changed the binary" }
  Write-Host "PASS Windows binary install; checksum, filename, version and PATH checks"
} finally {
  $env:LM_RESIZER_VERSION = $savedVersion
  $env:LM_RESIZER_RELEASE_BASE_URL = $savedBase
  $env:LM_RESIZER_INSTALL_DIR = $savedDest
  $env:LM_RESIZER_SKIP_PATH_UPDATE = $savedSkip
  $env:PATH = $savedProcessPath
  [Environment]::SetEnvironmentVariable("Path", $savedUserPath, "User")
  Remove-Item -LiteralPath $tmp -Recurse -Force
}
