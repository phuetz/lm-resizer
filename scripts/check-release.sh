#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"

cargo fmt --check
cargo test --release
cargo check --release
cargo check --release --examples
cargo build --release
"$root/scripts/smoke-proxy-preview.sh"
"$root/scripts/check-wasm-package.sh"
"$root/scripts/publish-wasm.sh" --dry-run
"$root/scripts/release-evidence.sh"
"$root/scripts/generate-checksums.sh"
"$root/scripts/check-publish-readiness.sh" >/dev/null

printf '%s\n' "lm-resizer release check passed"
