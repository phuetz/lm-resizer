#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
version="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"name":"lm-resizer","version":"\([^"]*\)".*/\1/p')"
if [ -z "$version" ]; then
  version="$(grep -m1 '^version = ' "$root/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')"
fi

# LM_RESIZER_BINARY: a binary built elsewhere (the static Linux build, see
# scripts/build-linux-static.sh). Otherwise build the native one.
if [ -n "${LM_RESIZER_BINARY:-}" ]; then
  bin="$LM_RESIZER_BINARY"
else
  node "$root/scripts/build-release-artifact.cjs" native
  bin="$root/target/release/lm-resizer"
fi
if [ ! -x "$bin" ]; then
  printf >&2 '%s\n' "release binary not found: $bin"
  exit 1
fi
# A published Linux archive must run on any x86_64 distribution: refuse a binary
# that needs a dynamic loader (glibc/musl) or shared libraries.
if [ "${LM_RESIZER_REQUIRE_STATIC:-}" = "1" ]; then
  if readelf -lW "$bin" | grep -q 'INTERP' || readelf -d "$bin" 2>/dev/null | grep -q 'NEEDED'; then
    printf >&2 '%s\n' "refusing to package $bin: it is dynamically linked (run scripts/build-linux-static.sh)"
    exit 1
  fi
fi
"$root/scripts/check-wasm-package.sh"
"$root/scripts/release-evidence.sh"

dist="$root/dist"
stage="$dist/lm-resizer-$version-$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
rm -rf "$stage"
mkdir -p "$stage"
cp "$bin" "$stage/lm-resizer"
# Only what an end user needs: the binary, its documentation and the agent skills.
# Development material (workflows, fixtures, examples, build scripts, WASM package) stays in the repository.
for f in LICENSE README.md README.fr.md CHANGELOG.md CONTRIBUTING.md SECURITY.md; do
  cp "$root/$f" "$stage/"
done
mkdir -p "$stage/docs" "$stage/scripts"
for f in FAQ.md FAQ.fr.md KNOWN-MISSES.md KNOWN-MISSES.fr.md CLAUDE_CODEX.md CLI-REFERENCE.md AGENT_HOOKS.md WINDOWS.md TOKEN-STATISTICS.md PROXY-MCP.md lm-resizer-hero.png; do
  cp "$root/docs/$f" "$stage/docs/"
done
cp -R "$root/skills" "$stage/"
cp "$root/scripts/install-grok-skill.sh" "$stage/scripts/"
cp "$root/dist/release-evidence.json" "$stage/"

tarball="$stage.tar.gz"
rm -f "$tarball"
tar -C "$dist" -czf "$tarball" "$(basename "$stage")"
"$root/scripts/release-evidence.sh"
"$root/scripts/generate-checksums.sh"
cp "$root/dist/release-evidence.json" "$stage/"
rm -f "$tarball"
tar -C "$dist" -czf "$tarball" "$(basename "$stage")"
"$root/scripts/generate-checksums.sh"
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$tarball" | sed "s#  $dist/#  #" > "$tarball.sha256"
else
  shasum -a 256 "$tarball" | sed "s#  $dist/#  #" > "$tarball.sha256"
fi
printf '%s\n' "Packaged $tarball"
