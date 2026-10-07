$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $root

# dist/ holds generated evidence and checksums only; a tracked copy goes stale.
if (Get-Command git -ErrorAction SilentlyContinue) {
  $trackedDist = git ls-files dist 2>$null
  if ($LASTEXITCODE -eq 0 -and $trackedDist) {
    throw "dist/ must not be tracked by Git (generated files go stale): $trackedDist"
  }
}

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

Invoke-Checked "source install docs" { node scripts/test-source-install-docs.cjs }
Invoke-Checked "release path flags" { node scripts/test-release-paths.cjs }
Invoke-Checked "installer fallback" { node scripts/test-install-fallback.cjs }
Invoke-Checked "cargo fmt --check" { cargo fmt --check }
# `--workspace` is required to test all members since the root Cargo.toml doesn't define `default-members`.
Invoke-Checked "cargo test --workspace --release" { cargo test --workspace --release }
Invoke-Checked "cargo check --release" { cargo check --release }
Invoke-Checked "cargo check --release --examples" { cargo check --release --examples }
Invoke-Checked "cargo build --release" { cargo build --release }
Invoke-Checked "Windows byte contracts" { python bench/real/check_windows_contract.py --binary target/release/lm-resizer.exe --output target/windows-contract.json }
Invoke-Checked "smoke-proxy-preview.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "smoke-proxy-preview.ps1")
}
Invoke-Checked "check-wasm-package.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "check-wasm-package.ps1")
}
Invoke-Checked "publish-wasm.ps1 -DryRun" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "publish-wasm.ps1") -DryRun
}
Invoke-Checked "package-release.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "package-release.ps1")
}
Invoke-Checked "check-publish-readiness.ps1" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "check-publish-readiness.ps1")
}

Write-Host "lm-resizer release check passed"
