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

json="$(cd "$pkg" && npm pack --dry-run --json 2>/dev/null)"
PACK_JSON="$json" node -e '
  const raw = process.env.PACK_JSON;
  let pack;
  try { pack = JSON.parse(raw); } catch (error) { console.error("npm pack --json did not return JSON:", raw.slice(0, 400)); process.exit(1); }
  const entry = Array.isArray(pack) ? pack[0] : pack;
  if (!entry || !Array.isArray(entry.files)) { console.error("npm pack --json has no files list:", raw.slice(0, 400)); process.exit(1); }
  const files = new Set(entry.files.map((file) => file.path));
  for (const path of ["index.js", "index.d.ts", "README.md", "lm_resizer_wasm.wasm"]) {
    if (!files.has(path)) {
      console.error(`npm package missing required file: ${path}`);
      process.exit(1);
    }
  }
'

printf '%s\n' "WASM npm package preflight passed"
