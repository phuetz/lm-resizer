#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"

node scripts/test-source-install-docs.cjs
node scripts/test-release-paths.cjs
cargo fmt --check
# `--workspace` is required to test all members since the root Cargo.toml doesn't define `default-members`.
cargo test --workspace --release
cargo check --release
cargo check --release --examples
cargo build --release
"$root/scripts/smoke-proxy-preview.sh"
"$root/scripts/check-wasm-package.sh"
"$root/scripts/publish-wasm.sh" --dry-run
"$root/scripts/package-release.sh"
"$root/scripts/check-publish-readiness.sh" >/dev/null
"$root/scripts/test-install-grok-skill.sh"
"$root/scripts/check-readme-install.sh"

printf '%s\n' "lm-resizer release check passed"
