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
fn interleaving_check_accepts_drain_orders_only() {
    let (out, err) = (
        &b"OUT begin\r\nOUT end\0\r\n"[..],
        "ERROR été\r\n".as_bytes(),
    );
    for order in [
        "OUT begin\r\nERROR été\r\nOUT end\0\r\n",
        "OUT begin\r\nOUT end\0\r\nERROR été\r\n",
        "ERROR été\r\nOUT begin\r\nOUT end\0\r\n",
        "OUT bERROR étéegin\r\n\r\nOUT end\0\r\n",
    ] {
        assert!(is_interleaving(order.as_bytes(), out, err), "{order:?}");
    }
    for wrong in [
        "OUT begin\r\nOUT end\0\r\n\n[stderr]\nERROR été\r\n",
        "OUT begin\r\nOUT end\0\r\n",
        "OUT end\0\r\nERROR été\r\nOUT begin\r\n",
        "OUT begin\r\nERROR \u{fffd}t\u{fffd}\r\nOUT end\0\r\n",
    ] {
        assert!(!is_interleaving(wrong.as_bytes(), out, err), "{wrong:?}");
    }
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
        let tee = recovered(dir.path(), &value);
        if options.is_empty() {
            // One shared pipe: the kernel keeps the producer's write order.
            assert_eq!(tee, "OUT begin\r\nERROR été\r\nOUT end\0\r\n".as_bytes());
        } else {
            // Two drained pipes: exact streams, no synthetic boundary, any
            // cross-stream order.
            assert!(
                is_interleaving(
                    &tee,
                    b"OUT begin\r\nOUT end\0\r\n",
                    "ERROR été\r\n".as_bytes()
                ),
                "{options:?}: {:?}",
                String::from_utf8_lossy(&tee)
            );
        }
    }
}
