//! Unfiltered execution with independent byte-exact stdout/stderr and usage history.
use anyhow::{Context, Result};
use serde_json::json;
use std::{
    io::Write,
    process::{Command, Stdio},
    time::Instant,
};

pub fn run(argv: &[String]) -> Result<i32> {
    let (program, args) = argv.split_first().context("proxy requires a command")?;
    let started = Instant::now();
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .output()
        .with_context(|| format!("execute {program}"))?;
    std::io::stdout().write_all(&output.stdout)?;
    std::io::stdout().flush()?;
    std::io::stderr().write_all(&output.stderr)?;
    std::io::stderr().flush()?;
    let code = crate::child_exit_code(output.status);
    if std::env::var("LM_RESIZER_TRACKING").as_deref() != Ok("0") {
        let mut raw = output.stdout;
        raw.extend_from_slice(&output.stderr);
        let mut row = json!({"timestamp_unix":crate::unix_timestamp(),"cwd":std::env::current_dir()?,"command":crate::shell_join(argv),"exit_code":code,"filter":"proxy","original_bytes":raw.len(),"filtered_bytes":raw.len(),"compressed_bytes":raw.len(),"bytes_saved":0,"duration_ms":started.elapsed().as_millis()});
        if let Ok(text) = std::str::from_utf8(&raw) {
            let tokens = serde_json::to_value(crate::TokenCounts::measure(text, text))?;
            row.as_object_mut()
                .unwrap()
                .extend(tokens.as_object().unwrap().clone());
        } else {
            row["token_count_method"] = json!("unmeasured binary output");
        }
        let dir = crate::default_state_dir()?;
        std::fs::create_dir_all(&dir)?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("exec-history.jsonl"))?;
        writeln!(file, "{row}")?;
    }
    Ok(code)
}
