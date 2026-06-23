param(
  [switch]$DryRun
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "package-wasm.ps1")
if ($LASTEXITCODE -ne 0) { throw "package-wasm.ps1 failed" }

$tempNpmrc = $null
if ($DryRun) {
  Write-Host "Running npm publish dry-run; authentication is not required"
} elseif ($env:NPM_TOKEN) {
  $tempNpmrc = Join-Path $env:TEMP "lm-resizer-npm-$PID.npmrc"
  "//registry.npmjs.org/:_authToken=$env:NPM_TOKEN" | Set-Content -Path $tempNpmrc -NoNewline
  $env:NPM_CONFIG_USERCONFIG = $tempNpmrc
} elseif ($env:NPM_TRUSTED_PUBLISHING -eq "1") {
  Write-Host "Using npm trusted publishing / OIDC"
} else {
  npm whoami | Out-Null
  if ($LASTEXITCODE -ne 0) {
    throw "npm is not authenticated. Set NPM_TOKEN, enable NPM_TRUSTED_PUBLISHING=1, or run npm login."
  }
}

Push-Location (Join-Path $root "packages\wasm")
try {
  if ($DryRun) {
    npm publish --access public --dry-run
  } else {
    $publishArgs = @("publish", "--access", "public")
    if ($env:NPM_PROVENANCE -eq "1") {
      $publishArgs += "--provenance"
    }
    npm @publishArgs
  }
  if ($LASTEXITCODE -ne 0) { throw "npm publish failed" }
} finally {
  Pop-Location
  if ($tempNpmrc) { Remove-Item -Force $tempNpmrc -ErrorAction SilentlyContinue }
}
