//! Reproducible side-by-side benchmark; no Python files belong to this repo.
use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Instant;
use tiktoken_rs::CoreBPE;

const TOOLS: [&str; 3] = ["LM Resizer", "RTK", "Headroom"];

#[derive(Clone, Deserialize)]
struct Case {
    id: String,
    category: String,
    file: String,
    oracle: Vec<String>,
    rtk_filter: Option<String>,
    command: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Row {
    case: String,
    category: String,
    tool: String,
    route: String,
    tokens_before: usize,
    tokens_after: usize,
    saving: f64,
    qualified_saving: f64,
    oracle_retention: f64,
    missing: Vec<String>,
    latency_ms: f64,
    error: Option<String>,
    exit_code: Option<i32>,
    detector_fallback: bool,
    sha256: String,
    normalized_sha256: String,
}

fn bench_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repo_dir() -> PathBuf {
    bench_dir()
        .parent()
        .expect("bench has a parent")
        .to_path_buf()
}

fn qa_dir() -> PathBuf {
    repo_dir().join("target/banc")
}

fn sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

fn count(bpe: &CoreBPE, text: &str) -> usize {
    bpe.encode_ordinary(text).len()
}

fn normalize_output(text: &str, home: &Path, repo: &Path) -> String {
    let result = text
        .replace(&home.display().to_string(), "~")
        .replace(&repo.display().to_string(), "<CHECKOUT>");
    Regex::new(r"/tee/\d{8,}_")
        .expect("valid tee regex")
        .replace_all(&result, "/tee/0000000000_")
        .into_owned()
}

fn setup_shims(qa: &Path) -> Result<PathBuf> {
    let shims = qa.join("shims");
    fs::create_dir_all(&shims)?;
    let replay = "#!/bin/sh\ncat \"$BANC_FIXTURE\"\nexit \"$BANC_EXIT\"\n";
    for name in [
        "cargo",
        "dotnet",
        "npm",
        "pytest",
        "git",
        "docker",
        "psql",
        "journalctl",
    ] {
        let path = shims.join(name);
        fs::write(&path, replay)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
    }
    Ok(shims)
}

fn run_command(
    program: &Path,
    args: &[String],
    input: Option<&str>,
    home: &Path,
    shims: &Path,
    fixture: &Path,
    expected_exit: i32,
    qa: &Path,
) -> Result<(Output, f64)> {
    fs::create_dir_all(home)?;
    let mut paths = vec![shims.to_path_buf()];
    paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    let path = env::join_paths(paths)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(qa)
        .env("HOME", home)
        .env("PATH", path)
        .env("BANC_FIXTURE", fixture)
        .env("BANC_EXIT", expected_exit.to_string())
        .env("XDG_CACHE_HOME", qa.join("cache"))
        .env("XDG_DATA_HOME", qa.join("data"))
        .env("XDG_CONFIG_HOME", qa.join("config"))
        .env("HF_HOME", qa.join("hf"))
        .env("RTK_NO_TELEMETRY", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let start = Instant::now();
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot start {}", program.display()))?;
    if let Some(text) = input {
        child
            .stdin
            .take()
            .context("stdin unavailable")?
            .write_all(text.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    Ok((output, start.elapsed().as_secs_f64() * 1000.0))
}

fn rtk_route(case: &Case, fixture: &Path) -> (Vec<String>, String, bool) {
    if let Some(filter) = &case.rtk_filter {
        return (
            vec!["pipe".into(), "--filter".into(), filter.clone()],
            format!("pipe --filter {filter}"),
            true,
        );
    }
    if case.category == "json" {
        return (
            vec!["json".into(), fixture.display().to_string()],
            "json".into(),
            false,
        );
    }
    if case.category == "code" || case.category == "prose" {
        return (
            vec!["read".into(), fixture.display().to_string()],
            "read".into(),
            false,
        );
    }
    (
        case.command
            .as_deref()
            .unwrap_or("cat")
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        "commande native".into(),
        false,
    )
}

fn staged_fixture(case: &Case, original: &Path, qa: &Path) -> Result<PathBuf> {
    if case.id != "code_python" {
        return Ok(original.to_path_buf());
    }
    let staged = qa.join("fixtures/code_python.py");
    fs::create_dir_all(staged.parent().expect("fixture parent"))?;
    fs::copy(original, &staged)?;
    Ok(staged)
}

fn expected_exit(case: &Case) -> i32 {
    if case.id.ends_with("fail") || ["docker", "psql", "compile_error"].contains(&case.id.as_str())
    {
        1
    } else {
        0
    }
}

fn missing_oracle(case: &Case, output: &str) -> Vec<String> {
    match case.id.as_str() {
        "git_log" => git_log_missing(case, output),
        "json_large" => json_missing(case, output),
        _ => case
            .oracle
            .iter()
            .filter(|fact| !output.contains(fact.as_str()))
            .cloned()
            .collect(),
    }
}

fn git_log_missing(case: &Case, output: &str) -> Vec<String> {
    let mut missing = Vec::new();
    if !output.contains("Fix overflow in invoice totals") {
        missing.push(case.oracle[0].clone());
    }
    let source = fs::read_to_string(bench_dir().join(&case.file)).unwrap_or_default();
    let hashes: Vec<&str> = Regex::new(r"(?m)^commit ([0-9a-f]{40})$")
        .unwrap()
        .captures_iter(&source)
        .map(|capture| capture.get(1).unwrap().as_str())
        .collect();
    let wanted = case.oracle.get(1).map(String::as_str).unwrap_or_default();
    let sha = Regex::new(r"(?m)^commit ([0-9a-f]{7,40})\b").unwrap();
    let unique = sha.captures_iter(output).any(|capture| {
        let abbreviation = capture.get(1).unwrap().as_str();
        wanted.starts_with(abbreviation)
            && hashes
                .iter()
                .filter(|hash| hash.starts_with(abbreviation))
                .count()
                == 1
    });
    if !unique {
        missing.push("SHA du commit ciblé, abrégé de façon unique".into());
    }
    for index in 0..45 {
        let pattern =
            Regex::new(&format!(r"Maintenance batch {index}\b")).expect("valid subject regex");
        if !pattern.is_match(output) {
            missing.push(format!("Maintenance batch {index}"));
        }
    }
    missing
}

fn json_missing(case: &Case, output: &str) -> Vec<String> {
    let mut found = [false; 9];
    if let Ok(value) = serde_json::from_str::<Value>(output) {
        found[0] = value["schema"] == "orders-v3";
        found[1] = value["count"] == 180;
        if let Some(rows) = value["rows"].as_array() {
            let get = |id: i64| rows.iter().find(|row| row["id"].as_i64() == Some(id));
            let anomaly = get(143);
            found[2] = rows.len() == 180 && anomaly.is_some();
            found[3] = anomaly.is_some_and(|row| row["state"] == "rejected");
            found[4] = anomaly.is_some_and(|row| row["amount"] == 4299);
            found[5] = anomaly.is_some_and(|row| row["meta"]["reason"] == "limit_exceeded");
            for (slot, id) in [(6, 0), (7, 89), (8, 179)] {
                found[slot] = get(id).is_some_and(|row| row["amount"] == id * 3);
            }
        } else if let Some(rows) = value["rows"].as_str() {
            let lines: Vec<&str> = rows.lines().collect();
            let table =
                lines.first().is_some_and(|line| line.contains("[180]")) && lines.len() == 181;
            let anomaly = lines
                .iter()
                .find(|line| line.starts_with("4299,143,"))
                .copied()
                .unwrap_or("");
            found[2] = table && !anomaly.is_empty();
            found[3] = anomaly.contains("rejected");
            found[4] = !anomaly.is_empty();
            found[5] = anomaly.contains("limit_exceeded");
            for (slot, id) in [(6, 0), (7, 89), (8, 179)] {
                found[slot] = lines
                    .iter()
                    .any(|line| line.starts_with(&format!("{},{id},", id * 3)));
            }
        }
    } else if output.trim_start().starts_with('{') {
        for (slot, expression) in [
            (0, r#"(?m)^\s*schema:\s*"orders-v3""#),
            (1, r"(?m)^\s*count:\s*180\b"),
            (2, r"(?m)^\s*id:\s*143\b"),
            (3, r#"(?m)^\s*state:\s*"rejected""#),
            (4, r"(?m)^\s*amount:\s*4299\b"),
            (5, r#"(?m)^\s*reason:\s*"limit_exceeded""#),
            (6, r"(?m)^\s*id:\s*0\b"),
            (7, r"(?m)^\s*id:\s*89\b"),
            (8, r"(?m)^\s*id:\s*179\b"),
        ] {
            found[slot] = Regex::new(expression).unwrap().is_match(output);
        }
    }
    case.oracle
        .iter()
        .zip(found)
        .filter_map(|(fact, present)| (!present).then_some(fact.clone()))
        .collect()
}

fn qualified_winner(rows: &[&Row]) -> String {
    let best = rows
        .iter()
        .filter(|row| row.oracle_retention == 1.0 && row.error.is_none())
        .map(|row| row.qualified_saving)
        .fold(0.0_f64, f64::max);
    if best <= 0.0 {
        return "aucun gain qualifié".into();
    }
    rows.iter()
        .filter(|row| {
            row.oracle_retention == 1.0
                && row.error.is_none()
                && (row.qualified_saving - best).abs() < 1e-12
        })
        .map(|row| row.tool.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))?;
    Ok(())
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

fn verify_home(lm_bin: &Path, shims: &Path, qa: &Path, bpe: &CoreBPE) -> Result<()> {
    let fixture = bench_dir().join("corpus/cargo_ok.txt");
    let args = vec![
        "exec".into(),
        "--store".into(),
        qa.join("lm-ccr.sqlite").display().to_string(),
        "--".into(),
        "cargo".into(),
        "test".into(),
    ];
    let mut views = Vec::new();
    for home in [qa.join("home-lm-resizer"), qa.join("h")] {
        let (output, _) = run_command(lm_bin, &args, None, &home, shims, &fixture, 0, qa)?;
        if !output.status.success() {
            bail!("HOME proof: lm-resizer exec failed");
        }
        let raw = String::from_utf8_lossy(&output.stdout);
        let normalized = normalize_output(&raw, &home, &repo_dir());
        views.push(json!({"raw_tokens": count(bpe, &raw),
                          "normalized_tokens": count(bpe, &normalized),
                          "normalized_sha256": sha256(&normalized)}));
    }
    let equal = views[0]["normalized_sha256"] == views[1]["normalized_sha256"];
    write_json(
        &bench_dir().join("preuve_home.json"),
        &json!({
            "long_home": views[0], "short_home": views[1], "normalized_equal": equal
        }),
    )?;
    if !equal {
        bail!("HOME proof: normalized outputs differ");
    }
    Ok(())
}

fn run_all(cases: &[Case], lm_bin: &Path, qa: &Path, bpe: &CoreBPE) -> Result<Vec<Row>> {
    let shims = setup_shims(qa)?;
    verify_home(lm_bin, &shims, qa, bpe)?;
    let result_dir = qa.join("results");
    fs::create_dir_all(&result_dir)?;
    let rtk = qa.join("rtk/rtk");
    let headroom = bench_dir().join("headroom_once.sh");
    let python = qa.join("venv/bin/python");
    let mut rows = Vec::new();
    for case in cases {
        let source = bench_dir().join(&case.file);
        let fixture = staged_fixture(case, &source, qa)?;
        let raw =
            fs::read_to_string(&source).with_context(|| format!("reading {}", source.display()))?;
        let before = count(bpe, &raw);
        let argv: Vec<String> = case
            .command
            .as_deref()
            .unwrap_or("cat")
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let is_command = argv.first().is_some_and(|word| word != "cat");
        for tool in TOOLS {
            let home = qa.join(format!("home-{}", tool.to_lowercase().replace(' ', "-")));
            let (program, args, input, route): (&Path, Vec<String>, Option<&str>, String) =
                match tool {
                    "LM Resizer" if is_command => {
                        let mut args = vec![
                            "exec".into(),
                            "--store".into(),
                            qa.join("lm-ccr.sqlite").display().to_string(),
                            "--".into(),
                        ];
                        args.extend(argv.iter().cloned());
                        (lm_bin, args, None, "exec".into())
                    }
                    "LM Resizer" => (
                        lm_bin,
                        vec![
                            "compress".into(),
                            "--input".into(),
                            fixture.display().to_string(),
                            "--store".into(),
                            qa.join("lm-ccr.sqlite").display().to_string(),
                        ],
                        None,
                        "compress --input".into(),
                    ),
                    "RTK" => {
                        let (args, route, stdin) = rtk_route(case, &fixture);
                        (&rtk, args, stdin.then_some(raw.as_str()), route)
                    }
                    _ => (
                        &headroom,
                        vec![python.display().to_string()],
                        Some(raw.as_str()),
                        "API Python".into(),
                    ),
                };
            let (output, latency_ms) = run_command(
                program,
                &args,
                input,
                &home,
                &shims,
                &fixture,
                expected_exit(case),
                qa,
            )?;
            let raw_out = String::from_utf8_lossy(&output.stdout).into_owned();
            let normalized = normalize_output(&raw_out, &home, &repo_dir());
            let after = count(bpe, &normalized);
            let error = if ![0, expected_exit(case)].contains(&output.status.code().unwrap_or(-1)) {
                Some(format!(
                    "code de sortie inattendu {:?}",
                    output.status.code()
                ))
            } else if normalized.trim().is_empty() {
                Some("sortie vide".into())
            } else {
                None
            };
            let missing = missing_oracle(case, &normalized);
            let retention = if error.is_some() {
                0.0
            } else {
                (case.oracle.len() - missing.len()) as f64 / case.oracle.len() as f64
            };
            let saving = 1.0 - after as f64 / before as f64;
            let qualified = if retention == 1.0 && error.is_none() {
                saving
            } else {
                0.0
            };
            let slug = format!("{}-{}", case.id, tool.to_lowercase().replace(' ', "-"));
            fs::write(result_dir.join(format!("{slug}.txt")), &raw_out)?;
            fs::write(
                result_dir.join(format!("{slug}.normalized.txt")),
                &normalized,
            )?;
            rows.push(Row {
                case: case.id.clone(),
                category: case.category.clone(),
                tool: tool.into(),
                route,
                tokens_before: before,
                tokens_after: after,
                saving,
                qualified_saving: qualified,
                oracle_retention: retention,
                missing,
                latency_ms,
                error,
                exit_code: output.status.code(),
                detector_fallback: String::from_utf8_lossy(&output.stderr)
                    .contains("pure-Python detection"),
                sha256: sha256(&raw_out),
                normalized_sha256: sha256(&normalized),
            });
            println!(
                "{:<17} {:<11} {:>5}->{:<5} oracle={:>3.0}%",
                case.id,
                tool,
                before,
                after,
                retention * 100.0
            );
        }
    }
    write_json(&result_dir.join("results.json"), &rows)?;
    write_json(&bench_dir().join("resultats.json"), &rows)?;
    Ok(rows)
}

fn row_for<'a>(rows: &'a [Row], case: &str, tool: &str) -> &'a Row {
    rows.iter()
        .find(|row| row.case == case && row.tool == tool)
        .expect("complete result matrix")
}

fn render_report(cases: &[Case], rows: &[Row], lm_bin: &Path, qa: &Path) -> Result<String> {
    let commit = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_dir())
        .output()?;
    let commit = String::from_utf8_lossy(&commit.stdout).trim().to_string();
    let binary_hash = format!("{:x}", Sha256::digest(fs::read(lm_bin)?));
    let mut report = format!("# Banc comparatif LM Resizer / RTK / Headroom\n\nAucune supériorité globale démontrée. Une économie ne compte que si l'oracle est intégralement conservé.\n\n## Versions et méthode\n\n- Checkout `{commit}` ; SHA-256 du binaire LM Resizer `{binary_hash}`.\n- RTK `0.50.0`, Headroom `0.39.1` avec ONNX Runtime `1.24.4`.\n- Tokenizer commun : `tiktoken-rs` `o200k_base` ; mêmes octets de fixture pour tous.\n- RTK suit la route déclarée dans `cases.json` ; LM Resizer utilise `exec` ou `compress --input` ; Headroom utilise son API `compress(messages)`.\n- Les chemins HOME/checkout et horodatages tee sont normalisés pour le comptage ; les originaux restent sous `target/banc/results/`.\n- Latence : processus complet, démarrage Python de Headroom compris.\n");
    if let Ok(proof) =
        serde_json::from_str::<Value>(&fs::read_to_string(bench_dir().join("preuve_home.json"))?)
    {
        report.push_str(&format!("- Preuve HOME : {} contre {} jetons bruts, {} contre {} normalisés, empreintes égales : {}.\n",
            proof["long_home"]["raw_tokens"], proof["short_home"]["raw_tokens"],
            proof["long_home"]["normalized_tokens"], proof["short_home"]["normalized_tokens"],
            proof["normalized_equal"]));
    }
    let fallback = rows
        .iter()
        .filter(|row| row.tool == "Headroom" && row.detector_fallback)
        .count();
    report.push_str(&format!("- Replis Headroom vers la détection Python : {fallback}/22. Détails des 66 mesures : `bench/resultats.json`.\n"));
    report.push_str("\n## Résultats par catégorie\n\n| Catégorie | Cas | LM Resizer : médiane / oracle | RTK : médiane / oracle | Headroom : médiane / oracle |\n|---|---:|---:|---:|---:|\n");
    let categories: BTreeSet<&str> = cases.iter().map(|case| case.category.as_str()).collect();
    for category in categories.iter().copied().chain(std::iter::once("GLOBAL")) {
        let subset: Vec<&Row> = rows
            .iter()
            .filter(|row| category == "GLOBAL" || row.category == category)
            .collect();
        report.push_str(&format!("| {category} | {} |", subset.len() / 3));
        for tool in TOOLS {
            let group: Vec<&Row> = subset
                .iter()
                .copied()
                .filter(|row| row.tool == tool)
                .collect();
            let q = median(group.iter().map(|row| row.qualified_saving).collect()) * 100.0;
            let o = group.iter().map(|row| row.oracle_retention).sum::<f64>() / group.len() as f64
                * 100.0;
            report.push_str(&format!(" {q:.1} % / {o:.1} % |"));
        }
        report.push('\n');
    }
    report.push_str("\n## Latence et échecs\n\n| Outil | Médiane | Maximum | Échecs techniques |\n|---|---:|---:|---:|\n");
    for tool in TOOLS {
        let group: Vec<&Row> = rows.iter().filter(|row| row.tool == tool).collect();
        let median_ms = median(group.iter().map(|row| row.latency_ms).collect());
        let max_ms = group
            .iter()
            .map(|row| row.latency_ms)
            .fold(0.0_f64, f64::max);
        let failures = group.iter().filter(|row| row.error.is_some()).count();
        report.push_str(&format!(
            "| {tool} | {median_ms:.0} ms | {max_ms:.0} ms | {failures} |\n"
        ));
    }
    report.push_str("\n## Gagnants et pertes par cas\n\n| Cas | Gagnant qualifié | LM Resizer | RTK | Headroom |\n|---|---|---:|---:|---:|\n");
    for case in cases {
        let trio: Vec<&Row> = TOOLS
            .iter()
            .map(|tool| row_for(rows, &case.id, tool))
            .collect();
        report.push_str(&format!("| {} | {} |", case.id, qualified_winner(&trio)));
        for row in trio {
            report.push_str(&format!(
                " {:.0} % / {:.0} %{} |",
                row.saving * 100.0,
                row.oracle_retention * 100.0,
                if row.error.is_some() { " ⚠" } else { "" }
            ));
        }
        report.push('\n');
    }
    report.push_str("\n## Faits perdus et corrections LM Resizer\n\n");
    for row in rows
        .iter()
        .filter(|row| !row.missing.is_empty() || row.error.is_some())
    {
        report.push_str(&format!(
            "- `{}` / {} : {} ; manquent {}.\n",
            row.case,
            row.tool,
            row.error.as_deref().unwrap_or("oracle incomplet"),
            row.missing.join(", ")
        ));
    }
    report.push_str("\nPour dépasser RTK et Headroom, LM Resizer doit corriger ses pertes d'oracle et dépasser les économies qualifiées suivantes :\n\n");
    for case in cases {
        let lm = row_for(rows, &case.id, "LM Resizer");
        let other = [
            row_for(rows, &case.id, "RTK"),
            row_for(rows, &case.id, "Headroom"),
        ];
        let best = other
            .into_iter()
            .max_by(|a, b| a.qualified_saving.total_cmp(&b.qualified_saving))
            .unwrap();
        if lm.oracle_retention < 1.0 {
            report.push_str(&format!(
                "- `{}` : garder {}.\n",
                case.id,
                lm.missing.join(", ")
            ));
        } else if lm.qualified_saving < best.qualified_saving {
            report.push_str(&format!(
                "- `{}` : dépasser {} ({:.1} %), avec oracle complet.\n",
                case.id,
                best.tool,
                best.qualified_saving * 100.0
            ));
        } else if lm.saving < 0.0 {
            report.push_str(&format!(
                "- `{}` : éliminer la croissance de sortie ({:.1} %).\n",
                case.id,
                lm.saving * 100.0
            ));
        }
    }
    report.push_str("\n## Sources officielles\n\n- [RTK 0.50.0](https://github.com/rtk-ai/rtk/releases/tag/v0.50.0).\n- [Headroom, installation et API](https://github.com/headroomlabs-ai/headroom/blob/main/README.md).\n\n## Ce que je n'ai pas pu vérifier\n\n- Réparation d'un test par agent : le sandbox du Codex imbriqué bloque l'essai avant modification.\n- Coûts facturés, cache fournisseur et intégrations proxy/CCR en production.\n- Informations utiles au-delà des oracles déclarés et généralisation à des sorties réelles non présentes dans ces 22 fixtures synthétiques.\n");
    let _ = qa;
    Ok(report)
}

fn main() -> Result<()> {
    let bench = bench_dir();
    let qa = qa_dir();
    fs::create_dir_all(&qa)?;
    let mut report = qa.join("RAPPORT.md");
    let mut lm_bin = qa.join("cargo-target/release/lm-resizer");
    let mut report_only = false;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--report" => report = PathBuf::from(args.next().context("--report needs path")?),
            "--lm-bin" => lm_bin = PathBuf::from(args.next().context("--lm-bin needs path")?),
            "--report-only" => report_only = true,
            _ => bail!("unknown argument: {arg}"),
        }
    }
    if let Some(parent) = report.parent() {
        fs::create_dir_all(parent)?;
    }
    let cases: Vec<Case> = serde_json::from_str(&fs::read_to_string(bench.join("cases.json"))?)?;
    let rows = if report_only {
        serde_json::from_str(&fs::read_to_string(bench.join("resultats.json"))?)?
    } else {
        let bpe = tiktoken_rs::o200k_base()?;
        run_all(&cases, &lm_bin, &qa, &bpe)?
    };
    fs::write(&report, render_report(&cases, &rows, &lm_bin, &qa)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases() -> Vec<Case> {
        serde_json::from_str(&fs::read_to_string(bench_dir().join("cases.json")).unwrap()).unwrap()
    }

    #[test]
    fn originals_preserve_every_declared_fact() {
        for case in cases() {
            let original = fs::read_to_string(bench_dir().join(&case.file)).unwrap();
            assert!(!case.oracle.is_empty(), "{}", case.id);
            assert!(missing_oracle(&case, &original).is_empty(), "{}", case.id);
        }
    }

    #[test]
    fn git_oracle_requires_each_subject_and_unique_target() {
        let case = cases()
            .into_iter()
            .find(|case| case.id == "git_log")
            .unwrap();
        let original = fs::read_to_string(bench_dir().join(&case.file)).unwrap();
        assert!(git_log_missing(
            &case,
            &original.replace("Maintenance batch 1\n", "Maintenance batch 10\n")
        )
        .contains(&"Maintenance batch 1".to_string()));
        assert!(git_log_missing(
            &case,
            &original.replace(
                "commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "commit aaaaaa"
            )
        )
        .iter()
        .any(|fact| fact.starts_with("SHA du commit")));
        assert!(git_log_missing(
            &case,
            &original.replace(
                "commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "commit 0000000"
            )
        )
        .iter()
        .any(|fact| fact.starts_with("SHA du commit")));
    }

    #[test]
    fn json_oracle_rejects_wrong_anomaly_and_missing_witness() {
        let case = cases()
            .into_iter()
            .find(|case| case.id == "json_large")
            .unwrap();
        let mut original: Value =
            serde_json::from_str(&fs::read_to_string(bench_dir().join(&case.file)).unwrap())
                .unwrap();
        original["rows"][143]["amount"] = json!(4300);
        original["rows"][89]["amount"] = json!(0);
        let missing = json_missing(&case, &original.to_string());
        assert!(missing.contains(&"\"amount\": 4299".to_string()));
        assert!(missing.contains(&"\"id\": 89".to_string()));
    }

    #[test]
    fn route_and_winner_invariants() {
        let cases = cases();
        let cargo = cases.iter().find(|case| case.id == "cargo_ok").unwrap();
        assert_eq!(
            rtk_route(cargo, Path::new("input")).0,
            ["pipe", "--filter", "cargo-test"]
        );
        let make_row = |tool: &str| Row {
            case: "x".into(),
            category: "tests".into(),
            tool: tool.into(),
            route: String::new(),
            tokens_before: 10,
            tokens_after: 10,
            saving: 0.0,
            qualified_saving: 0.0,
            oracle_retention: 1.0,
            missing: vec![],
            latency_ms: 0.0,
            error: None,
            exit_code: Some(0),
            detector_fallback: false,
            sha256: String::new(),
            normalized_sha256: String::new(),
        };
        let rows = [
            make_row("LM Resizer"),
            make_row("RTK"),
            make_row("Headroom"),
        ];
        assert_eq!(
            qualified_winner(&rows.iter().collect::<Vec<_>>()),
            "aucun gain qualifié"
        );
    }

    #[test]
    fn normalizing_home_keeps_token_count_stable() {
        let bpe = tiktoken_rs::o200k_base().unwrap();
        let a = normalize_output(
            "/long/home/tee/1234567890_out.txt",
            Path::new("/long/home"),
            Path::new("/repo"),
        );
        let b = normalize_output(
            "/h/tee/9999999999_out.txt",
            Path::new("/h"),
            Path::new("/repo"),
        );
        assert_eq!(a, b);
        assert_eq!(count(&bpe, &a), count(&bpe, &b));
    }
}
