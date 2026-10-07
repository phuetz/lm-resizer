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

# A binary that cannot start on this machine (wrong libc, missing loader) must
# produce an explanation, not a bare "not found".
mkdir -p "$tmp/unrunnable"
cat > "$tmp/version-repack/$folder/lm-resizer" <<'EOF2'
#!/bin/sh
echo "Error loading shared library libstdc++.so.6: No such file or directory" >&2
exit 127
EOF2
tar -C "$tmp/version-repack" -czf "$tmp/unrunnable/$archive" "$folder"
printf '%s  %s\n' "$(hash_file "$tmp/unrunnable/$archive")" "$archive" \
  > "$tmp/unrunnable/$archive.sha256"
expect_rejected 'a binary that cannot start' "$tmp/unrunnable" 'cannot run on this machine'
grep -F 'libstdc++.so.6' "$tmp/error.log" >/dev/null || { echo "diagnostic lost the loader message" >&2; exit 1; }

# Minimal images have no curl: wget must work, and with neither the installer says so.
mkdir -p "$tmp/tools-wget" "$tmp/tools-none"
# sha256sum (GNU) or shasum (macOS, absent from coreutils-free runners): the installer accepts either.
for t in sh uname mktemp tar gzip cat grep cut sha256sum shasum install mv rm mkdir dirname tr env head cp; do
  p="$(command -v "$t" 2>/dev/null || true)"
  [ -n "$p" ] || continue
  ln -sf "$p" "$tmp/tools-wget/$t"
  ln -sf "$p" "$tmp/tools-none/$t"
done
cat > "$tmp/tools-wget/wget" <<'EOF2'
#!/bin/sh
# wget -q -O <out> <url> (file:// only)
out="$3"; url="$4"
cp "${url#file://}" "$out"
EOF2
chmod 755 "$tmp/tools-wget/wget"
env -i HOME="$tmp/home" PATH="$tmp/tools-wget" LM_RESIZER_VERSION="$version" \
  LM_RESIZER_RELEASE_BASE_URL="file://$tmp/release" LM_RESIZER_INSTALL_DIR="$tmp/bin-wget" \
  "$(command -v sh)" "$root/install.sh" >"$tmp/wget.log" 2>&1 || { cat "$tmp/wget.log" >&2; echo "installer failed with wget only" >&2; exit 1; }
[ -x "$tmp/bin-wget/lm-resizer" ] || { echo "wget-only install produced no binary" >&2; exit 1; }
grep -F 'export PATH=' "$tmp/wget.log" >/dev/null || { echo "installer did not print an export PATH line" >&2; exit 1; }
if env -i HOME="$tmp/home" PATH="$tmp/tools-none" LM_RESIZER_VERSION="$version" \
  LM_RESIZER_RELEASE_BASE_URL="file://$tmp/release" LM_RESIZER_INSTALL_DIR="$tmp/bin-none" \
  "$(command -v sh)" "$root/install.sh" >"$tmp/none.log" 2>&1; then
  echo "installer succeeded without curl or wget" >&2; exit 1
fi
grep -F 'curl or wget is required' "$tmp/none.log" >/dev/null || { cat "$tmp/none.log" >&2; exit 1; }

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
