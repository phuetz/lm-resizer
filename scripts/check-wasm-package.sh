#!/usr/bin/env sh
set -eu

root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
pkg="$root/packages/wasm"

node --check "$pkg/index.js"
"$root/scripts/package-wasm.sh"

# Real execution smoke: instantiate the freshly-built .wasm and run the full
# pipeline (catches runtime bugs a syntax check cannot — empty-query alloc,
# wasm-only panics, minify-vs-real-pipeline regressions).
node "$pkg/smoke.mjs"

json="$(cd "$pkg" && npm pack --dry-run --json)"
PACK_JSON="$json" node -e '
  const pack = JSON.parse(process.env.PACK_JSON);
  const files = new Set(pack[0].files.map((file) => file.path));
  for (const path of ["index.js", "index.d.ts", "README.md", "lm_resizer_wasm.wasm"]) {
    if (!files.has(path)) {
      console.error(`npm package missing required file: ${path}`);
      process.exit(1);
    }
  }
'

printf '%s\n' "WASM npm package preflight passed"
