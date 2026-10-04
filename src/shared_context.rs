//! Named, local handoffs backed by the same SQLite file as CCR. Agents that
//! use the same `--store` can read a compact version and request the original.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize, Debug)]
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
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // Handoffs include originals, so retain durability across power loss.
    conn.pragma_update(None, "synchronous", "FULL")?;

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
    if key.is_empty() {
        bail!("shared key must not be empty");
    }
    if key.chars().any(char::is_control) {
        bail!("shared key must not contain control characters");
    }
    if key.len() > 200 {
        bail!("shared key is {} bytes long; the limit is 200 bytes (accented characters take 2 or more)", key.len());
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
    if let Err(e) = conn.execute(
        "INSERT INTO shared_context(key, compressed, original) VALUES (?1, ?2, ?3)",
        params![key, shared, original],
    ) {
        if let rusqlite::Error::SqliteFailure(err, _) = &e {
            if err.code == rusqlite::ErrorCode::ConstraintViolation {
                bail!("shared key {key:?} already exists; handoffs are never overwritten, choose another name");
            }
        }
        return Err(e).with_context(|| format!("saving shared key {key:?}"));
    }
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
    fn duplicate_key_reports_clear_error_without_sqlite_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("handoffs.sqlite3");
        put(&path, "k1", "orig", "comp").unwrap();
        let err = put(&path, "k1", "orig2", "comp2").unwrap_err();
        let err_str = format!("{err:#}");
        assert!(err_str.contains("already exists"), "err: {err_str}");
        assert!(!err_str.contains("UNIQUE"), "err: {err_str}");
        assert!(!err_str.contains("1555"), "err: {err_str}");
    }

    #[test]
    fn long_key_error_reports_length_in_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("handoffs.sqlite3");

        let err_empty = put(&path, "", "x", "x").unwrap_err();
        assert!(format!("{err_empty:#}").contains("shared key must not be empty"));

        let err_control = put(&path, "bad\nkey", "x", "x").unwrap_err();
        assert!(
            format!("{err_control:#}").contains("shared key must not contain control characters")
        );

        let long_key = "é".repeat(120);
        let err_long = put(&path, &long_key, "x", "x").unwrap_err();
        assert!(format!("{err_long:#}").contains("shared key is 240 bytes long; the limit is 200 bytes (accented characters take 2 or more)"));
    }

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

        // Verify WAL mode was enabled (it's persistent on the file)
        let check_conn = rusqlite::Connection::open(&path).unwrap();
        let journal_mode: String = check_conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
        let synchronous: i64 = super::open(&path)
            .unwrap()
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(synchronous, 2);
    }
}
