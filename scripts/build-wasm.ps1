$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$target = "wasm32-unknown-unknown"
rustup target add $target
if ($LASTEXITCODE -ne 0) { throw "rustup target add failed" }
$env:RUSTFLAGS = '--cfg getrandom_backend="wasm_js"'
cargo build -p lm-resizer-wasm --release --target $target
if ($LASTEXITCODE -ne 0) { throw "WASM cargo build failed" }
$artifact = Join-Path $root "target\$target\release\lm_resizer_wasm.wasm"
if (-not (Test-Path $artifact)) { throw "WASM artifact not found: $artifact" }
$packageDir = Join-Path $root "packages\wasm"
New-Item -ItemType Directory -Force -Path $packageDir | Out-Null
Copy-Item $artifact (Join-Path $packageDir "lm_resizer_wasm.wasm")
Write-Host "WASM package artifact written to packages/wasm/lm_resizer_wasm.wasm"
