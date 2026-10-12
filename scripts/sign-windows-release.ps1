param(
  [string]$CertificateThumbprint = $env:LM_RESIZER_SIGN_CERT_THUMBPRINT,
  [string]$CertificatePath = $env:LM_RESIZER_SIGN_CERT_PATH,
  [string]$CertificatePassword = $env:LM_RESIZER_SIGN_CERT_PASSWORD,
  [string]$TimestampUrl = $(if ($env:LM_RESIZER_TIMESTAMP_URL) { $env:LM_RESIZER_TIMESTAMP_URL } else { "http://timestamp.digicert.com" }),
  [switch]$DryRun
)

$ErrorActionPreference = "Stop"

# LM_RESIZER_SIGNED_ARCHIVE_HELPER_START
function Update-LmResizerWindowsArchives {
  param([Parameter(Mandatory = $true)][string]$DistRoot)

  $stages = @(
    Get-ChildItem -Path $DistRoot -Directory -Filter "lm-resizer-*-windows-x86_64" -ErrorAction SilentlyContinue
  )
  if ($stages.Count -eq 0) {
    throw "no packaged Windows staging directory found; run package-release.ps1 before signing"
  }

  foreach ($stage in $stages) {
    $stagedExe = Join-Path $stage.FullName "lm-resizer.exe"
    if (-not (Test-Path -LiteralPath $stagedExe -PathType Leaf)) {
      throw "packaged Windows staging directory is missing lm-resizer.exe: $($stage.FullName)"
    }

    $zip = "$($stage.FullName).zip"
    Remove-Item -LiteralPath $zip -Force -ErrorAction SilentlyContinue
    Compress-Archive -Path (Join-Path $stage.FullName "*") -DestinationPath $zip
    $sha = [Security.Cryptography.SHA256]::Create()
    $inputFile = [IO.File]::OpenRead($zip)
    try {
      $hash = [BitConverter]::ToString($sha.ComputeHash($inputFile)).Replace('-', '').ToLowerInvariant()
    } finally {
      $inputFile.Dispose()
      $sha.Dispose()
    }
    Set-Content -LiteralPath "$zip.sha256" `
      -Value "$hash  $([IO.Path]::GetFileName($zip))" -Encoding ASCII
    Write-Host "Repackaged signed archive $zip"
  }
}
# LM_RESIZER_SIGNED_ARCHIVE_HELPER_END

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
if (-not $signtool -and -not $DryRun) {
  throw "signtool.exe not found. Install Windows SDK or run this from a Developer PowerShell."
}

$targets = @()
$releaseExe = Join-Path $root "target\release\lm-resizer.exe"
if (Test-Path $releaseExe) { $targets += $releaseExe }
$distRoot = Join-Path $root "dist"
if (Test-Path $distRoot) {
  $targets += @(Get-ChildItem -Path $distRoot -Recurse -Filter "lm-resizer.exe" -File | Select-Object -ExpandProperty FullName)
}
$targets = @($targets | Sort-Object -Unique)
if ($targets.Count -eq 0) {
  throw "no Windows release executables found"
}

if (-not $CertificateThumbprint -and -not $CertificatePath -and -not $DryRun) {
  throw "set LM_RESIZER_SIGN_CERT_THUMBPRINT or LM_RESIZER_SIGN_CERT_PATH before signing"
}

foreach ($target in $targets) {
  $args = @("sign", "/fd", "SHA256", "/tr", $TimestampUrl, "/td", "SHA256")
  if ($CertificateThumbprint) {
    $args += @("/sha1", $CertificateThumbprint)
  } elseif ($CertificatePath) {
    $args += @("/f", $CertificatePath)
    if ($CertificatePassword) { $args += @("/p", $CertificatePassword) }
  } else {
    $args += @("/sha1", "<LM_RESIZER_SIGN_CERT_THUMBPRINT>")
  }
  $args += $target

  if ($DryRun) {
    if (-not $signtool) {
      Write-Host "signtool.exe not found; install Windows SDK before real signing"
    }
    Write-Host "signtool.exe $($args -join ' ')"
  } else {
    & $signtool.Source @args
    if ($LASTEXITCODE -ne 0) { throw "signtool failed for $target" }
  }
}
# LM_RESIZER_SIGNING_COMPLETE

if (-not $DryRun) {
  Update-LmResizerWindowsArchives -DistRoot $distRoot
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "generate-checksums.ps1")
}
