const encoder = new TextEncoder();
const decoder = new TextDecoder();

export async function initLmResizerWasm(input) {
  const instance = input instanceof WebAssembly.Module
    ? await WebAssembly.instantiate(input, {})
    : (await WebAssembly.instantiate(input, {})).instance;

  const exports = instance.exports;
  for (const name of [
    "memory",
    "lm_resizer_alloc",
    "lm_resizer_free",
    "lm_resizer_compress_json",
    "lm_resizer_string_free"
  ]) {
    if (!exports[name]) {
      throw new Error(`lm-resizer wasm export missing: ${name}`);
    }
  }

  function memoryBytes() {
    return new Uint8Array(exports.memory.buffer);
  }

  function writeBytes(bytes) {
    const ptr = exports.lm_resizer_alloc(bytes.length);
    if (!ptr) {
      throw new Error("lm_resizer_alloc returned null");
    }
    memoryBytes().set(bytes, ptr);
    return ptr;
  }

  function readCString(ptr) {
    const memory = memoryBytes();
    let end = ptr;
    while (end < memory.length && memory[end] !== 0) {
      end += 1;
    }
    return decoder.decode(memory.subarray(ptr, end));
  }

  function compressJson(content, query = "") {
    const contentBytes = encoder.encode(content);
    const queryBytes = encoder.encode(query);
    const contentPtr = writeBytes(contentBytes);
    const queryPtr = writeBytes(queryBytes);
    let outPtr = 0;
    try {
      outPtr = exports.lm_resizer_compress_json(
        contentPtr,
        contentBytes.length,
        queryPtr,
        queryBytes.length
      );
      if (!outPtr) {
        throw new Error("lm_resizer_compress_json returned null");
      }
      const json = readCString(outPtr);
      const report = JSON.parse(json);
      if (report && report.error) {
        throw new Error(report.error);
      }
      return report;
    } finally {
      if (outPtr) {
        exports.lm_resizer_string_free(outPtr);
      }
      exports.lm_resizer_free(contentPtr, contentBytes.length);
      exports.lm_resizer_free(queryPtr, queryBytes.length);
    }
  }

  return { compressJson };
}
