//! Serialize complete JSONL frames before append so concurrent writers cannot
//! separate one record's JSON from its newline through formatted I/O writes.
use anyhow::Result;
use fs2::FileExt;
use serde::Serialize;
use std::{io::Write, path::Path};

pub fn append(path: &Path, record: &impl Serialize) -> Result<()> {
    let mut frame = serde_json::to_vec(record)?;
    frame.push(b'\n');
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    // Hold the OS advisory lock across every write_all retry, including short
    // writes. Closing this handle releases it on both success and error.
    file.lock_exclusive()?;
    file.write_all(&frame)?;
    Ok(())
}
