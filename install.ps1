& {
$ErrorActionPreference = "Stop"

$version = if ($env:LM_RESIZER_VERSION) { $env:LM_RESIZER_VERSION } else { "0.2.4" }
if ($version -notmatch '^[0-9A-Za-z.+-]+$') { throw "Invalid version: $version" }
if ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne [Runtime.InteropServices.Architecture]::X64) {
  throw "Only Windows x86_64 is supported"
}

$archive = "lm-resizer-$version-windows-x86_64.zip"
$base = if ($env:LM_RESIZER_RELEASE_BASE_URL) {
  $env:LM_RESIZER_RELEASE_BASE_URL.TrimEnd('/')
} else {
  "https://github.com/phuetz/lm-resizer/releases/download/v$version"
}
$destDir = if ($env:LM_RESIZER_INSTALL_DIR) { $env:LM_RESIZER_INSTALL_DIR } else { Join-Path $HOME ".local\bin" }
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("lm-resizer-install-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  $zip = Join-Path $tmp $archive
  $checksumFile = "$zip.sha256"
  function Get-ReleaseAsset {
    param([string]$Name, [string]$Destination)
    if (Test-Path -LiteralPath $base -PathType Container) {
      Copy-Item -LiteralPath (Join-Path $base $Name) -Destination $Destination
    } else {
      Invoke-WebRequest -Uri "$base/$Name" -OutFile $Destination
    }
  }
  Get-ReleaseAsset $archive $zip
  Get-ReleaseAsset "$archive.sha256" $checksumFile
  $checksumLine = (Get-Content -LiteralPath $checksumFile -Raw).Trim()
  if ($checksumLine -notmatch '^([0-9a-fA-F]{64})\s\s(.+)$' -or $Matches[2] -ne $archive) {
    throw "Invalid checksum file for $archive"
  }
  $expected = $Matches[1]
  $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash
  if ($actual -ne $expected) { throw "SHA-256 mismatch for $archive" }

  $unpack = Join-Path $tmp "unpacked"
  Expand-Archive -LiteralPath $zip -DestinationPath $unpack
  $source = Join-Path $unpack "lm-resizer.exe"
  if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Archive is missing lm-resizer.exe" }
  $reported = & $source --version
  if ($reported -ne "lm-resizer $version") { throw "Archive has unexpected binary version: $reported" }
  New-Item -ItemType Directory -Force -Path $destDir | Out-Null
  $staged = Join-Path $destDir (".lm-resizer." + [guid]::NewGuid().ToString("N") + ".exe")
  try {
    Copy-Item -LiteralPath $source -Destination $staged
    Move-Item -LiteralPath $staged -Destination (Join-Path $destDir "lm-resizer.exe") -Force
  } finally {
    if (Test-Path -LiteralPath $staged) { Remove-Item -LiteralPath $staged -Force }
  }
  & (Join-Path $destDir "lm-resizer.exe") --version
  Write-Host "Installed $(Join-Path $destDir 'lm-resizer.exe')"

  if ($env:LM_RESIZER_SKIP_PATH_UPDATE -ne "1") {
    $parts = @($env:PATH -split ';' | Where-Object { $_ })
    if ($parts -notcontains $destDir) {
      $env:PATH = "$destDir;$env:PATH"
      $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
      $userParts = @($userPath -split ';' | Where-Object { $_ })
      if ($userParts -notcontains $destDir) {
        [Environment]::SetEnvironmentVariable("Path", "$userPath;$destDir".TrimStart(';'), "User")
      }
    }
  }
} finally {
  Remove-Item -LiteralPath $tmp -Recurse -Force
}
}
