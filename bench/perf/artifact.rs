//! Validate the candidate's Cargo-reported executable, never infer its path.
use anyhow::{ensure, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub fn verify(binary: &Path, proof: &Path) -> Result<Value> {
    let proof: Value = serde_json::from_slice(&fs::read(proof)?)?;
    ensure!(
        proof["cargo_artifact"]["target"] == "lm-resizer",
        "proof lacks the Cargo executable artifact"
    );
    ensure!(
        proof["build_command"].as_array().is_some_and(|args| args
            .iter()
            .any(|arg| arg == "--message-format=json-render-diagnostics")),
        "proof did not select Cargo's reported artifact"
    );
    let digest = format!("{:x}", Sha256::digest(fs::read(binary)?));
    ensure!(
        proof["binary_sha256"] == digest,
        "measured binary differs from Cargo build proof"
    );
    Ok(proof)
}
