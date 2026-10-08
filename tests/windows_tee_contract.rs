mod support;
use support::*;

#[test]
fn stream_without_json_keeps_live_output_on_stdout() {
    let dir = sandbox();
    let out = lm(dir.path())
        .args(["exec", "--stream", "--"])
        .arg(producer())
        .arg("dual")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(23), "{out:?}");
    assert_eq!(
        out.stdout,
        "OUT begin\r\nERROR été\r\nOUT end\0\r\n".as_bytes()
    );
}
#[test]
fn option_modes_archive_original_mixed_stream_bytes() {
    let dir = sandbox();
    for options in [
        vec![],
        vec!["--raw-on-failure"],
        vec!["--stream"],
        vec!["--raw-on-failure", "--stream"],
    ] {
        let out = lm(dir.path())
            .args(["exec", "--json"])
            .args(options)
            .arg("--")
            .arg(producer())
            .arg("dual")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(23), "{out:?}");
        let value = report(&out);
        assert_eq!(
            recovered(dir.path(), &value),
            "OUT begin\r\nERROR été\r\nOUT end\0\r\n".as_bytes()
        );
    }
}
