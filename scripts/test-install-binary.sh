#!/bin/sh
# End-to-end POSIX install from a release-format archive; never invokes Cargo.
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$root/Cargo.toml" | head -n 1)"
platform="$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
archive="lm-resizer-$version-$platform.tar.gz"
source_dir="${1:-$root/dist}"
[ -f "$source_dir/$archive" ] || { echo "missing $source_dir/$archive" >&2; exit 1; }
[ -f "$source_dir/$archive.sha256" ] || { echo "missing $source_dir/$archive.sha256" >&2; exit 1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
mkdir -p "$tmp/release" "$tmp/bad" "$tmp/wrong-name" "$tmp/wrong-version" "$tmp/home"
cp "$source_dir/$archive" "$source_dir/$archive.sha256" "$tmp/release/"
cp "$source_dir/$archive" "$source_dir/$archive.sha256" "$tmp/bad/"

run_installer() {
  release_dir="$1"
  test_path="${2:-/usr/bin:/bin}"
  env -i HOME="$tmp/home" PATH="$test_path" LM_RESIZER_VERSION="$version" \
    LM_RESIZER_RELEASE_BASE_URL="file://$release_dir" \
    LM_RESIZER_INSTALL_DIR="$tmp/bin" bash -o pipefail -c \
    'curl -fsSL "$1" | sh' _ "file://$root/install.sh"
}

if PATH=/usr/bin:/bin command -v cargo >/dev/null 2>&1; then
  echo "test PATH unexpectedly contains Cargo" >&2
  exit 1
fi
run_installer "$tmp/release"
installed="$tmp/bin/lm-resizer"
[ -x "$installed" ] || { echo "installer did not create an executable" >&2; exit 1; }
"$installed" --version | grep -F "lm-resizer $version" >/dev/null
hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  else
    shasum -a 256 "$1" | cut -d ' ' -f 1
  fi
}
before="$(hash_file "$installed")"

expect_rejected() {
  label="$1"
  release_dir="$2"
  diagnostic="$3"
  if run_installer "$release_dir" >"$tmp/error.log" 2>&1; then
    echo "installer accepted $label" >&2
    exit 1
  fi
  grep -F "$diagnostic" "$tmp/error.log" >/dev/null || {
    cat "$tmp/error.log" >&2
    exit 1
  }
  [ "$(hash_file "$installed")" = "$before" ] || {
    echo "$label changed the installed binary" >&2
    exit 1
  }
}

folder="lm-resizer-$version-$platform"
mkdir -p "$tmp/repacked"
tar -xzf "$tmp/bad/$archive" -C "$tmp/repacked"
# The README statistics link must resolve in the shipped archive.
[ -f "$tmp/repacked/$folder/docs/TOKEN-STATISTICS.md" ] || {
  echo "release archive missing docs/TOKEN-STATISTICS.md" >&2; exit 1;
}
# Exercise the skill installer shipped in the archive, not the checkout copy.
# A release missing skills/grok/lm-resizer must fail before publication.
for target in grok codex; do
  env -i HOME="$tmp/home" PATH=/usr/bin:/bin bash \
    "$tmp/repacked/$folder/scripts/install-grok-skill.sh" \
    --target "$target" --dest "$tmp/skill-$target"
  cmp "$tmp/repacked/$folder/skills/grok/lm-resizer/SKILL.md" \
    "$tmp/skill-$target/SKILL.md"
done
printf 'tampered\n' > "$tmp/repacked/$folder/TAMPERED"
tar -C "$tmp/repacked" -czf "$tmp/bad/$archive" "$folder"
expect_rejected 'a tampered archive' "$tmp/bad" 'SHA-256 mismatch'

cp "$source_dir/$archive" "$tmp/wrong-name/"
printf '%s  %s\n' "$(hash_file "$source_dir/$archive")" 'WRONG.tar.gz' \
  > "$tmp/wrong-name/$archive.sha256"
expect_rejected 'a mismatched sidecar filename' "$tmp/wrong-name" 'checksum filename mismatch'

# A valid, correctly hashed archive with an older binary must be rejected.
older_version=0.2.2
mkdir -p "$tmp/version-repack"
mv "$tmp/repacked/$folder" "$tmp/version-repack/$folder"
cat > "$tmp/version-repack/$folder/lm-resizer" <<EOF
#!/bin/sh
echo "lm-resizer $older_version"
EOF
chmod 755 "$tmp/version-repack/$folder/lm-resizer"
tar -C "$tmp/version-repack" -czf "$tmp/wrong-version/$archive" "$folder"
printf '%s  %s\n' "$(hash_file "$tmp/wrong-version/$archive")" "$archive" \
  > "$tmp/wrong-version/$archive.sha256"
expect_rejected 'a wrong binary version' "$tmp/wrong-version" 'archive has unexpected binary version'

# Cargo can be present elsewhere on PATH; the installer must not call it.
mkdir -p "$tmp/fake-toolchain"
cat > "$tmp/fake-toolchain/cargo" <<EOF
#!/bin/sh
echo called > "$tmp/cargo-was-called"
exit 99
EOF
chmod 755 "$tmp/fake-toolchain/cargo"
run_installer "$tmp/release" "$tmp/fake-toolchain:/usr/bin:/bin" >/dev/null
[ ! -e "$tmp/cargo-was-called" ] || { echo "installer invoked Cargo" >&2; exit 1; }
echo "PASS binary install without Cargo; checksum, filename and version rejection"
