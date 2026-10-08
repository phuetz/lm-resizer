//! Stream provenance contract, exercised with a portable Rust producer on Windows.
#![cfg(windows)]

mod support;
use support::*;

#[test]
fn raw_failure_view_labels_stderr_but_tee_stays_unmodified() {
    let dir = sandbox();
    let out = lm(dir.path())
        .args(["exec", "--json", "--raw-on-failure", "--"])
        .arg(producer())
        .arg("dual")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(23), "{out:?}");

    let value = report(&out);
    assert_eq!(value["streams"]["stdout_bytes"], 21);
    assert_eq!(value["streams"]["stderr_bytes"], 13);
    assert_eq!(
        value["streams"]["layout"],
        "stdout_then_stderr; [stderr] boundary; no cross-stream chronology"
    );
    assert_eq!(
        value["output"],
        "[FAIL] Command failed (exit code: 23)\n\
OUT begin\r\nOUT end\0\r\n\n[stderr]\nERROR été\r\n\
[capture: stdout and stderr captured separately; displayed order is not chronological]\n"
    );
    assert_eq!(
        recovered(dir.path(), &value),
        "OUT begin\r\nERROR été\r\nOUT end\0\r\n".as_bytes()
    );
}

#[test]
fn stream_keeps_live_stdout_and_stderr_on_their_native_channels() {
    let dir = sandbox();
    let out = lm(dir.path())
        .args(["exec", "--stream", "--"])
        .arg(producer())
        .arg("dual")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(23), "{out:?}");
    assert_eq!(out.stdout, b"OUT begin\r\nOUT end\0\r\n");

    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.starts_with("ERROR été\r\n"), "{stderr:?}");
    assert!(
        stderr.contains("\n[lm-resizer filtered output]\n"),
        "{stderr:?}"
    );
    assert!(stderr.contains("\n[stderr]\nERROR été\r\n"), "{stderr:?}");
}
