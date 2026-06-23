$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null

powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "build-wasm.ps1")
if ($LASTEXITCODE -ne 0) { throw "build-wasm.ps1 failed" }
$wasm = Join-Path $root "packages\wasm\lm_resizer_wasm.wasm"
if (-not (Test-Path $wasm)) { throw "missing WASM artifact: $wasm" }

Push-Location (Join-Path $root "packages\wasm")
try {
  npm pack --pack-destination $dist | Out-Host
  if ($LASTEXITCODE -ne 0) { throw "npm pack failed" }
} finally {
  Pop-Location
}

Write-Host "WASM npm package artifact written to $dist"
