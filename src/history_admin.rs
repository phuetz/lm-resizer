//! Reset counters without touching recovery data; correlate local recovery use.
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{IsTerminal, Write},
    path::Path,
};

pub fn reset(dir: &Path, yes: bool) -> Result<Value> {
    if !yes {
        if !std::io::stdin().is_terminal() {
            bail!("reset requires --yes in non-interactive mode; recovery data is preserved");
        }
        eprint!("Reset local statistics (keep recovery data)? [y/N] ");
        std::io::stderr().flush()?;
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if !matches!(line.trim(), "y" | "Y" | "yes") {
            bail!("reset cancelled");
        }
    }
    let archive = dir.join(format!(
        "statistics-backup-{}-{}",
        crate::unix_timestamp(),
        std::process::id()
    ));
    let names = [
        "exec-history.jsonl",
        "retrieval-feedback.jsonl",
        "proxy-history.jsonl",
    ];
    let existing: Vec<_> = names.into_iter().filter(|n| dir.join(n).exists()).collect();
    if !existing.is_empty() {
        std::fs::create_dir(&archive)?;
    }
    let mut moved = Vec::new();
    for name in existing {
        if let Err(error) = std::fs::rename(dir.join(name), archive.join(name)) {
            for name in &moved {
                let _ = std::fs::rename(archive.join(name), dir.join(name));
            }
            return Err(error.into());
        }
        moved.push(name);
    }
    Ok(
        json!({"reset":true,"archived_files":moved,"backup":if moved.is_empty(){None}else{Some(archive)},"recovery_data_preserved":true}),
    )
}

pub fn read(dir: &Path, name: &str) -> Result<String> {
    match std::fs::read_to_string(dir.join(name)) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}
pub fn recalls(history: &str, feedback: &str) -> Value {
    let mut by_filter = BTreeMap::<String, (usize, usize)>::new();
    let mut keys = BTreeMap::<String, Vec<(u64, String)>>::new();
    for row in history
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        let filter = row["filter"].as_str().unwrap_or("unknown").to_owned();
        let refs = row["recovery_refs"].as_array().cloned().unwrap_or_default();
        if refs.is_empty() {
            continue;
        }
        by_filter.entry(filter.clone()).or_default().0 += 1;
        for key in refs.iter().filter_map(Value::as_str) {
            keys.entry(key.into())
                .or_default()
                .push((row["timestamp_unix"].as_u64().unwrap_or(0), filter.clone()));
        }
    }
    let mut unattributed = 0;
    for row in feedback
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        let key = row["hash"].as_str().unwrap_or("");
        let timestamp = row["timestamp_unix"].as_u64().unwrap_or(0);
        if let Some((_, filter)) = keys.get(key).and_then(|v| {
            v.iter()
                .filter(|(t, _)| *t <= timestamp)
                .max_by_key(|(t, _)| *t)
        }) {
            by_filter.entry(filter.clone()).or_default().1 += 1;
        } else {
            unattributed += 1;
        }
    }
    json!({"by_filter":by_filter.into_iter().map(|(filter,(elisions,recalls))|json!({"filter":filter,"elisions":elisions,"recalls":recalls,"recalls_per_elision":recalls as f64 / elisions.max(1) as f64})).collect::<Vec<_>>(),"unattributed_recalls":unattributed,"method":"recovery references correlated to latest preceding execution; historical records without references remain unattributed"})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recall_references_join_latest_preceding_filter() {
        let h = [
            json!({"timestamp_unix":1,"filter":"git","recovery_refs":["abc"]}),
            json!({"timestamp_unix":3,"filter":"cargo","recovery_refs":["abc"]}),
        ]
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        let f = [
            json!({"timestamp_unix":2,"hash":"abc"}),
            json!({"timestamp_unix":4,"hash":"abc"}),
            json!({"timestamp_unix":4,"hash":"legacy"}),
        ]
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
        let r = recalls(&h, &f);
        assert_eq!(r["unattributed_recalls"], 1);
        for row in r["by_filter"].as_array().unwrap() {
            assert_eq!(row["recalls"], 1);
            assert_eq!(row["elisions"], 1);
        }
    }
    #[test]
    fn reset_archives_only_counters() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "exec-history.jsonl",
            "retrieval-feedback.jsonl",
            "ccr.sqlite3",
            "raw.log",
        ] {
            std::fs::write(root.path().join(name), "original").unwrap();
        }
        let report = reset(root.path(), true).unwrap();
        assert!(!root.path().join("exec-history.jsonl").exists());
        assert!(root.path().join("ccr.sqlite3").exists());
        assert!(root.path().join("raw.log").exists());
        assert_eq!(
            std::fs::read_to_string(
                std::path::Path::new(report["backup"].as_str().unwrap()).join("exec-history.jsonl")
            )
            .unwrap(),
            "original"
        );
    }
}
