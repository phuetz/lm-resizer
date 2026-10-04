#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$root"

# dist/ holds generated evidence and checksums only; a tracked copy goes stale
# (v0.2.3 shipped a SHA256SUMS that matched no published archive).
if command -v git >/dev/null 2>&1 && git rev-parse --git-dir >/dev/null 2>&1; then
  tracked="$(git ls-files dist)"
  if [ -n "$tracked" ]; then
    printf >&2 '%s\n' "dist/ must not be tracked by Git (generated files go stale):" "$tracked"
    exit 1
  fi
fi
node scripts/test-source-install-docs.cjs
node scripts/test-release-paths.cjs
node scripts/test-install-fallback.cjs
python3 bench/real/test_source_similarity.py
python3 bench/real/check_source_similarity.py
cargo fmt --check
# `--workspace` is required to test all members since the root Cargo.toml doesn't define `default-members`.
cargo test --workspace --release
cargo check --release
cargo check --release --examples
node scripts/build-release-artifact.cjs native
python3 bench/real/check_native_surface.py target/release/lm-resizer
python3 bench/real/check_windows_contract.py --binary target/release/lm-resizer --output target/windows-contract.json
python3 bench/real/test_large_capture.py
"$root/scripts/smoke-proxy-preview.sh"
"$root/scripts/check-wasm-package.sh"
"$root/scripts/publish-wasm.sh" --dry-run
"$root/scripts/package-release.sh"
"$root/scripts/check-publish-readiness.sh" >/dev/null
"$root/scripts/test-install-grok-skill.sh"
"$root/scripts/check-readme-install.sh"

printf '%s\n' "lm-resizer release check passed"
