$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

function Invoke-Checked {
  param(
    [Parameter(Mandatory = $true)]
    [string]$Label,
    [Parameter(Mandatory = $true)]
    [scriptblock]$Command
  )
  & $Command
  if ($LASTEXITCODE -ne 0) { throw "$Label failed" }
}

Invoke-Checked "cargo fmt --check" { cargo fmt --check }
Invoke-Checked "cargo test --release" { cargo test --release }
Invoke-Checked "cargo check --release" { cargo check --release }
Invoke-Checked "cargo check --release --examples" { cargo check --release --examples }
Invoke-Checked "cargo build --release" { cargo build --release }
Invoke-Checked "smoke-proxy-preview.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "smoke-proxy-preview.ps1")
}
Invoke-Checked "check-wasm-package.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "check-wasm-package.ps1")
}
Invoke-Checked "publish-wasm.ps1 -DryRun" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "publish-wasm.ps1") -DryRun
}
Invoke-Checked "release-evidence.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "release-evidence.ps1")
}
Invoke-Checked "generate-checksums.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "generate-checksums.ps1")
}
Invoke-Checked "check-publish-readiness.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "check-publish-readiness.ps1")
}

Write-Host "lm-resizer release check passed"
