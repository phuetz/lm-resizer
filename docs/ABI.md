# C And WASM ABI

`lm-resizer-core` exports a minimal C-compatible ABI for non-Rust hosts. The
same exports are usable from WASM runtimes that can read/write linear memory.

## Build

```bash
cargo build -p lm-resizer-core --release
```

The core crate declares:

```toml
crate-type = ["rlib", "cdylib", "staticlib"]
```

The public C header is:

```text
include/lm_resizer.h
```

## Exports

```c
char *lm_resizer_compress_json(
    const unsigned char *content_ptr,
    uintptr_t content_len,
    const unsigned char *query_ptr,
    uintptr_t query_len
);

void lm_resizer_string_free(char *ptr);

unsigned char *lm_resizer_alloc(uintptr_t len);
void lm_resizer_free(unsigned char *ptr, uintptr_t len);
```

`lm_resizer_compress_json` expects UTF-8 bytes and returns a null-terminated
JSON string. On success, the JSON shape matches `CompressionReport`. On failure,
the returned JSON is:

```json
{ "error": "..." }
```

Always release returned strings with `lm_resizer_string_free`.

`lm_resizer_alloc` and `lm_resizer_free` are provided for WASM hosts that need
to allocate input buffers inside lm-resizer memory before calling
`lm_resizer_compress_json`.

## WASM Wrapper

The npm-style wrapper lives in `packages/wasm`.

```bash
./scripts/build-wasm.sh
# Windows:
powershell -File scripts/build-wasm.ps1
```

Then load it from JavaScript:

```js
import { readFile } from "node:fs/promises";
import { initLmResizerWasm } from "./packages/wasm/index.js";

const bytes = await readFile("./packages/wasm/lm_resizer_wasm.wasm");
const lm = await initLmResizerWasm(bytes);
const report = lm.compressJson('{ "a": 1 }');
console.log(report.output);
```
