#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
dist="$root/dist"
[ -d "$dist" ] || { printf >&2 '%s\n' "missing dist directory: $dist"; exit 1; }

out="$dist/SHA256SUMS"
: > "$out"
found=0
for file in "$dist"/*.zip "$dist"/*.tar.gz "$dist"/*.tgz "$dist"/*.exe "$dist"/*.wasm; do
  [ -f "$file" ] || continue
  found=1
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | sed "s#  $dist/#  #" >> "$out"
  else
    shasum -a 256 "$file" | sed "s#  $dist/#  #" >> "$out"
  fi
done

[ "$found" -eq 1 ] || { printf >&2 '%s\n' "no release artifacts found under $dist"; exit 1; }
printf '%s\n' "Checksums written to $out"
