#!/bin/sh
# Install a prebuilt, checksum-verified lm-resizer binary. No Rust toolchain needed.
set -eu

version="${LM_RESIZER_VERSION:-0.2.4}"
case "$version" in
  ''|*[!0-9A-Za-z.+-]*) echo "invalid version: $version" >&2; exit 2 ;;
esac

os="$(uname -s)"
arch="$(uname -m)"
case "$os:$arch" in
  Linux:x86_64) platform=linux-x86_64 ;;
  Linux:aarch64|Linux:arm64) platform=linux-aarch64 ;;
  Darwin:x86_64) platform=darwin-x86_64 ;;
  Darwin:arm64) platform=darwin-arm64 ;;
  *) echo "unsupported platform: $os/$arch" >&2; exit 2 ;;
esac

archive="lm-resizer-$version-$platform.tar.gz"
base="${LM_RESIZER_RELEASE_BASE_URL:-https://github.com/phuetz/lm-resizer/releases/download/v$version}"
base="${base%/}"
dest_dir="${LM_RESIZER_INSTALL_DIR:-$HOME/.local/bin}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM

curl -fsSL "$base/$archive" -o "$tmp/$archive"
curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256"
expected=""
named=""
read -r expected named < "$tmp/$archive.sha256" || true
cr="$(printf '\r')"
named="${named%"$cr"}"
if [ -z "$expected" ] || [ -z "$named" ]; then
  echo "invalid checksum file: $tmp/$archive.sha256" >&2
  exit 1
fi
if [ "$named" != "$archive" ]; then
  echo "checksum filename mismatch: $named" >&2
  exit 1
fi
if ! printf '%s\n' "$expected" | grep -Eq '^[0-9a-fA-F]{64}$'; then
  echo "invalid checksum" >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$archive" | cut -d ' ' -f 1)"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp/$archive" | cut -d ' ' -f 1)"
else
  echo "sha256sum or shasum is required" >&2
  exit 1
fi
if [ "$actual" != "$expected" ]; then
  echo "SHA-256 mismatch for $archive" >&2
  exit 1
fi

folder="lm-resizer-$version-$platform"
tar -xzf "$tmp/$archive" -C "$tmp" "$folder/lm-resizer"
reported="$("$tmp/$folder/lm-resizer" --version)"
if [ "$reported" != "lm-resizer $version" ]; then
  echo "archive has unexpected binary version: $reported" >&2
  exit 1
fi
mkdir -p "$dest_dir"
staged="$dest_dir/.lm-resizer.$$.new"
trap 'rm -f "$staged"; rm -rf "$tmp"' EXIT HUP INT TERM
install -m 755 "$tmp/$folder/lm-resizer" "$staged"
mv -f "$staged" "$dest_dir/lm-resizer"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
"$dest_dir/lm-resizer" --version
echo "Installed $dest_dir/lm-resizer"
case ":$PATH:" in
  *":$dest_dir:"*) ;;
  *) echo "Add $dest_dir to PATH to run lm-resizer by name." ;;
esac
