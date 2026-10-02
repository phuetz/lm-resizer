use std::process::Command;
#[test]
fn update_dry_run_is_offline_and_does_not_install() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("bin");
    let out = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .env("HOME", root.path())
        .args(["update", "--version", "9.9.9", "--dry-run", "--install-dir"])
        .arg(&target)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!target.exists());
    assert!(String::from_utf8_lossy(&out.stdout).contains("SHA-256"));
}
#[cfg(unix)]
#[test]
fn update_installs_verified_local_release_and_rejects_corruption() {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let release = root.path().join("release");
    std::fs::create_dir(&release).unwrap();
    let platform = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "darwin-arm64"
        } else {
            "darwin-x86_64"
        }
    } else {
        "linux-x86_64"
    };
    let folder = format!("lm-resizer-9.9.9-{platform}");
    std::fs::create_dir(release.join(&folder)).unwrap();
    let binary = release.join(&folder).join("lm-resizer");
    std::fs::write(&binary, "#!/bin/sh\nprintf 'lm-resizer 9.9.9\\n'\n").unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let archive = format!("{folder}.tar.gz");
    assert!(Command::new("tar")
        .current_dir(&release)
        .args(["-czf", &archive, &folder])
        .status()
        .unwrap()
        .success());
    let bytes = std::fs::read(release.join(&archive)).unwrap();
    let checksum = format!("{:x}  {archive}\n", Sha256::digest(&bytes));
    std::fs::write(release.join(format!("{archive}.sha256")), checksum).unwrap();
    let target = root.path().join("bin");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
            .env("HOME", root.path())
            .args(["update", "--version", "9.9.9", "--apply", "--install-dir"])
            .arg(&target)
            .arg("--release-base-url")
            .arg(format!("file://{}", release.display()))
            .output()
            .unwrap()
    };
    let out = run();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let installed = std::fs::read(target.join("lm-resizer")).unwrap();
    std::fs::write(release.join(&archive), "corrupt").unwrap();
    assert!(!run().status.success());
    assert_eq!(std::fs::read(target.join("lm-resizer")).unwrap(), installed);
}
