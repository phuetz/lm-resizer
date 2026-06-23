$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$dist = Join-Path $root "dist"
if (-not (Test-Path $dist)) {
  throw "missing dist directory: $dist"
}

$patterns = @("*.zip", "*.tar.gz", "*.tgz", "*.exe", "*.wasm")
$files = @()
foreach ($pattern in $patterns) {
  $files += @(Get-ChildItem -Path $dist -Filter $pattern -File -ErrorAction SilentlyContinue)
}
$files = @($files | Sort-Object FullName -Unique)
if ($files.Count -eq 0) {
  throw "no release artifacts found under $dist"
}

$out = Join-Path $dist "SHA256SUMS"
Set-Content -Path $out -Value "" -NoNewline -Encoding UTF8
foreach ($file in $files) {
  $hash = (Get-FileHash -Algorithm SHA256 -Path $file.FullName).Hash.ToLowerInvariant()
  Add-Content -Path $out -Value "$hash  $($file.Name)" -Encoding UTF8
}

Write-Host "Checksums written to $out"
