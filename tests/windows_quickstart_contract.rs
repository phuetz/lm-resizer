#[cfg(windows)]
mod support;
#[cfg(windows)]
use support::*;

#[cfg(windows)] // echo is a cmd.exe builtin, rather than a Windows executable.
#[test]
fn windows_quickstart_invokes_echo_through_cmd() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("lm-resizer exec -- cmd.exe /d /c echo hello"));
    let dir = sandbox();
    let out = lm(dir.path())
        .args(["exec", "--", "cmd.exe", "/d", "/c", "echo", "hello"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello"));
}
