//! Serialize complete JSONL frames before append so concurrent writers cannot
//! separate one record's JSON from its newline through formatted I/O writes.
use anyhow::Result;
use serde::Serialize;
use std::{io::Write, path::Path};

pub fn append(path: &Path, record: &impl Serialize) -> Result<()> {
    let mut frame = serde_json::to_vec(record)?;
    frame.push(b'\n');
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?
        .write_all(&frame)?;
    Ok(())
}
