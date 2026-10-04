use std::process::Command;

fn proxy(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .arg("mcp-proxy")
        .args(args)
        .output()
        .expect("run mcp-proxy")
}

#[test]
fn rejects_unknown_option_before_separator() {
    let output = proxy(&["--no-ccr", "--", "/bin/true"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "unknown option '--no-ccr' for mcp-proxy; options go before `--`"
    ));
}

#[test]
fn accepts_separator() {
    assert!(proxy(&["--", "/bin/true"]).status.success());
}

#[test]
fn accepts_no_separator() {
    assert!(proxy(&["/bin/true"]).status.success());
}

#[test]
fn forwards_upstream_options_after_separator() {
    assert!(proxy(&["--", "/bin/true", "--port", "1"]).status.success());
}
