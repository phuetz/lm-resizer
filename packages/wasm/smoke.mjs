// Real WASM execution smoke test (no mocks).
//
// Instantiates the actual built `lm_resizer_wasm.wasm` through the published
// `index.js` wrapper and runs the full compression pipeline on real inputs.
// This catches bugs that a syntax-only `node --check` cannot — e.g. the
// empty-`query` allocation path and any wasm-only runtime panic in the
// pipeline. Exits non-zero on the first failed assertion.
//
// Run directly (`node packages/wasm/smoke.mjs`) or via
// `scripts/check-wasm-package.sh`.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { initLmResizerWasm } from "./index.js";

const here = dirname(fileURLToPath(import.meta.url));

function assert(cond, message) {
  if (!cond) {
    console.error(`WASM smoke FAILED: ${message}`);
    process.exit(1);
  }
}

const wasmBytes = readFileSync(join(here, "lm_resizer_wasm.wasm"));
const lm = await initLmResizerWasm(wasmBytes);

// Case 1 — a large uniform-schema JSON array with an EMPTY query. This is the
// exact path that used to throw "lm_resizer_alloc returned null" (empty query
// → alloc(0)) and then panic on wasm (Instant::now()). It must run the real
// pipeline (json_offload / SmartCrusher), not the old minify stub.
const rows = Array.from({ length: 60 }, (_, i) => ({
  id: i,
  name: `item-${i}`,
  status: "ok",
  score: 100,
}));
const payload = JSON.stringify(rows);
const report = lm.compressJson(payload, "");

assert(!report.error, `unexpected error: ${report.error}`);
assert(
  report.original_bytes === payload.length,
  `original_bytes ${report.original_bytes} != input ${payload.length}`
);
assert(report.compressed_bytes > 0, "compressed_bytes should be > 0");
assert(
  report.bytes_saved > 0,
  `expected real compression, bytes_saved=${report.bytes_saved}`
);
assert(
  report.output !== payload,
  "output should differ from input (real pipeline, not pass-through)"
);
assert(
  Array.isArray(report.steps_applied) &&
    report.steps_applied.length > 0 &&
    !(report.steps_applied.length === 1 && report.steps_applied[0] === "json_minify"),
  `expected real pipeline steps, got ${JSON.stringify(report.steps_applied)}`
);

// Case 2 — plain text must round-trip without error (and without crashing).
const plain = lm.compressJson("hello world, this is plain prose with no structure", "");
assert(!plain.error, `plain text errored: ${plain.error}`);
assert(typeof plain.output === "string", "plain text output should be a string");

// Case 3 — empty content + empty query must not crash (both alloc(0) paths).
const empty = lm.compressJson("", "");
assert(!empty.error, `empty input errored: ${empty.error}`);

console.log(
  `WASM smoke passed: json_array ${report.original_bytes}B -> ${report.compressed_bytes}B ` +
    `(saved ${report.bytes_saved}, steps ${JSON.stringify(report.steps_applied)})`
);
