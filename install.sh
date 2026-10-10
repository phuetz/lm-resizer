#!/bin/sh
# Install a prebuilt, checksum-verified lm-resizer binary. No Rust toolchain needed.
set -eu

version="${LM_RESIZER_VERSION:-0.2.6}"
case "$version" in
  ''|*[!0-9A-Za-z.+-]*) echo "invalid version: $version" >&2; exit 2 ;;
esac

os="$(uname -s)"
arch="$(uname -m)"
case "$os:$arch" in
  Linux:x86_64) platform=linux-x86_64 ;;
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

# curl, or wget when curl is absent (minimal Debian/Ubuntu/Alpine images ship
# neither by default). The installer itself only needs POSIX sh, tar and gzip.
fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$1" -o "$2"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    echo "curl or wget is required to download $1" >&2
    echo "Install one first, for example: apt-get install -y curl ca-certificates (Debian/Ubuntu) or apk add curl (Alpine)." >&2
    exit 1
  fi
}
fetch "$base/$archive" "$tmp/$archive" || { echo "download failed: $base/$archive" >&2; exit 1; }
fetch "$base/$archive.sha256" "$tmp/$archive.sha256" || { echo "download failed: $base/$archive.sha256" >&2; exit 1; }
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
status=0
reported="$("$tmp/$folder/lm-resizer" --version 2>"$tmp/run.err")" || status=$?
if [ "$status" -ne 0 ]; then
  echo "the downloaded lm-resizer binary cannot run on this machine (exit $status):" >&2
  cat "$tmp/run.err" >&2
  echo "Linux binaries are built static (musl) and need no glibc, libstdc++ or system library." >&2
  if [ "$os" = "Linux" ]; then
    echo "If you see 'not found' or 'Exec format error': check that this is a 64-bit x86_64 system (uname -m gives $arch)," >&2
    echo "that you did not install an older release built against glibc, and that the file system is not mounted noexec." >&2
  fi
  echo "Alternative: build from source (see the README, section Install from source)." >&2
  exit 1
fi
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
  *) echo "Add $dest_dir to PATH to run lm-resizer by name, for this shell and future ones:"
     echo "  export PATH=\"$dest_dir:\$PATH\"" ;;
esac
