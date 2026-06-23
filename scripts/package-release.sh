#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
version="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"name":"lm-resizer","version":"\([^"]*\)".*/\1/p')"
if [ -z "$version" ]; then
  version="$(grep -m1 '^version = ' "$root/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')"
fi

cargo build --release
bin="$root/target/release/lm-resizer"
if [ ! -x "$bin" ]; then
  printf >&2 '%s\n' "release binary not found: $bin"
  exit 1
fi
"$root/scripts/check-wasm-package.sh"
"$root/scripts/release-evidence.sh"

dist="$root/dist"
stage="$dist/lm-resizer-$version-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
rm -rf "$stage"
mkdir -p "$stage"
cp "$bin" "$stage/lm-resizer"
cp "$root/README.md" "$stage/"
cp "$root/CONTRIBUTING.md" "$stage/"
cp "$root/SECURITY.md" "$stage/"
cp -R "$root/.github" "$stage/"
cp -R "$root/docs" "$stage/"
cp -R "$root/examples" "$stage/"
cp -R "$root/fixtures" "$stage/"
cp -R "$root/include" "$stage/"
cp -R "$root/scripts" "$stage/"
mkdir -p "$stage/packages"
cp -R "$root/packages/wasm" "$stage/packages/"
cp "$root/dist/release-evidence.json" "$stage/"

tarball="$stage.tar.gz"
rm -f "$tarball"
tar -C "$dist" -czf "$tarball" "$(basename "$stage")"
"$root/scripts/release-evidence.sh"
"$root/scripts/generate-checksums.sh"
cp "$root/dist/release-evidence.json" "$stage/"
cp "$root/dist/SHA256SUMS" "$stage/"
rm -f "$tarball"
tar -C "$dist" -czf "$tarball" "$(basename "$stage")"
"$root/scripts/generate-checksums.sh"
printf '%s\n' "Packaged $tarball"
