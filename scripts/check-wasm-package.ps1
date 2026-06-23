$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$pkg = Join-Path $root "packages\wasm"

node --check (Join-Path $pkg "index.js")
if ($LASTEXITCODE -ne 0) { throw "node --check failed" }

powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "package-wasm.ps1")
if ($LASTEXITCODE -ne 0) { throw "package-wasm.ps1 failed" }

Push-Location $pkg
try {
  $json = npm pack --dry-run --json
  if ($LASTEXITCODE -ne 0) { throw "npm pack --dry-run failed" }
} finally {
  Pop-Location
}

$pack = $json | ConvertFrom-Json
$files = @($pack[0].files | ForEach-Object { $_.path })
$required = @(
  "index.js",
  "index.d.ts",
  "README.md",
  "lm_resizer_wasm.wasm"
)
foreach ($path in $required) {
  if ($files -notcontains $path) {
    throw "npm package missing required file: $path"
  }
}

Write-Host "WASM npm package preflight passed"
