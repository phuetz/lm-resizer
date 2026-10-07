#!/bin/sh
# Build the portable Linux x86_64 binary: static musl, no glibc, no libstdc++.
#
# Why a container: the native `tokenizers` dependency compiles C++ (esaxx-rs)
# and C (oniguruma, SQLite, ring). Ubuntu's `musl-tools` only ships a C
# compiler; Alpine is musl-native and has gcc/g++ whose libstdc++ links
# statically. A binary built on ubuntu-latest needs GLIBC_2.39 and libstdc++.so.6
# and fails on Debian 12, Ubuntu 20.04/22.04 and Alpine.
#
# Usage: scripts/build-linux-static.sh   (needs Docker; works as any user)
# Output: target/static/x86_64-unknown-linux-musl/release/lm-resizer
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"

if [ "${1:-}" != "--inside" ]; then
  command -v docker >/dev/null 2>&1 || { echo "docker is required to build the static Linux binary" >&2; exit 1; }
  image="${LM_RESIZER_STATIC_BUILD_IMAGE:-alpine:3.20}"
  mkdir -p "$root/target/static"
  docker run --rm \
    -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
    -e RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-}" \
    -v "$root":/w "$image" sh /w/scripts/build-linux-static.sh --inside
  exit 0
fi

# --- inside the Alpine container (root, ephemeral) ---
apk add --no-cache build-base curl perl pkgconf >/dev/null
export HOME=/root
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none >/dev/null 2>&1
. "$HOME/.cargo/env"
[ -n "${RUSTUP_TOOLCHAIN:-}" ] || unset RUSTUP_TOOLCHAIN
cd /w
export CARGO_TARGET_DIR=/w/target/static
# Do NOT set +crt-static by hand: on a musl host it also applies to proc-macros,
# which then fail to build. The musl target is static by default with rustup.
export RUSTFLAGS="--remap-path-prefix=/w=/build/lm-resizer --remap-path-prefix=/root/.cargo=/build/cargo --remap-path-prefix=/root/.rustup=/build/rustup"
cargo build --release --locked --target x86_64-unknown-linux-musl
chown -R "$HOST_UID:$HOST_GID" /w/target/static
