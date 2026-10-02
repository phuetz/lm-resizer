#![cfg(unix)]
use sha2::{Digest, Sha256};
use std::{os::unix::fs::PermissionsExt, process::Command};
#[test]
fn installer_selects_checksum_verified_aarch64_archive() {
    let root = tempfile::tempdir().unwrap();
    let release = root.path().join("release");
    let tools = root.path().join("tools");
    std::fs::create_dir(&release).unwrap();
    std::fs::create_dir(&tools).unwrap();
    let uname = tools.join("uname");
    std::fs::write(
        &uname,
        "#!/bin/sh\ncase \"$1\" in -s) echo Linux;; -m) echo aarch64;; esac\n",
    )
    .unwrap();
    std::fs::set_permissions(uname, std::fs::Permissions::from_mode(0o755)).unwrap();
    let folder = "lm-resizer-9.9.9-linux-aarch64";
    std::fs::create_dir(release.join(folder)).unwrap();
    let binary = release.join(folder).join("lm-resizer");
    std::fs::write(&binary, "#!/bin/sh\necho 'lm-resizer 9.9.9'\n").unwrap();
    std::fs::set_permissions(binary, std::fs::Permissions::from_mode(0o755)).unwrap();
    let archive = format!("{folder}.tar.gz");
    assert!(Command::new("tar")
        .current_dir(&release)
        .args(["-czf", &archive, folder])
        .status()
        .unwrap()
        .success());
    let bytes = std::fs::read(release.join(&archive)).unwrap();
    std::fs::write(
        release.join(format!("{archive}.sha256")),
        format!("{:x}  {archive}\n", Sha256::digest(bytes)),
    )
    .unwrap();
    let dest = root.path().join("bin");
    let out = Command::new("sh")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/install.sh"))
        .env("HOME", root.path())
        .env(
            "PATH",
            format!("{}:{}", tools.display(), std::env::var("PATH").unwrap()),
        )
        .env("LM_RESIZER_VERSION", "9.9.9")
        .env(
            "LM_RESIZER_RELEASE_BASE_URL",
            format!("file://{}", release.display()),
        )
        .env("LM_RESIZER_INSTALL_DIR", &dest)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dest.join("lm-resizer").exists());
}
