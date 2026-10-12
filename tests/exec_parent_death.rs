//! `kill -9` du processus `exec` : l'enfant ne lui survit pas (Linux, `PR_SET_PDEATHSIG`).
//! Audit du 9 octobre 2026 : `exec -- sleep 40` tué par SIGKILL laissait `sleep` rattaché à un
//! autre parent ; un serveur lancé par le crochet continuait après que l'agent avait tué l'outil.
#![cfg(target_os = "linux")]
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn children(pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    if let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) {
        for task in tasks.flatten() {
            if let Ok(text) = std::fs::read_to_string(task.path().join("children")) {
                out.extend(
                    text.split_whitespace()
                        .filter_map(|n| n.parse::<u32>().ok()),
                );
            }
        }
    }
    out
}

/// Vivant : présent dans `/proc` et pas à l'état zombie.
fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .is_some_and(|state| state != "Z" && state != "X")
    })
}

fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

#[test]
fn killing_exec_with_sigkill_kills_its_child() {
    let mut survivors = Vec::new();
    for mode in [&[][..], &["--raw-on-failure"][..], &["--stream"][..]] {
        let state = tempfile::tempdir().unwrap();
        let mut exec = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("LM_RESIZER_STATE_DIR", state.path())
            .arg("exec")
            .args(mode)
            .args(["--", "sleep", "30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let parent = exec.id();
        let mut child = None;
        assert!(
            wait_for(Duration::from_secs(10), || {
                child = children(parent).first().copied();
                child.is_some()
            }),
            "{mode:?}: aucun enfant sous {parent}"
        );
        let child = child.unwrap();
        exec.kill().unwrap(); // SIGKILL
        exec.wait().unwrap();
        if !wait_for(Duration::from_secs(5), || !alive(child)) {
            survivors.push(format!(
                "{mode:?}: enfant {child} vivant après kill -9 de {parent}"
            ));
            let _ = Command::new("kill")
                .args(["-9", &child.to_string()])
                .status();
        }
    }
    assert!(survivors.is_empty(), "{}", survivors.join("\n"));
}
