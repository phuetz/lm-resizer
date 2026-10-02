//! Cold-process latency and byte-exact CLI parity on a pinned public corpus.
use anyhow::{bail, ensure, Context, Result};
use clap::Parser;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Instant,
};

#[path = "artifact.rs"]
mod artifact;
use artifact::{Artifact, Provenance};

const REVISION: &str = "09b1db0617311f3b39f165bb3480c0ce2f718b27";
const URL: &str = "https://github.com/microsoft/TypeScript.git";

#[derive(Parser)]
struct Args {
    /// Verified prebuilt artifact; otherwise build HEAD from its Git archive.
    #[arg(long)]
    binary: Option<PathBuf>,
    /// Build this original commit before measuring, never reuse an old CLI.
    #[arg(long, conflicts_with_all = ["before", "baseline_results"])]
    before_ref: Option<String>,
    /// Build artifacts only, then exit before measuring.
    #[arg(long, conflicts_with = "baseline_results")]
    build_only: bool,
    /// Original release binary for full JSON/stdout/stderr parity.
    #[arg(long, conflicts_with = "baseline_results")]
    before: Option<PathBuf>,
    /// Reuse original JSON results with matching input hashes.
    #[arg(long)]
    baseline_results: Option<PathBuf>,
    #[arg(long)]
    repo: Option<PathBuf>,
    /// New artifact directory: must not exist.
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 3)]
    runs: usize,
}

#[derive(Serialize)]
struct Case {
    name: String,
    command: Vec<String>,
    #[serde(skip)]
    input: PathBuf,
    bytes: usize,
    sha256: String,
    threshold: Option<f64>,
    child_exit: i32,
}

fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn checked(command: &mut Command) -> Result<Output> {
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output)
}

fn add_case(
    cases: &mut Vec<Case>,
    directory: &Path,
    name: &str,
    command: &[&str],
    data: &[u8],
    threshold: Option<f64>,
    child_exit: i32,
) -> Result<()> {
    let input = directory.join(format!("{name}.input"));
    fs::write(&input, data)?;
    cases.push(Case {
        name: name.into(),
        command: command.iter().map(|s| (*s).into()).collect(),
        input,
        bytes: data.len(),
        sha256: digest(data),
        threshold,
        child_exit,
    });
    Ok(())
}

fn prepare(repo: &Path, directory: &Path) -> Result<Vec<Case>> {
    if !repo.exists() {
        checked(
            Command::new("git")
                .args(["clone", "--quiet", "--depth", "1", URL])
                .arg(repo),
        )?;
        checked(
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["fetch", "--quiet", "--depth", "1", "origin", REVISION]),
        )?;
        checked(
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["checkout", "--quiet", "--detach", REVISION]),
        )?;
    }
    let output = checked(
        Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-parse", "HEAD"]),
    )?;
    ensure!(
        String::from_utf8(output.stdout)?.trim() == REVISION,
        "corpus revision mismatch"
    );
    let mut cases = Vec::new();
    for (name, command) in [("ls", vec!["ls", "-R"]), ("find", vec!["find", "."])] {
        let data = checked(
            Command::new(command[0])
                .args(&command[1..])
                .env("LC_ALL", "C")
                .current_dir(repo),
        )?
        .stdout;
        ensure!(!data.is_empty(), "empty public listing");
        add_case(
            &mut cases,
            directory,
            &format!("{name}-real"),
            &command,
            &data,
            Some(2.0),
            0,
        )?;
        let large = data.repeat(5_000_000_usize.div_ceil(data.len()));
        if large != data {
            add_case(
                &mut cases,
                directory,
                &format!("{name}-5mb"),
                &command,
                &large,
                Some(2.0),
                0,
            )?;
        }
        if name == "find" {
            let text = std::str::from_utf8(&data)?;
            let mut end = 3_000_000.min(data.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            add_case(
                &mut cases,
                directory,
                "cat-paths-3mb",
                &["cat", "input.txt"],
                &data[..end],
                Some(0.3),
                0,
            )?;
        }
    }
    let data = fs::read(repo.join("tsc/testdata/fixtures/compiler/checker.ts"))?;
    let text = std::str::from_utf8(&data)?;
    let mut end = 3_000_000.min(data.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    ensure!(end >= 2_999_996, "source fixture is smaller than 3 MB");
    add_case(
        &mut cases,
        directory,
        "cat-3mb",
        &["cat", "input.txt"],
        &data[..end],
        Some(0.3),
        0,
    )?;
    let mut paths = fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("bench/corpus"))?
        .map(|r| r.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    for path in paths {
        let name = path
            .file_stem()
            .context("fixture stem")?
            .to_str()
            .context("UTF-8 stem")?;
        let command = match name {
            "git_diff" => vec!["git", "diff"],
            "git_log" => vec!["git", "log"],
            "docker" => vec!["docker", "ps"],
            "psql" => vec!["psql"],
            _ => match name.split('_').next().unwrap_or("") {
                "cargo" => vec!["cargo", "test"],
                "npm" => vec!["npm", "test"],
                "pytest" => vec!["pytest"],
                "dotnet" => vec!["dotnet", "test"],
                _ => vec!["unknown-command"],
            },
        };
        add_case(
            &mut cases,
            directory,
            &format!("corpus-{name}"),
            &command,
            &fs::read(&path)?,
            None,
            0,
        )?;
    }
    for (name, command, data, code) in [
        (
            "unicode-listing",
            vec!["ls"],
            " a \r\n\té\n中\n\n a \n".as_bytes(),
            0,
        ),
        (
            "lost-diagnostic",
            vec!["ls"],
            b"ERROR: failed\nExpected: 1\nActual: 2\n".as_slice(),
            0,
        ),
        (
            "raw-failure",
            vec!["cargo", "test"],
            b"error: compile failed\n detail\n".as_slice(),
            1,
        ),
        ("empty", vec!["cat"], b"".as_slice(), 0),
        ("no-newline", vec!["cat"], "sans fin é".as_bytes(), 0),
    ] {
        add_case(&mut cases, directory, name, &command, data, None, code)?;
    }
    Ok(cases)
}

#[derive(Clone, Serialize, serde::Deserialize)]
struct Measurement {
    seconds: f64,
    commit: String,
    binary_sha256: String,
    format: String,
}

impl Measurement {
    fn verify_identity(&self, provenance: &Provenance) -> Result<()> {
        ensure!(
            self.commit == provenance.commit
                && self.binary_sha256 == provenance.binary_sha256
                && self.format == "json",
            "baseline measurement provenance mismatch"
        );
        Ok(())
    }
}

fn invoke(
    artifact: &Artifact,
    case: &Case,
    directory: &Path,
    shim: &Path,
    json_mode: bool,
) -> Result<(Output, Measurement)> {
    artifact.verify()?;
    let mut path = vec![shim.to_path_buf()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut command = Command::new(&artifact.binary);
    command.arg("exec");
    if json_mode {
        command.arg("--json");
    }
    command
        .arg("--")
        .args(&case.command)
        .current_dir(directory)
        .env("LC_ALL", "C")
        .env("LM_RESIZER_STATE_DIR", directory.join("state"))
        .env("LM_PERF_INPUT", &case.input)
        .env("LM_PERF_EXIT", case.child_exit.to_string())
        .env("PATH", std::env::join_paths(path)?);
    for name in ["LM_RESIZER_PROFILE", "LM_RESIZER_TEE", "LM_RESIZER_STORE"] {
        command.env_remove(name);
    }
    let started = Instant::now();
    let output = command.output()?;
    let elapsed = started.elapsed().as_secs_f64();
    ensure!(
        output.status.code() == Some(case.child_exit),
        "{}: exit {:?}: {}",
        case.name,
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    artifact.verify()?;
    Ok((
        output,
        Measurement {
            seconds: elapsed,
            commit: artifact.provenance.commit.clone(),
            binary_sha256: artifact.provenance.binary_sha256.clone(),
            format: if json_mode { "json" } else { "text" }.into(),
        },
    ))
}

#[cfg(unix)]
fn create_shims(cases: &[Case], shim: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::create_dir(shim)?;
    for name in cases.iter().map(|c| &c.command[0]).collect::<BTreeSet<_>>() {
        let path = shim.join(name);
        fs::write(
            &path,
            "#!/bin/sh\n/bin/cat \"$LM_PERF_INPUT\"\nexit \"$LM_PERF_EXIT\"\n",
        )?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}
#[cfg(not(unix))]
fn create_shims(_: &[Case], _: &Path) -> Result<()> {
    bail!("performance replay requires Unix /bin/sh and /bin/cat")
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.runs > 0, "--runs must be positive");
    ensure!(!args.output.exists(), "artifact directory already exists");
    fs::create_dir_all(&args.output)?;
    let directory = args.output.canonicalize()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cache = root.join("target");
    let binary = if let Some(path) = &args.binary {
        Artifact::load(path, root)?
    } else {
        Artifact::build(root, "HEAD", &directory.join("artifacts/after"), &cache)?
    };
    let before = if let Some(reference) = &args.before_ref {
        Some(Artifact::build(
            root,
            reference,
            &directory.join("artifacts/before"),
            &cache,
        )?)
    } else {
        args.before
            .as_ref()
            .map(|p| Artifact::load(p, root))
            .transpose()?
    };
    if args.build_only {
        return Ok(());
    }
    let repo = args
        .repo
        .unwrap_or_else(|| std::env::temp_dir().join("lmr-perf-typescript"));
    let cases = prepare(&repo, &directory)?;
    let baseline: Option<Value> = args
        .baseline_results
        .as_ref()
        .map(|p| -> Result<Value> {
            Ok(serde_json::from_slice(&fs::read(p.join("results.json"))?)?)
        })
        .transpose()?;
    let baseline_provenance = if let Some(baseline) = &baseline {
        ensure!(
            baseline["schema_version"] == 2,
            "baseline lacks versioned artifact provenance"
        );
        let provenance: Provenance = serde_json::from_value(baseline["before_artifact"].clone())
            .context("baseline before artifact missing")?;
        provenance.validate(root)?;
        ensure!(
            baseline["revision"] == REVISION,
            "baseline revision mismatch"
        );
        Some(provenance)
    } else {
        None
    };
    let shim = directory.join("bin");
    create_shims(&cases, &shim)?;
    let mut result = json!({"schema_version":2, "revision":REVISION, "after_artifact":binary.provenance, "cases":[], "passed":true, "threshold_policy":"every sample, strict absolute limit"});
    if let Some(before) = &before {
        result["before_artifact"] = json!(before.provenance);
    } else if let Some(provenance) = &baseline_provenance {
        result["before_artifact"] = json!(provenance);
    }
    let mut failures = Vec::new();
    for case in cases {
        let mut row = serde_json::to_value(&case)?;
        let mut samples = Vec::new();
        let mut measurements = Vec::new();
        let mut last = None;
        for _ in 0..if case.threshold.is_some() {
            args.runs
        } else {
            1
        } {
            let (output, elapsed) = invoke(&binary, &case, &directory, &shim, false)?;
            samples.push(elapsed.seconds);
            measurements.push(elapsed);
            last = Some(output);
        }
        let output = last.context("missing sample")?;
        fs::write(
            directory.join(format!("{}.after.stdout", case.name)),
            &output.stdout,
        )?;
        fs::write(
            directory.join(format!("{}.after.stderr", case.name)),
            &output.stderr,
        )?;
        let maximum = samples.iter().copied().fold(0.0, f64::max);
        let mut sorted = samples.clone();
        sorted.sort_by(f64::total_cmp);
        let n = sorted.len();
        let median = (sorted[(n - 1) / 2] + sorted[n / 2]) / 2.0;
        row["measurements"] = json!(measurements);
        row["seconds"] = json!(samples);
        row["median"] = json!(median);
        row["maximum"] = json!(maximum);
        row["stdout_sha256"] = json!(digest(&output.stdout));
        row["stderr_sha256"] = json!(digest(&output.stderr));
        if let Some(threshold) = case.threshold {
            if maximum >= threshold {
                failures.push(format!("{}: {maximum:.3}s >= {threshold:.3}s", case.name));
            }
        }
        if before.is_some() || baseline.is_some() {
            let (old_stdout, old_stderr, elapsed) = if let Some(before) = &before {
                let (old, measurement) = invoke(before, &case, &directory, &shim, true)?;
                (old.stdout, old.stderr, measurement)
            } else {
                let rows = baseline.as_ref().context("baseline")?["cases"]
                    .as_array()
                    .context("baseline cases")?;
                let recorded = rows
                    .iter()
                    .find(|r| r["name"] == case.name)
                    .context("missing baseline case")?;
                ensure!(
                    recorded["sha256"] == case.sha256,
                    "{}: baseline input hash mismatch",
                    case.name
                );
                let path = args.baseline_results.as_ref().context("baseline path")?;
                let stdout = fs::read(path.join(format!("{}.before.json", case.name)))?;
                let stderr = fs::read(path.join(format!("{}.before.stderr", case.name)))?;
                ensure!(
                    recorded["before_stdout_sha256"] == digest(&stdout)
                        && recorded["before_stderr_sha256"] == digest(&stderr),
                    "baseline output hash mismatch"
                );
                let measurement: Measurement =
                    serde_json::from_value(recorded["before_measurement"].clone())?;
                let provenance = baseline_provenance
                    .as_ref()
                    .context("baseline provenance")?;
                measurement.verify_identity(provenance)?;
                (stdout, stderr, measurement)
            };
            let (new, after_measurement) = invoke(&binary, &case, &directory, &shim, true)?;
            fs::write(
                directory.join(format!("{}.before.json", case.name)),
                &old_stdout,
            )?;
            fs::write(
                directory.join(format!("{}.before.stderr", case.name)),
                &old_stderr,
            )?;
            fs::write(
                directory.join(format!("{}.after.json", case.name)),
                &new.stdout,
            )?;
            let parity = old_stdout == new.stdout && old_stderr == new.stderr;
            row["before_seconds"] = json!(elapsed.seconds);
            row["before_measurement"] = json!(elapsed);
            row["after_json_measurement"] = json!(after_measurement);
            row["baseline_reused"] = json!(baseline.is_some());
            row["before_stdout_sha256"] = json!(digest(&old_stdout));
            row["before_stderr_sha256"] = json!(digest(&old_stderr));
            row["parity"] = json!(parity);
            if !parity {
                failures.push(format!("{}: JSON/stdout/stderr mismatch", case.name));
            }
            let old: Value = serde_json::from_slice(&old_stdout)?;
            ensure!(
                old["output"]
                    .as_str()
                    .context("original output")?
                    .as_bytes()
                    == output.stdout,
                "{}: rendered stdout mismatch",
                case.name
            );
        }
        println!(
            "{}: max={maximum:.3}s, bytes={}{}",
            case.name,
            case.bytes,
            if row["parity"].is_boolean() {
                format!(", parity={}", row["parity"])
            } else {
                String::new()
            }
        );
        result["cases"]
            .as_array_mut()
            .context("result cases")?
            .push(row);
        result["passed"] = json!(failures.is_empty());
        result["failures"] = json!(failures);
        fs::write(
            directory.join("results.json"),
            serde_json::to_string_pretty(&result)? + "\n",
        )?;
    }
    if !failures.is_empty() {
        bail!("{}", failures.join("\n"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn baseline_measurement_must_name_the_recorded_commit_and_binary() {
        let provenance = Provenance {
            commit: "commit-a".into(),
            tree: String::new(),
            binary_sha256: "hash-a".into(),
            archive_sha256: String::new(),
            rustc: String::new(),
            build_command: Vec::new(),
        };
        let measurement = Measurement {
            seconds: 0.1,
            commit: "commit-a".into(),
            binary_sha256: "hash-a".into(),
            format: "json".into(),
        };
        measurement.verify_identity(&provenance).unwrap();
        let mut changed = measurement.clone();
        changed.commit = "commit-b".into();
        assert!(changed.verify_identity(&provenance).is_err());
        changed = measurement;
        changed.binary_sha256 = "hash-b".into();
        assert!(changed.verify_identity(&provenance).is_err());
    }
}
