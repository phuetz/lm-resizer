& {
$ErrorActionPreference = "Stop"

# LM_RESIZER_PATH_HELPER_START
function Add-LmResizerInstallPath {
  param(
    [Parameter(Mandatory = $true)][string]$Directory,
    [scriptblock]$GetUserPath = { [Environment]::GetEnvironmentVariable("Path", "User") },
    [scriptblock]$SetUserPath = { param($value) [Environment]::SetEnvironmentVariable("Path", $value, "User") }
  )

  # The process PATH can already contain the install directory even when the
  # persistent user PATH does not. Check and update the two scopes separately.
  $userPath = & $GetUserPath
  $userParts = @($userPath -split ';' | Where-Object { $_ })
  if ($userParts -notcontains $Directory) {
    & $SetUserPath "$userPath;$Directory".TrimStart(';')
  }

  $sessionParts = @($env:PATH -split ';' | Where-Object { $_ })
  if ($sessionParts -notcontains $Directory) {
    $env:PATH = "$Directory;$env:PATH"
  }
}
# LM_RESIZER_PATH_HELPER_END

$version = if ($env:LM_RESIZER_VERSION) { $env:LM_RESIZER_VERSION } else { "0.2.6" }
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
      try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$Name" -OutFile $Destination
      } catch {
        $node = Get-Command node -ErrorAction SilentlyContinue
        if (-not $node) { throw "HTTPS download failed. Use Node.js or download the archive and .sha256 on another machine, then set LM_RESIZER_RELEASE_BASE_URL to their local folder. $($_.Exception.Message)" }
        Write-Warning "PowerShell HTTPS failed; retrying with Node.js certificate validation enabled."
        $download = @'
const https = require('https'), fs = require('fs');
const [url, dest] = process.argv.slice(1);
function get(url, remaining) {
  if (!url.startsWith('https://')) throw new Error('Only HTTPS downloads are accepted');
  https.get(url, {rejectUnauthorized: true}, res => {
    if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
      res.resume();
      if (!remaining) throw new Error('Too many redirects');
      return get(new URL(res.headers.location, url).href, remaining - 1);
    }
    if (res.statusCode !== 200) throw new Error('HTTP ' + res.statusCode);
    const file = fs.createWriteStream(dest);
    res.on('error', err => { throw err; });
    file.on('error', err => { throw err; });
    res.pipe(file);
  }).on('error', err => { throw err; });
}
get(url, 5);
'@
        & $node.Source -e $download "$base/$Name" $Destination
        if ($LASTEXITCODE -ne 0) { throw "Node.js HTTPS download failed ($LASTEXITCODE)" }
      }
    }
  }
  Get-ReleaseAsset $archive $zip
  Get-ReleaseAsset "$archive.sha256" $checksumFile
  $checksumLine = (Get-Content -LiteralPath $checksumFile -Raw).Trim()
  if ($checksumLine -notmatch '^([0-9a-fA-F]{64})\s\s(.+)$' -or $Matches[2] -ne $archive) {
    throw "Invalid checksum file for $archive"
  }
  $expected = $Matches[1]
  # Pure .NET: independent of a PSModulePath inherited from PowerShell 7.
  $sha = [Security.Cryptography.SHA256]::Create()
  $inputFile = [IO.File]::OpenRead($zip)
  try {
    $actual = [BitConverter]::ToString($sha.ComputeHash($inputFile)).Replace('-', '')
  } finally {
    $inputFile.Dispose()
    $sha.Dispose()
  }
  if ($actual -ne $expected) { throw "SHA-256 mismatch for $archive" }

  $unpack = Join-Path $tmp "unpacked"
  # Windows PowerShell 5.1: Assembly.Load with a partial name cannot resolve
  # this assembly; Add-Type -AssemblyName goes through the GAC and works.
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [IO.Compression.ZipFile]::ExtractToDirectory($zip, $unpack)
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
    Add-LmResizerInstallPath -Directory $destDir
  }
} finally {
  Remove-Item -LiteralPath $tmp -Recurse -Force
}
}
