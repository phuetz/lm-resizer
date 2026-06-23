$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifest = Join-Path $root "Cargo.toml"
$metadata = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
$package = $metadata.packages | Where-Object { $_.name -eq "lm-resizer" } | Select-Object -First 1
if (-not $package) { throw "lm-resizer package metadata not found" }

$version = $package.version
$target = Join-Path $root "target\release\lm-resizer.exe"
cargo build --release
if (-not (Test-Path $target)) { throw "release binary not found: $target" }

powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "check-wasm-package.ps1")
if ($LASTEXITCODE -ne 0) { throw "check-wasm-package.ps1 failed" }
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "release-evidence.ps1")
if ($LASTEXITCODE -ne 0) { throw "release-evidence.ps1 failed" }

$dist = Join-Path $root "dist"
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$stage = Join-Path $dist "lm-resizer-$version-windows-x86_64"
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item $target (Join-Path $stage "lm-resizer.exe")
Copy-Item (Join-Path $root "README.md") $stage
Copy-Item (Join-Path $root "CONTRIBUTING.md") $stage
Copy-Item (Join-Path $root "SECURITY.md") $stage
Copy-Item (Join-Path $root ".github") (Join-Path $stage ".github") -Recurse
Copy-Item (Join-Path $root "docs") (Join-Path $stage "docs") -Recurse
Copy-Item (Join-Path $root "examples") (Join-Path $stage "examples") -Recurse
Copy-Item (Join-Path $root "fixtures") (Join-Path $stage "fixtures") -Recurse
Copy-Item (Join-Path $root "include") (Join-Path $stage "include") -Recurse
Copy-Item (Join-Path $root "scripts") (Join-Path $stage "scripts") -Recurse
Copy-Item (Join-Path $root "packages\wasm") (Join-Path $stage "packages\wasm") -Recurse
Copy-Item (Join-Path $root "dist\release-evidence.json") $stage

$zip = "$stage.zip"
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip

powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "release-evidence.ps1")
if ($LASTEXITCODE -ne 0) { throw "release-evidence.ps1 failed" }
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "generate-checksums.ps1")
if ($LASTEXITCODE -ne 0) { throw "generate-checksums.ps1 failed" }
Copy-Item (Join-Path $root "dist\release-evidence.json") $stage -Force
Copy-Item (Join-Path $root "dist\SHA256SUMS") $stage -Force
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "generate-checksums.ps1")
if ($LASTEXITCODE -ne 0) { throw "generate-checksums.ps1 failed" }

Write-Host "Packaged $zip"
