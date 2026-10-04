$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$manifest = Join-Path $root "Cargo.toml"
$metadata = cargo metadata --format-version 1 --no-deps | ConvertFrom-Json
$package = $metadata.packages | Where-Object { $_.name -eq "lm-resizer" } | Select-Object -First 1
if (-not $package) { throw "lm-resizer package metadata not found" }

$version = $package.version
$target = Join-Path $root "target\release\lm-resizer.exe"
node (Join-Path $PSScriptRoot "build-release-artifact.cjs") native
if ($LASTEXITCODE -ne 0) { throw "native release build failed" }
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
# Only what an end user needs: the binary, its documentation and the agent skills.
# Development material (workflows, fixtures, examples, build scripts, WASM package) stays in the repository.
foreach ($f in @("LICENSE", "README.md", "README.fr.md", "CHANGELOG.md", "CONTRIBUTING.md", "SECURITY.md")) {
  Copy-Item (Join-Path $root $f) $stage
}
New-Item -ItemType Directory -Force -Path (Join-Path $stage "docs") | Out-Null
foreach ($f in @("FAQ.md", "FAQ.fr.md", "KNOWN-MISSES.md", "KNOWN-MISSES.fr.md", "CLAUDE_CODEX.md", "TOKEN-STATISTICS.md", "PROXY-MCP.md", "lm-resizer-hero.png")) {
  Copy-Item (Join-Path $root "docs\$f") (Join-Path $stage "docs")
}
Copy-Item (Join-Path $root "skills") (Join-Path $stage "skills") -Recurse
New-Item -ItemType Directory -Force -Path (Join-Path $stage "scripts") | Out-Null
Copy-Item (Join-Path $root "scripts\install-grok-skill.sh") (Join-Path $stage "scripts")
Copy-Item (Join-Path $root "dist\release-evidence.json") $stage

$zip = "$stage.zip"
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip

powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "release-evidence.ps1")
if ($LASTEXITCODE -ne 0) { throw "release-evidence.ps1 failed" }
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "generate-checksums.ps1")
if ($LASTEXITCODE -ne 0) { throw "generate-checksums.ps1 failed" }
Copy-Item (Join-Path $root "dist\release-evidence.json") $stage -Force
Remove-Item -Force $zip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip
powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot "generate-checksums.ps1")
if ($LASTEXITCODE -ne 0) { throw "generate-checksums.ps1 failed" }

$hash = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant()
Set-Content -Path "$zip.sha256" -Value "$hash  $([IO.Path]::GetFileName($zip))" -Encoding ASCII

Write-Host "Packaged $zip"
