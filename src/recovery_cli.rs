//! Recovery browsing and line selection; never changes output compression.
use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub struct Options {
    hash: Option<String>,
    #[arg(long)]
    store: Option<PathBuf>,
    /// Return the full stored original (also the default in LM Resizer).
    #[arg(long)]
    full: bool,
    #[arg(long,default_value_t=1,value_parser=clap::value_parser!(u64).range(1..))]
    from: u64,
    #[arg(long)]
    lines: Option<usize>,
    #[arg(long)]
    grep: Option<String>,
    #[arg(long, conflicts_with = "hash")]
    list: bool,
}

fn keys(path: &Path) -> Result<Vec<String>> {
    if !path.exists() {
        return Ok(vec![]);
    }
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut statement=conn.prepare("SELECT hash FROM ccr_entries WHERE created_at + ttl_seconds > ?1 ORDER BY created_at DESC, hash")?;
    let result = statement
        .query_map([crate::unix_timestamp() as i64], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}
pub fn window(
    text: &str,
    from: u64,
    lines: Option<usize>,
    pattern: Option<&str>,
) -> Result<String> {
    let regex = pattern.map(regex::Regex::new).transpose()?;
    if from == 1 && lines.is_none() && regex.is_none() {
        return Ok(text.into());
    }
    Ok(text
        .split_inclusive('\n')
        .skip(usize::try_from(from.saturating_sub(1)).unwrap_or(usize::MAX))
        .filter(|line| regex.as_ref().is_none_or(|r| r.is_match(line)))
        .take(lines.unwrap_or(usize::MAX))
        .collect())
}
pub fn run(opts: Options) -> Result<()> {
    let path = opts.store.unwrap_or(crate::default_store_path()?);
    let hashes = keys(&path)?;
    let tee = crate::list_tee_files()?;
    if opts.list || opts.hash.is_none() {
        println!("{}", json!({"sqlite":hashes,"tee":tee.files}));
        return Ok(());
    }
    let hash = opts.hash.unwrap();
    if hash.is_empty() || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("CCR entry not found: {hash}; recovery reference must be a hexadecimal hash or unique prefix");
    }
    let ccr: Vec<_> = hashes.iter().filter(|key| key.starts_with(&hash)).collect();
    let files: Vec<_> = tee
        .files
        .iter()
        .filter(|f| f.name.starts_with(&hash))
        .collect();
    if ccr.len() + files.len() != 1 {
        bail!(
            "recovery reference {hash} matched {} entries; use --list or a longer prefix",
            ccr.len() + files.len()
        );
    }
    let (key, payload, source) = if let Some(key) = ccr.first() {
        let store = crate::open_store(Some(path))?;
        (
            (*key).clone(),
            store.get(key).context("CCR entry expired or missing")?,
            "cli",
        )
    } else {
        let file = files[0];
        let path = crate::resolve_tee_file(&file.name)?;
        (
            file.name[..12.min(file.name.len())].to_string(),
            std::fs::read_to_string(path)?,
            "tee",
        )
    };
    let output = window(&payload, opts.from, opts.lines, opts.grep.as_deref())?;
    let _ = crate::record_retrieval_feedback(&key, output.len(), source);
    print!("{output}");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_windows_preserve_line_endings_and_final_partial_line() {
        let text = "a\r\nb\r\nc";
        assert_eq!(window(text, 1, None, None).unwrap(), text);
        assert_eq!(window(text, 2, Some(1), None).unwrap(), "b\r\n");
        assert_eq!(window(text, 1, None, Some("b|c")).unwrap(), "b\r\nc");
        assert_eq!(window(text, 99, None, None).unwrap(), "");
        assert!(window(text, 1, None, Some("[")).is_err());
    }
}
