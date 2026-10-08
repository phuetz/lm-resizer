mod support;
use support::*;

#[test]
fn stream_without_json_keeps_live_streams_on_their_native_channels() {
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
    assert!(stderr.contains("\n[stderr]\nERROR été\r\n"), "{stderr:?}");
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
            .args(options.iter().copied())
            .arg("--")
            .arg(producer())
            .arg("dual")
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(23), "{out:?}");
        let value = report(&out);
        if options.is_empty() {
            assert!(value["streams"].is_null());
        } else {
            assert_eq!(value["streams"]["stdout_bytes"], 21);
            assert_eq!(value["streams"]["stderr_bytes"], 13);
            assert!(value["output"]
                .as_str()
                .unwrap()
                .contains("\n[stderr]\nERROR été\r\n"));
        }
        assert_eq!(
            recovered(dir.path(), &value),
            "OUT begin\r\nERROR été\r\nOUT end\0\r\n".as_bytes()
        );
    }
}
