#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
target="wasm32-unknown-unknown"
rustup target add "$target"
RUSTFLAGS='--cfg getrandom_backend="wasm_js"' cargo build -p lm-resizer-wasm --release --target "$target"
artifact="$root/target/$target/release/lm_resizer_wasm.wasm"
if [ ! -f "$artifact" ]; then
  printf >&2 '%s\n' "WASM artifact not found: $artifact"
  exit 1
fi
mkdir -p "$root/packages/wasm"
cp "$artifact" "$root/packages/wasm/lm_resizer_wasm.wasm"
printf '%s\n' "WASM package artifact written to packages/wasm/lm_resizer_wasm.wasm"
