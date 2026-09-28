//! Named, local handoffs backed by the same SQLite file as CCR. Agents that
//! use the same `--store` can read a compact version and request the original.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct ShareReport {
    pub key: String,
    pub original_bytes: usize,
    pub shared_bytes: usize,
}

fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("opening shared store: {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS shared_context (
            key TEXT PRIMARY KEY,
            compressed TEXT NOT NULL,
            original TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (unixepoch())
        );",
    )?;
    Ok(conn)
}

fn check_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 200 || key.chars().any(char::is_control) {
        bail!("shared key must contain 1–200 bytes without control characters");
    }
    Ok(())
}

/// Store a new handoff. Existing names are preserved until deliberately
/// removed by a future explicit operation; accidental overwrites are errors.
pub fn put(path: &Path, key: &str, original: &str, compact: &str) -> Result<ShareReport> {
    check_key(key)?;
    let conn = open(path)?;
    let shared = if compact.len() < original.len() {
        compact
    } else {
        original
    };
    conn.execute(
        "INSERT INTO shared_context(key, compressed, original) VALUES (?1, ?2, ?3)",
        params![key, shared, original],
    )
    .with_context(|| format!("saving shared key {key:?}; it may already exist"))?;
    Ok(ShareReport {
        key: key.to_string(),
        original_bytes: original.len(),
        shared_bytes: shared.len(),
    })
}

pub fn get(path: &Path, key: &str, full: bool) -> Result<Option<String>> {
    check_key(key)?;
    let conn = open(path)?;
    let column = if full { "original" } else { "compressed" };
    let sql = format!("SELECT {column} FROM shared_context WHERE key = ?1");
    Ok(conn.query_row(&sql, [key], |row| row.get(0)).optional()?)
}

pub fn list(path: &Path) -> Result<Vec<String>> {
    let conn = open(path)?;
    let mut statement = conn.prepare("SELECT key FROM shared_context ORDER BY key")?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::{get, list, put};

    #[test]
    fn named_handoff_is_shared_across_connections_and_preserves_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("handoffs.sqlite3");
        let raw = "heading\n".repeat(200);
        let compact = "heading (200 repetitions)\n";
        let report = put(&path, "research", &raw, compact).unwrap();
        assert!(report.shared_bytes < report.original_bytes);
        assert_eq!(
            get(&path, "research", false).unwrap().as_deref(),
            Some(compact)
        );
        assert_eq!(
            get(&path, "research", true).unwrap().as_deref(),
            Some(raw.as_str())
        );
        assert_eq!(list(&path).unwrap(), vec!["research"]);
        assert!(put(&path, "research", "other", "other").is_err());
        assert!(put(&path, "bad\nkey", "x", "x").is_err());
    }
}
