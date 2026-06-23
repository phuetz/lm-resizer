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
  compressJson(content: string, query?: string): CompressionReport;
}

export function initLmResizerWasm(input: WebAssembly.Module | BufferSource): Promise<LmResizerWasm>;
