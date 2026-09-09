# @phuetz/lm-resizer

**Query-aware compression of JSON tool output for LLM agents, as a WebAssembly module.**

Coding agents (Claude Code, Codex, MCP servers, your own loops) spend most of their context window on
noisy tool output: huge JSON arrays, repeated schemas, logs. `lm-resizer` shrinks that text before it
reaches the model while keeping what matters, such as errors, file paths and the fields your query asks
for. This package is the WebAssembly build of the Rust core, usable from Node.js (≥ 18) or a browser.

The full toolbox (CLI wrapper around any command, HTTP proxy, MCP server, Claude Code / Codex hooks,
test-runner filters) lives in the Rust binary: see the
[lm-resizer repository](https://github.com/phuetz/lm-resizer#readme) and its
[releases](https://github.com/phuetz/lm-resizer/releases).

## Install

```bash
npm install @phuetz/lm-resizer
```

The package ships the compiled `lm_resizer_wasm.wasm` (about 2.6 MB); nothing to build.

## Usage (Node.js)

```js
import { initLmResizerWasmFromPackage } from "@phuetz/lm-resizer";

const lm = await initLmResizerWasmFromPackage();

const report = lm.compressJson(bigJsonString);          // generic pipeline
const focused = lm.compressJson(bigJsonString, "error"); // keep what relates to "error"

console.log(report.bytes_saved, report.steps_applied);
console.log(report.output); // the compressed text to hand to the model
```

If you want to load the module yourself (browser, custom bundling), pass the bytes or a compiled
`WebAssembly.Module`:

```js
import { initLmResizerWasm } from "@phuetz/lm-resizer";

const bytes = await fetch("/lm_resizer_wasm.wasm").then((r) => r.arrayBuffer());
const lm = await initLmResizerWasm(bytes);
```

## API

- `initLmResizerWasmFromPackage(): Promise<LmResizerWasm>` — Node.js only, loads the bundled wasm.
- `initLmResizerWasm(input: WebAssembly.Module | BufferSource): Promise<LmResizerWasm>`
- `lm.compressJson(content: string, query?: string): CompressionReport`

`CompressionReport` fields: `content_type`, `original_bytes`, `compressed_bytes`, `bytes_saved`,
`steps_applied` (e.g. `["json_offload"]`), `cache_keys`, `output`.

## What it does, and does not do

- Input must be JSON text. Large arrays of similar objects, deep nesting and repeated keys compress
  well (the package's own smoke test: 3 161 → 1 222 bytes). A small or already dense document may come
  back unchanged, with `bytes_saved: 0` and an empty `steps_applied`; that is the honest answer, not an
  error.
- Plain-text and log compression (npm/cargo/pytest output, stack traces, directory listings) is in the
  Rust CLI, not in this wasm build.
- Pure function, no I/O, no network. The wasm module allocates from its own linear memory.

## Building from source

```bash
git clone https://github.com/phuetz/lm-resizer && cd lm-resizer
rustup target add wasm32-unknown-unknown
./scripts/build-wasm.sh        # writes packages/wasm/lm_resizer_wasm.wasm
node packages/wasm/smoke.mjs   # real-execution smoke test
```

## License

Apache-2.0. Source: https://github.com/phuetz/lm-resizer
