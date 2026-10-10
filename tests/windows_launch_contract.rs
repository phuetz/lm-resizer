mod support;
use support::*;

#[test]
#[cfg(unix)] // Invalid Windows images can open an OS dialog in unattended tests.
fn invalid_executable_is_126_in_every_exec_mode() {
    let dir = sandbox();
    let file = dir
        .path()
        .join(format!("invalid{}", std::env::consts::EXE_SUFFIX));
    let executed = dir.path().join("unexpected-shell-execution");
    // Valid shell text without a shebang must remain an invalid image. If
    // execvp falls back to sh, this creates observable evidence before exit.
    std::fs::write(
        &file,
        format!("printf executed > '{}'\nexit 23\n", executed.display()),
    )
    .unwrap();
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
        assert!(
            !executed.exists(),
            "an invalid image was interpreted by a shell: {out:?}"
        );
        assert_eq!(out.status.code(), Some(126), "{out:?}");
        assert!(report(&out)["output"]
            .as_str()
            .unwrap()
            .contains("cannot execute"));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn kernel_rejected_images_never_reach_a_shell() {
    use std::os::unix::fs::PermissionsExt;
    let dir = sandbox();
    let executed = dir.path().join("unexpected-interpreter-execution");
    let interpreter = dir.path().join("headerless-interpreter");
    std::fs::write(
        &interpreter,
        format!("printf executed > '{}'\nexit 23\n", executed.display()),
    )
    .unwrap();
    std::fs::set_permissions(&interpreter, std::fs::Permissions::from_mode(0o700)).unwrap();
    // Recognizing a prefix in the parent is insufficient: the kernel can
    // reject a malformed ELF or the interpreter named by a valid shebang.
    for (name, contents) in [
        ("truncated-elf", b"\x7fELF\nexit 23\n".to_vec()),
        (
            "bad-interpreter",
            format!("#!{}\nexit 23\n", interpreter.display()).into_bytes(),
        ),
    ] {
        let image = dir.path().join(name);
        std::fs::write(&image, contents).unwrap();
        std::fs::set_permissions(&image, std::fs::Permissions::from_mode(0o700)).unwrap();
        for options in [vec![], vec!["--raw-on-failure"], vec!["--stream"]] {
            let out = lm(dir.path())
                .args(["exec", "--json"])
                .args(options)
                .arg("--")
                .arg(&image)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(126), "{name}: {out:?}");
            assert!(!executed.exists(), "{name}: {out:?}");
            assert!(report(&out)["output"]
                .as_str()
                .unwrap()
                .contains("cannot execute"));
        }
    }
}

#[cfg(target_os = "linux")]
#[test]
fn direct_exec_preserves_shebang_arguments_environment_and_path_search() {
    use std::os::unix::fs::PermissionsExt;
    let dir = sandbox();
    let script = dir.path().join("path-selected-script");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '%s|%s|%s\\n' \"$1\" \"$2\" \"$LMR_LAUNCH_VALUE\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    for options in [vec![], vec!["--raw-on-failure"], vec!["--stream"]] {
        let out = lm(dir.path())
            .env("PATH", dir.path())
            .env("LMR_LAUNCH_VALUE", "env value")
            .args(["exec", "--json"])
            .args(options)
            .args(["--", "path-selected-script", "a b", "'quoted'"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(report(&out)["output"]
            .as_str()
            .unwrap()
            .contains("a b|'quoted'|env value"));

        // execv alone would execute this cwd file even with an empty PATH.
        let out = lm(dir.path())
            .current_dir(dir.path())
            .env("PATH", dir.path().join("absent-path-directory"))
            .args(["exec", "--json"])
            .args(["--", "path-selected-script"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(127), "{out:?}");
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
