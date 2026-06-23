$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$package = Get-Content (Join-Path $root "Cargo.toml") -Raw
$core = Get-Content (Join-Path $root "crates\lm-resizer-core\Cargo.toml") -Raw
$wasm = Get-Content (Join-Path $root "crates\lm-resizer-wasm\Cargo.toml") -Raw
$npm = Get-Content (Join-Path $root "packages\wasm\package.json") -Raw | ConvertFrom-Json

function Get-CargoVersion($content) {
  if ($content -match '(?m)^version\s*=\s*"([^"]+)"') { return $Matches[1] }
  throw "missing Cargo package version"
}

$dist = Join-Path $root "dist"
$tarballs = @()
if (Test-Path $dist) {
  $tarballs = @(Get-ChildItem $dist -Filter "*.tgz" | Select-Object -ExpandProperty Name)
}
$binaryArchives = @()
if (Test-Path $dist) {
  $binaryArchives = @(Get-ChildItem $dist -File | Where-Object { $_.Extension -in @(".zip", ".gz") } | Select-Object -ExpandProperty Name)
}

$providerFixtures = @(Get-ChildItem (Join-Path $root "fixtures\provider-cache") -Filter "*.json" | Select-Object -ExpandProperty Name)
$checksums = @()
if (Test-Path (Join-Path $dist "SHA256SUMS")) {
  $checksums = @("SHA256SUMS")
}

$evidence = [ordered]@{
  generated_at_utc = (Get-Date).ToUniversalTime().ToString("o")
  version = Get-CargoVersion $package
  cargo_versions = [ordered]@{
    root = Get-CargoVersion $package
    core = Get-CargoVersion $core
    wasm = Get-CargoVersion $wasm
  }
  npm_version = $npm.version
  wasm_tarballs = $tarballs
  binary_archives = $binaryArchives
  provider_cache_fixtures = $providerFixtures
  checksums = $checksums
  release_checklist = "docs/RELEASE.md"
  contribution_docs = @(
    "CONTRIBUTING.md",
    "SECURITY.md"
  )
  github_templates = @(
    ".github/ISSUE_TEMPLATE/provider-fixture.yml",
    ".github/ISSUE_TEMPLATE/custom-filter.yml",
    ".github/dependabot.yml"
  )
  release_checks = @(
    "cargo fmt --check",
    "cargo test --release",
    "cargo check --release",
    "cargo check --release --examples",
    "cargo build --release",
    "scripts/smoke-proxy-preview.ps1",
    "scripts/check-wasm-package.ps1",
    "scripts/publish-wasm.ps1 -DryRun",
    "scripts/generate-checksums.ps1",
    "scripts/check-publish-readiness.ps1"
  )
  publish_commands = [ordered]@{
    dry_run = "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/publish-wasm.ps1 -DryRun"
    real_publish = "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/publish-wasm.ps1"
  }
  publication_requires = @(
    "npm trusted publishing for .github/workflows/publish-wasm.yml or NPM_TOKEN/npm login fallback",
    "release approval for npm publish --access public --provenance"
  )
}

$out = Join-Path $dist "release-evidence.json"
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$evidence | ConvertTo-Json -Depth 5 | Set-Content -Encoding UTF8 $out
Write-Host "Release evidence written to $out"
