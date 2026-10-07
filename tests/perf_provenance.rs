#[path = "../bench/perf/artifact.rs"]
mod artifact;

#[test]
fn rejects_stale_binary_and_legacy_guessed_path_proof() {
    use sha2::{Digest, Sha256};
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("lm-resizer");
    let proof = dir.path().join("proof.json");
    std::fs::write(&binary, b"CURRENT").unwrap();
    let mut value = serde_json::json!({"cargo_artifact":{"target":"lm-resizer"},"build_command":["cargo","build","--message-format=json-render-diagnostics"],"binary_sha256":format!("{:x}", Sha256::digest(b"CURRENT"))});
    std::fs::write(&proof, value.to_string()).unwrap();
    assert!(artifact::verify(&binary, &proof).is_ok());
    std::fs::write(&binary, b"STALE").unwrap();
    assert!(artifact::verify(&binary, &proof).is_err());
    value["binary_sha256"] = serde_json::json!(format!("{:x}", Sha256::digest(b"STALE")));
    value.as_object_mut().unwrap().remove("cargo_artifact");
    std::fs::write(&proof, value.to_string()).unwrap();
    assert!(artifact::verify(&binary, &proof).is_err());
}
