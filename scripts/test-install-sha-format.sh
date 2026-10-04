#!/bin/sh
set -eu

tmp="$(mktemp -d)"

cleanup() {
  rm -rf "$tmp"
}
trap cleanup EXIT HUP INT TERM

export LM_RESIZER_VERSION="0.0.1"
export LM_RESIZER_RELEASE_BASE_URL="file://$tmp"
export LM_RESIZER_INSTALL_DIR="$tmp/bin"

os="$(uname -s)"
arch="$(uname -m)"
case "$os:$arch" in
  Linux:x86_64) platform=linux-x86_64 ;;
  Darwin:x86_64) platform=darwin-x86_64 ;;
  Darwin:arm64) platform=darwin-arm64 ;;
  *) echo "unsupported platform: $os/$arch" >&2; exit 0 ;;
esac

archive="lm-resizer-$LM_RESIZER_VERSION-$platform.tar.gz"
folder="lm-resizer-$LM_RESIZER_VERSION-$platform"

mkdir -p "$tmp/$folder"
cat << EOF_BIN > "$tmp/$folder/lm-resizer"
#!/bin/sh
if [ "\$1" = "--version" ]; then
    echo "lm-resizer $LM_RESIZER_VERSION"
else
    echo "dummy"
fi
EOF_BIN
chmod +x "$tmp/$folder/lm-resizer"

tar -czf "$tmp/$archive" -C "$tmp" "$folder"

if command -v sha256sum >/dev/null 2>&1; then
  checksum="$(sha256sum "$tmp/$archive" | cut -d ' ' -f 1)"
elif command -v shasum >/dev/null 2>&1; then
  checksum="$(shasum -a 256 "$tmp/$archive" | cut -d ' ' -f 1)"
else
  echo "sha256sum or shasum is required" >&2
  exit 1
fi

run_test() {
  desc="$1"
  sha_content="$2"
  expect_success="$3"

  printf "%b" "$sha_content" > "$tmp/$archive.sha256"

  set +e
  out="$(env -i HOME="$tmp" PATH="/usr/bin:/bin" LM_RESIZER_VERSION="$LM_RESIZER_VERSION" LM_RESIZER_RELEASE_BASE_URL="$LM_RESIZER_RELEASE_BASE_URL" LM_RESIZER_INSTALL_DIR="$LM_RESIZER_INSTALL_DIR" sh install.sh 2>&1)"
  ret=$?
  set -e

  if [ "$expect_success" = "1" ] && [ $ret -eq 0 ]; then
    echo "OK: $desc"
  elif [ "$expect_success" = "0" ] && [ $ret -ne 0 ]; then
    if printf "%s\n" "$out" | grep -q "invalid checksum file"; then
      echo "OK: $desc (failed correctly)"
    else
      echo "FAIL: $desc (failed with wrong message: $out)"
      exit 1
    fi
  else
    echo "FAIL: $desc (exit code $ret, output: $out)"
    exit 1
  fi
}

run_test "Normal" "$checksum  $archive\n" 1
run_test "No trailing newline" "$checksum  $archive" 1
run_test "CRLF" "$checksum  $archive\r\n" 1
run_test "Empty file" "" 0

echo "All tests passed!"
