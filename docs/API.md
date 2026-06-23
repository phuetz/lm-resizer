# Rust API

`lm-resizer-core` exposes a small embeddable API for Rust hosts that want the
same default compression behavior as the CLI without spawning a process.

## Compatibility Contract

The public high-level surface is:

- `LmResizer::new()`
- `LmResizer::with_store(...)`
- `LmResizer::compress(...)`
- `CompressionReport`
- `default_pipeline()`

Until the crate reaches `1.0`, additive fields may be added to
`CompressionReport`, but existing field names and meanings should not change
without a changelog entry and a minor-version bump. Removals or semantic breaks
require a major-version bump once the crate is past `1.0`.

## Example

Run the compiled example:

```bash
cargo run --example basic
cargo run --example persistent_store
```

Minimal embedding:

```rust
use lm_resizer_core::LmResizer;

let resizer = LmResizer::new();
let report = resizer.compress(r#"{ "status": "ok" }"#, "current task");

println!("{}", report.output);
println!("saved {} bytes", report.bytes_saved);
```

Use `LmResizer::with_store(...)` when the caller needs shared CCR storage across
multiple compression calls. The default constructor uses in-memory CCR storage,
which is deterministic and process-local.

## Persistent CCR Store

For hosts that need retrieval across process restarts, open a SQLite CCR store
and pass it into `LmResizer::with_store(...)`:

```rust
use std::sync::Arc;

use lm_resizer_core::{
    ccr::{backends::SqliteCcrStore, CcrStore},
    LmResizer,
};

let store = Arc::new(SqliteCcrStore::open("lm-resizer-ccr.sqlite3", 1_800)?);
let resizer = LmResizer::with_store(store.clone());

let report = resizer.compress(tool_output, "current task");

for key in &report.cache_keys {
    if let Some(original) = store.get(key) {
        println!("retrieved {} bytes", original.len());
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

The full runnable sample is [`examples/persistent_store.rs`](../examples/persistent_store.rs).

## Proxy Embedding Smoke Test

The HTTP/proxy surface is owned by the `lm-resizer` binary. Use the release
smoke script when a host or deployment wrapper needs proof that the proxy boots
and returns compressed provider previews:

```bash
cargo build --release
./scripts/smoke-proxy-preview.sh
```

Windows:

```powershell
cargo build --release
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-proxy-preview.ps1
```

The smoke starts `lm-resizer serve`, posts an OpenAI-compatible
`/v1/chat/completions` request without an upstream, verifies the preview
response, then shuts the proxy down.
