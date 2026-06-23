use std::sync::Arc;

use lm_resizer_core::{
    ccr::{backends::SqliteCcrStore, compute_key, CcrStore},
    LmResizer,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!(
        "lm-resizer-example-ccr-{}.sqlite3",
        std::process::id()
    ));

    let store = Arc::new(SqliteCcrStore::open(&path, 1_800)?);
    let resizer = LmResizer::with_store(store.clone());

    let report = resizer.compress(
        r#"{
  "events": [
    { "level": "info", "message": "starting build" },
    { "level": "info", "message": "starting build" },
    { "level": "error", "message": "compile failed" }
  ]
}"#,
        "summarize build output",
    );

    println!("db={}", path.display());
    println!("steps={}", report.steps_applied.join(","));
    println!("cache_keys={}", report.cache_keys.join(","));

    let payload = "recoverable command output";
    let key = compute_key(payload.as_bytes());
    store.put(&key, payload);
    drop(resizer);
    drop(store);

    let reopened = SqliteCcrStore::open(&path, 1_800)?;
    let recovered = reopened.get(&key).unwrap_or_default();
    println!("recovered={recovered}");

    Ok(())
}
