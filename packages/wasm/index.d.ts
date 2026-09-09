export interface CompressionReport {
  content_type: string;
  original_bytes: number;
  compressed_bytes: number;
  bytes_saved: number;
  steps_applied: string[];
  cache_keys: string[];
  output: string;
}

export interface LmResizerWasm {
  /**
   * Compress a JSON document (string). `query` biases retention towards
   * matching keys/values (query-aware compression); leave it empty for the
   * generic pipeline. Returns the compressed text in `output` plus a report.
   */
  compressJson(content: string, query?: string): CompressionReport;
}

export function initLmResizerWasm(input: WebAssembly.Module | BufferSource): Promise<LmResizerWasm>;

/** Node.js only: load the wasm module bundled with this package. */
export function initLmResizerWasmFromPackage(): Promise<LmResizerWasm>;
