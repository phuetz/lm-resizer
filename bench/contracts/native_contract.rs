use std::io::Read;
use std::process::Command;

#[test]
fn parity_corpus_is_anonymized() {
    assert!(Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/bench/real/check_public_captures.py"
        ))
        .status()
        .unwrap()
        .success());
    let bytes = include_bytes!("../rtk-parity/corpus.json.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(&bytes[..])
        .read_to_string(&mut text)
        .unwrap();
    let cases: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(cases.as_array().unwrap().len() >= 46);
    // Public measurements and rendered views must satisfy the same rule.
    for bytes in [
        &include_bytes!("../rtk-parity/corpus.json.gz")[..],
        &include_bytes!("../rtk-parity/results.json.gz")[..],
        &include_bytes!("../rtk-parity/views.json.gz")[..],
        &include_bytes!("../headroom-extra/corpus.json.gz")[..],
    ] {
        let mut artifact = String::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_string(&mut artifact)
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&artifact).unwrap();
        assert_anonymous(&value);
    }
    let live: serde_json::Value =
        serde_json::from_str(include_str!("../rtk-parity/live.json")).unwrap();
    assert_anonymous(&live);
    let extra: serde_json::Value =
        serde_json::from_str(include_str!("../headroom-extra/results.json")).unwrap();
    assert_anonymous(&extra);
    let mixed: serde_json::Value =
        serde_json::from_str(include_str!("../rtk-parity/mixed-streams.json")).unwrap();
    assert_anonymous(&mixed);
    let tsc: serde_json::Value =
        serde_json::from_str(include_str!("../rtk-parity/tsc-nondeterminism.json")).unwrap();
    assert_anonymous(&tsc);
}

fn assert_anonymous(value: &serde_json::Value) {
    static HOME_PATH_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"/(?:home|Users)/([^/\s]+)").unwrap());
    match value {
        serde_json::Value::String(text) => {
            for forbidden in [
                concat!("/home/", "pa", "trice"),
                concat!("/data/", "pa", "trice"),
                "phuetz",
                concat!("Pa", "trice"),
            ] {
                assert!(
                    !text.contains(forbidden),
                    "personal identifier: {forbidden}"
                );
            }
            for found in HOME_PATH_RE.captures_iter(text) {
                assert_eq!(&found[1], "user", "non-anonymous home directory");
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(assert_anonymous),
        serde_json::Value::Object(items) => items.values().for_each(assert_anonymous),
        _ => {}
    }
}

#[test]
#[cfg(unix)]
fn direct_cargo_tee_survives_ten_mib_cap() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let shim = dir.path().join("cargo");
    std::fs::write(
        &shim,
        "#!/bin/sh\nhead -c 11000000 /dev/zero | tr '\\000' x\n",
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_owned())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("PATH", path)
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["exec", "--json", "--", "cargo", "build"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out.stderr);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(report["filter"].as_str().unwrap().starts_with("native:"));
    assert_eq!(report["original_bytes"], 11_000_000);
    let key = report["tee_hint"]
        .as_str()
        .unwrap()
        .strip_prefix("[raw: ")
        .unwrap()
        .strip_suffix(']')
        .unwrap();
    let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["tee", "read", key])
        .output()
        .unwrap();
    assert!(recovered.status.success());
    assert_eq!(recovered.stdout.len(), 11_000_000);
    assert!(recovered.stdout.iter().all(|&b| b == b'x'));
}

#[test]
#[cfg(unix)]
fn direct_mixed_streams_preserve_write_order_and_binary_bytes_beyond_ten_mib() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    // os.write gives an explicit producer order without buffering or sleeps.
    let script = "#!/usr/bin/env python3\nimport os\nfor i in range(6000):\n os.write(1, b'O'*1024+b'\\r\\n')\n os.write(2, b'E'*1024+b'\\x00\\n')\nraise SystemExit(3)\n";
    let shim = dir.path().join("cargo");
    std::fs::write(&shim, script).unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_owned())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let mut expected = Vec::new();
    for _ in 0..6000 {
        expected.extend_from_slice(&[b'O'; 1024]);
        expected.extend_from_slice(b"\r\n");
        expected.extend_from_slice(&[b'E'; 1024]);
        expected.extend_from_slice(b"\0\n");
    }
    for program in ["cargo", "pytest"] {
        if program == "pytest" {
            std::fs::copy(&shim, dir.path().join("pytest")).unwrap();
        }
        let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("PATH", &path)
            .env(
                "LM_RESIZER_STATE_DIR",
                dir.path().join(program).with_extension("state"),
            )
            .args(["exec", "--json", "--", program, "test"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(3), "{:?}", out.stderr);
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(report["filter"].as_str().unwrap().starts_with("native:"));
        assert_eq!(report["original_bytes"], expected.len());
        let key = report["tee_hint"]
            .as_str()
            .unwrap()
            .strip_prefix("[raw: ")
            .unwrap()
            .strip_suffix(']')
            .unwrap();
        let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env(
                "LM_RESIZER_STATE_DIR",
                dir.path().join(program).with_extension("state"),
            )
            .args(["tee", "read", key])
            .output()
            .unwrap();
        assert!(recovered.status.success());
        assert_eq!(recovered.stdout.len(), expected.len());
        assert!(
            recovered.stdout == expected,
            "byte order differs for {program}"
        );
    }
}

#[test]
#[cfg(unix)]
fn direct_empty_output_is_not_replaced_with_a_generated_summary() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let shim = dir.path().join("pytest");
    std::fs::write(&shim, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_owned())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("PATH", path)
        .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
        .args(["exec", "--json", "--", "pytest"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["original_bytes"], 0);
    if let Some(hint) = report["tee_hint"].as_str() {
        let key = hint
            .strip_prefix("[raw: ")
            .unwrap()
            .strip_suffix(']')
            .unwrap();
        let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
            .args(["tee", "read", key])
            .output()
            .unwrap();
        assert!(recovered.status.success());
        assert!(recovered.stdout.is_empty());
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn run_capture_shim(
    script: &str,
    programs: &[&str],
    command: &[&str],
) -> (serde_json::Value, Vec<u8>, i32) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    for program in programs {
        let shim = dir.path().join(program);
        std::fs::write(&shim, script).unwrap();
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_owned())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let state = dir.path().join("state");
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("PATH", path)
        .env("LM_RESIZER_STATE_DIR", &state)
        .args(["exec", "--json", "--"])
        .args(command)
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stderr)));
    let raw = if let Some(hint) = report["tee_hint"].as_str() {
        let key = hint
            .strip_prefix("[raw: ")
            .unwrap()
            .strip_suffix(']')
            .unwrap();
        let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", &state)
            .args(["tee", "read", key])
            .output()
            .unwrap();
        assert!(recovered.status.success());
        recovered.stdout
    } else {
        report["output"].as_str().unwrap().as_bytes().to_vec()
    };
    if let Some(start) = report["output"]
        .as_str()
        .unwrap()
        .find("lm-resizer tee read ")
    {
        let output = report["output"].as_str().unwrap();
        let command = output[start..].split(']').next().unwrap();
        let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", dir.path().join("state"))
            .args(command.split_whitespace().skip(1))
            .output()
            .unwrap();
        assert!(recovered.status.success());
        assert_eq!(
            recovered.stdout, raw,
            "the displayed recovery command must return the tee"
        );
    }
    (report, raw, out.status.code().unwrap())
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn direct_jest_preserves_failure_and_stderr_in_tee() {
    let mut corpus = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("../rtk-parity/corpus.json.gz")[..])
        .read_to_string(&mut corpus)
        .unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&corpus).unwrap();
    let case = cases.iter().find(|c| c["id"] == "fresh-jest").unwrap();
    let stderr = case["stderr"].as_str().unwrap();
    let script = format!(
        "#!/usr/bin/env python3\nimport os\nos.write(2,{}.encode())\nraise SystemExit(1)\n",
        serde_json::to_string(stderr).unwrap()
    );
    let (report, raw, code) =
        run_capture_shim(&script, &["jest", "npx", "npm", "pnpm", "yarn"], &["jest"]);
    let view = report["output"].as_str().unwrap();
    assert!(
        view.contains("invoice"),
        "failed test must remain visible: {view}"
    );
    assert!(!view.contains("PASSTHROUGH"));
    assert_eq!(raw, stderr.as_bytes());
    assert_eq!(code, 1);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn direct_tee_keeps_large_writes_and_fd_aliases_in_order() {
    let script = "#!/usr/bin/env python3\nimport os\na=os.dup(2)\nos.write(1,b'O'*70000+b'\\r\\n')\nos.writev(a,[b'E'*70000,b'\\x00\\n'])\npid=os.fork()\nif pid==0:\n os.dup2(a,1)\n os.write(1,b'child-error\\n')\n os._exit(0)\nos.waitpid(pid,0)\nos.write(1,b'last-out\\n')\nraise SystemExit(3)\n";
    let (_, raw, code) = run_capture_shim(script, &["pytest"], &["pytest"]);
    let mut expected = vec![b'O'; 70000];
    expected.extend_from_slice(b"\r\n");
    expected.extend_from_slice(&vec![b'E'; 70000]);
    expected.extend_from_slice(b"\0\nchild-error\nlast-out\n");
    assert_eq!(raw, expected);
    assert_eq!(code, 3);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn direct_concurrent_children_are_drained_and_atomic_records_stay_whole() {
    let script = "#!/usr/bin/env python3\nimport os\npids=[]\nfor fd,c in [(1,b'A'),(2,b'B')]:\n p=os.fork()\n if p==0:\n  os.write(fd,c*90000+b'\\n')\n  os._exit(0)\n pids.append(p)\nfor p in pids: os.waitpid(p,0)\n";
    let (_, raw, code) = run_capture_shim(script, &["pytest"], &["pytest"]);
    assert_eq!(raw.len(), 180002);
    // Writes larger than PIPE_BUF may interleave within a producer's line.
    // Requiring two intact 90 KB rows would assert a guarantee pipes don't give.
    assert_eq!(raw.iter().filter(|&&b| b == b'A').count(), 90000);
    assert_eq!(raw.iter().filter(|&&b| b == b'B').count(), 90000);
    assert_eq!(raw.iter().filter(|&&b| b == b'\n').count(), 2);
    assert_eq!(code, 0);
    let script = "#!/usr/bin/env python3\nimport os\npids=[]\nfor fd,c in [(1,b'A'),(2,b'B')]:\n p=os.fork()\n if p==0:\n  assert os.fpathconf(fd,'PC_PIPE_BUF')>=1024\n  for n in range(500): os.write(fd,c+str(n).encode().ljust(1022,b'.')+b'\\n')\n  os._exit(0)\n pids.append(p)\nfor p in pids: os.waitpid(p,0)\n";
    let (_, raw, code) = run_capture_shim(script, &["pytest"], &["pytest"]);
    assert_eq!(code, 0);
    let mut next = [0; 2];
    for row in raw.split_inclusive(|&b| b == b'\n') {
        assert_eq!(row.len(), 1024);
        let stream = match row[0] {
            b'A' => 0,
            b'B' => 1,
            other => panic!("invalid stream {other}"),
        };
        let payload = format!("{:.<1022}", next[stream]);
        let expected = format!("{}{payload}\n", char::from(row[0]));
        assert_eq!(row, expected.as_bytes());
        next[stream] += 1;
    }
    assert_eq!(next, [500, 500]);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn direct_mixed_view_and_tee_retains_producer_order() {
    let script = "#!/usr/bin/env python3\nimport os\nfor i in range(8):\n os.write(1,f'OUT{i:04d}\\n'.encode())\n os.write(2,f'ERR{i:04d}\\n'.encode())\nraise SystemExit(3)\n";
    let (report, raw, code) = run_capture_shim(script, &["pytest"], &["pytest"]);
    let expected_raw: String = (0..8).map(|i| format!("OUT{i:04}\nERR{i:04}\n")).collect();
    assert_eq!(raw, expected_raw.as_bytes());
    assert_eq!(
        report["output"],
        format!("[FAIL] Command failed (exit code: 3)\n{expected_raw}")
    );
    assert_eq!(code, 3);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn direct_ls_and_tree_archive_the_native_producer_not_the_view() {
    for (program, raw) in [
        (
            "ls",
            "total 4\n-rw-r--r-- 1 user group 42 Oct  3 10:00 only.txt\n",
        ),
        ("tree", ".\n└── only.txt\n\n0 directories, 1 file\n"),
    ] {
        let script = format!(
            "#!/usr/bin/env python3\nimport os\nos.write(1,{}.encode())\n",
            serde_json::to_string(raw).unwrap()
        );
        let (report, recovered, code) = run_capture_shim(&script, &[program], &[program]);
        assert_eq!(code, 0);
        assert_eq!(report["original_bytes"], raw.len());
        assert_eq!(recovered, raw.as_bytes());
        assert!(report["output"].as_str().unwrap().contains("only.txt"));
        assert!(report["filtered_bytes"].as_u64().unwrap() < raw.len() as u64);
        assert!(report["tee_hint"].is_string());
    }
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn direct_failed_exec_reports_launch_errors_and_keeps_an_empty_raw() {
    let absent = tempfile::tempdir().unwrap();
    let script = format!(
        "#!{}\n",
        absent.path().join("missing-interpreter").display()
    );
    let (report, raw, code) = run_capture_shim(&script, &["pytest"], &["pytest"]);
    assert_eq!(code, 127);
    assert!(raw.is_empty());
    assert_eq!(report["original_bytes"], 0);
    let view = report["output"].as_str().unwrap();
    assert!(view.starts_with("[FAIL] Command failed (exit code: 127)\n"));
    assert!(view.contains("lm-resizer: cannot execute pytest:"));
}

#[test]
fn pipe_recovery_hint_resolves_through_the_lm_command() {
    use std::io::Write;
    use std::process::Stdio;
    let dir = tempfile::tempdir().unwrap();
    // Disposition réelle du format par défaut : en-tête, Author:, Date:, ligne vide, message indenté.
    let raw = "commit abcdef\nAuthor: A <a@example.test>\nDate:   today\n\n".to_owned()
        + &"    complete commit detail\n".repeat(100);
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path())
        .args(["pipe", "--filter", "git-log", "--json", "--exit-code", "2"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(raw.as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let output = report["output"].as_str().unwrap();
    let key = output
        .split("[tee:")
        .nth(1)
        .unwrap()
        .split(']')
        .next()
        .unwrap();
    let recovered = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("LM_RESIZER_STATE_DIR", dir.path())
        .args(["tee", "read", key])
        .output()
        .unwrap();
    assert!(recovered.status.success());
    assert_eq!(recovered.stdout, raw.as_bytes());
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn direct_ctest_tail_recall_returns_the_full_raw_and_keeps_failure_count() {
    let mut input = String::from("Test project /tmp/build\n\n0% tests passed, 25 tests failed out of 25\n\nThe following tests FAILED:\n");
    for number in 1..=25 {
        input.push_str(&format!("\t  {number} - broken_{number} (Failed)\n"));
    }
    let script = format!(
        "#!/usr/bin/env python3\nimport os\nos.write(1,{}.encode())\nraise SystemExit(8)\n",
        serde_json::to_string(&input).unwrap()
    );
    let (report, raw, code) = run_capture_shim(&script, &["ctest"], &["ctest"]);
    let view = report["output"].as_str().unwrap();
    assert!(view.contains("25 tests failed"));
    assert_eq!(raw, input.as_bytes());
    assert_eq!(code, 8);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
fn direct_capture_drains_a_writer_that_outlives_its_parent() {
    let script = "#!/usr/bin/env python3\nimport os, time\nif os.fork() == 0:\n time.sleep(0.05)\n os.write(2,b'late stderr\\n')\n os._exit(0)\nos.write(1,b'parent stdout\\n')\nos._exit(7)\n";
    let (report, raw, code) = run_capture_shim(script, &["cargo"], &["cargo", "build"]);
    assert_eq!(code, 7);
    assert_eq!(raw, b"parent stdout\nlate stderr\n");
    assert_eq!(report["original_bytes"], raw.len());
}
