# @lm-resizer/wasm

Minimal JavaScript wrapper around the lm-resizer WASM ABI.

Build the WASM module:

```bash
rustup target add wasm32-unknown-unknown
./scripts/build-wasm.sh
# Windows:
powershell -File scripts/build-wasm.ps1
```

The build script copies `lm_resizer_wasm.wasm` next to this package's
`index.js`. To create a local npm tarball under `dist`, run
`scripts/package-wasm.sh` or `scripts/package-wasm.ps1`.

```js
import { readFile } from "node:fs/promises";
import { initLmResizerWasm } from "@lm-resizer/wasm";

const bytes = await readFile("lm_resizer_wasm.wasm");
const lm = await initLmResizerWasm(bytes);
const report = lm.compressJson('{ "a": 1 }');
console.log(report.output);
```
