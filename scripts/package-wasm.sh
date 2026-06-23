#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
dist="$root/dist"
mkdir -p "$dist"

"$root/scripts/build-wasm.sh"
if [ ! -f "$root/packages/wasm/lm_resizer_wasm.wasm" ]; then
  printf >&2 '%s\n' "missing WASM artifact: $root/packages/wasm/lm_resizer_wasm.wasm"
  exit 1
fi

cd "$root/packages/wasm"
npm pack --pack-destination "$dist"
printf '%s\n' "WASM npm package artifact written to $dist"
