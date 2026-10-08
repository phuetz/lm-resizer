mod support;
use support::*;

#[test]
#[cfg(unix)] // Invalid Windows images can open an OS dialog in unattended tests.
fn invalid_executable_is_126_in_every_exec_mode() {
    let dir = sandbox();
    let file = dir
        .path()
        .join(format!("invalid{}", std::env::consts::EXE_SUFFIX));
    std::fs::write(&file, b"this is not an executable image").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    for option in [None, Some("--raw-on-failure"), Some("--stream")] {
        let mut cmd = lm(dir.path());
        cmd.args(["exec", "--json"]);
        if let Some(option) = option {
            cmd.arg(option);
        }
        let out = cmd.arg("--").arg(&file).output().unwrap();
        assert_eq!(out.status.code(), Some(126), "{out:?}");
        assert!(report(&out)["output"]
            .as_str()
            .unwrap()
            .contains("cannot execute"));
    }
}

#[test]
fn directory_is_not_executable_in_every_exec_mode() {
    let dir = sandbox();
    for option in [None, Some("--raw-on-failure"), Some("--stream")] {
        let mut cmd = lm(dir.path());
        cmd.args(["exec", "--json"]);
        if let Some(option) = option {
            cmd.arg(option);
        }
        let out = cmd.arg("--").arg(dir.path()).output().unwrap();
        assert_eq!(out.status.code(), Some(126), "{out:?}");
        assert!(report(&out)["output"]
            .as_str()
            .unwrap()
            .contains("cannot execute"));
    }
}
#[test]
fn launch_failures_match_in_every_exec_mode() {
    let dir = sandbox();
    let mut baseline = None;
    for options in [vec![], vec!["--raw-on-failure"], vec!["--stream"]] {
        let out = lm(dir.path())
            .args(["exec", "--json"])
            .args(options)
            .args(["--", "lmr_missing_windows_contract_026"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(127), "{out:?}");
        let value = report(&out);
        let diagnostic = value["output"].as_str().unwrap().to_owned();
        assert!(diagnostic.contains("command not found") && diagnostic.contains("nothing was run"));
        if let Some(expected) = &baseline {
            assert_eq!(&diagnostic, expected);
        } else {
            baseline = Some(diagnostic);
        }
    }
}

#[cfg(unix)] // Unix permission bits can reliably make a producer non-executable.
#[test]
fn non_executable_is_126_in_every_exec_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = sandbox();
    let file = dir.path().join("not-executable");
    std::fs::write(&file, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    for option in [None, Some("--raw-on-failure"), Some("--stream")] {
        let mut cmd = lm(dir.path());
        cmd.args(["exec", "--json"]);
        if let Some(option) = option {
            cmd.arg(option);
        }
        let out = cmd.arg("--").arg(&file).output().unwrap();
        assert_eq!(out.status.code(), Some(126), "{out:?}");
        assert!(report(&out)["output"]
            .as_str()
            .unwrap()
            .contains("cannot execute"));
    }
}
