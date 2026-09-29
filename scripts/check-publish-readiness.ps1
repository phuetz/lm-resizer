$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$packageJsonPath = Join-Path $root "packages\wasm\package.json"
$workflowPath = Join-Path $root ".github\workflows\publish-wasm.yml"
$evidencePath = Join-Path $root "dist\release-evidence.json"

if (-not (Test-Path $packageJsonPath)) { throw "missing $packageJsonPath" }
if (-not (Test-Path $workflowPath)) { throw "missing $workflowPath" }
if (-not (Test-Path $evidencePath)) { throw "missing $evidencePath; run scripts\check-release.ps1 first" }

$packageJson = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
if ($packageJson.name -ne "@phuetz/lm-resizer") {
  throw "unexpected npm package name: $($packageJson.name)"
}
if (-not $packageJson.version) {
  throw "missing npm package version"
}

$workflow = Get-Content $workflowPath -Raw
foreach ($required in @(
  "environment: npm-production",
  "id-token: write",
  "NPM_TRUSTED_PUBLISHING",
  "NPM_PROVENANCE",
  "scripts/publish-wasm.sh"
)) {
  if (-not $workflow.Contains($required)) {
    throw "publish workflow missing: $required"
  }
}

$evidence = Get-Content $evidencePath -Raw | ConvertFrom-Json
if ($evidence.npm_version -ne $packageJson.version) {
  throw "release evidence npm_version $($evidence.npm_version) does not match package.json $($packageJson.version)"
}
if (-not $evidence.wasm_tarballs -or $evidence.wasm_tarballs.Count -eq 0) {
  throw "release evidence does not list a WASM tarball"
}
if (-not $evidence.release_checks -or -not ($evidence.release_checks -contains "scripts/smoke-proxy-preview.ps1")) {
  throw "release evidence does not list proxy smoke check"
}
$plugin = Get-Content (Join-Path $root ".claude-plugin\plugin.json") -Raw | ConvertFrom-Json
$marketplace = Get-Content (Join-Path $root ".claude-plugin\marketplace.json") -Raw | ConvertFrom-Json
$lmEntry = $marketplace.plugins | Where-Object { $_.name -eq "lm-resizer" } | Select-Object -First 1
if (-not $lmEntry -or $plugin.version -ne $packageJson.version -or $lmEntry.version -ne $packageJson.version) {
  throw "plugin, marketplace and npm package versions do not match"
}
if ($evidence.version -ne $packageJson.version) { throw "release evidence version mismatch" }
foreach ($cargoVersion in @($evidence.cargo_versions.root, $evidence.cargo_versions.core, $evidence.cargo_versions.wasm)) {
  if ($cargoVersion -ne $packageJson.version) { throw "release evidence Cargo versions do not match npm package" }
}
if (-not $evidence.binary_archives -or $evidence.binary_archives.Count -eq 0) {
  throw "release evidence does not list a platform binary archive"
}
$sumsPath = Join-Path $root "dist\SHA256SUMS"
if (-not (Test-Path $sumsPath)) { throw "missing dist/SHA256SUMS" }
$sums = @{}
foreach ($line in Get-Content $sumsPath) {
  if ($line -notmatch '^([0-9a-f]{64})  (.+)$') { throw "invalid checksum line: $line" }
  $sums[$Matches[2]] = $Matches[1]
}
foreach ($artifact in @($evidence.wasm_tarballs) + @($evidence.binary_archives)) {
  if (-not $artifact.Contains($packageJson.version)) { throw "stale artifact version: $artifact" }
  $file = Join-Path $root "dist\$artifact"
  if (-not (Test-Path $file)) { throw "missing release artifact: $artifact" }
  $hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($sums[$artifact] -ne $hash) { throw "missing or invalid SHA-256 for $artifact" }
}

$trustedPublishingConfigured = $env:NPM_TRUSTED_PUBLISHING -eq "1"
$tokenConfigured = -not [string]::IsNullOrWhiteSpace($env:NPM_TOKEN)
$authStatus = if ($trustedPublishingConfigured) {
  "trusted-publishing-env"
} elseif ($tokenConfigured) {
  "npm-token-env"
} else {
  "external-approval-required"
}

$report = [ordered]@{
  package = $packageJson.name
  version = $packageJson.version
  workflow = ".github/workflows/publish-wasm.yml"
  environment = "npm-production"
  evidence = "dist/release-evidence.json"
  tarballs = $evidence.wasm_tarballs
  auth_status = $authStatus
  ready_for_manual_publish_after_approval = $true
  external_requirements = @(
    "Configure npm trusted publishing for the GitHub repository or provide NPM_TOKEN",
    "Protect the npm-production GitHub environment with maintainer approval",
    "Run the Publish WASM Package workflow with matching confirm_package and confirm_version"
  )
}

$report | ConvertTo-Json -Depth 5
