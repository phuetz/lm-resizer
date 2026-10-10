mod support;
use std::process::Command;
use support::*;
#[test]
fn utf16_bom_error_is_visible_and_tee_is_exact() {
    let dir = sandbox();
    let original = Command::new(producer())
        .arg("utf16")
        .output()
        .unwrap()
        .stdout;
    let out = lm(dir.path())
        .args(["exec", "--json", "--"])
        .arg(producer())
        .arg("utf16")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(23));
    let value = report(&out);
    assert!(
        value["output"]
            .as_str()
            .unwrap()
            .contains("ERROR erreur utile été résumé"),
        "{value}"
    );
    assert_eq!(recovered(dir.path(), &value), original);
}
