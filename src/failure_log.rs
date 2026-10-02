//! Observable CLI errors and explicit raw fallbacks. Never infer parser failures
//! from an unchanged output: unchanged output can be a valid filter result.
use anyhow::Result;
use serde_json::{json, Value};
use std::io::Write;

pub fn record(error: &anyhow::Error) -> Result<()> {
    if std::env::var("LM_RESIZER_TRACKING").as_deref() == Ok("0") {
        return Ok(());
    }
    let dir = crate::default_state_dir()?;
    std::fs::create_dir_all(&dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("command-errors.jsonl"))?;
    writeln!(
        file,
        "{}",
        json!({"timestamp_unix":crate::unix_timestamp(),"cwd":std::env::current_dir().ok(),"command":crate::shell_join(&std::env::args().skip(1).collect::<Vec<_>>()),"reason":"cli_error","error":format!("{error:#}")})
    )?;
    Ok(())
}
pub fn report(project: Option<&str>) -> Result<Value> {
    let dir = crate::default_state_dir()?;
    let select = |text: String| {
        text.lines()
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .filter(|r| project.is_none_or(|p| r["cwd"].as_str() == Some(p)))
            .collect::<Vec<_>>()
    };
    let errors = select(crate::history_admin::read(&dir, "command-errors.jsonl")?);
    let fallbacks: Vec<_> = select(crate::history_admin::read(&dir, "exec-history.jsonl")?)
        .into_iter()
        .filter(|r| {
            r["filter"] == "raw_on_failure"
                || r["compression_steps"].as_array().is_some_and(|steps| {
                    steps.iter().any(|s| {
                        s.as_str()
                            .is_some_and(|s| s.starts_with("diagnostic_gate:"))
                    })
                })
        })
        .collect();
    Ok(
        json!({"errors":errors,"raw_fallbacks":fallbacks,"coverage":"CLI processing errors and explicit raw-on-failure/diagnostic-gate fallbacks; silent parser fallback is not inferred from unchanged output"}),
    )
}
