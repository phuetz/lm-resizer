//! Build and identify the immutable CLI actually used by the benchmark.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub commit: String,
    pub tree: String,
    pub binary_sha256: String,
    pub archive_sha256: String,
    pub rustc: String,
    pub build_command: Vec<String>,
}

pub struct Artifact {
    pub binary: PathBuf,
    pub provenance: Provenance,
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = super::checked(Command::new("git").arg("-C").arg(repo).args(args))?;
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn sidecar(binary: &Path) -> PathBuf {
    binary.with_extension("provenance.json")
}

impl Provenance {
    pub fn validate(&self, repo: &Path) -> Result<()> {
        ensure!(
            self.commit.len() == 40 && self.commit.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid artifact commit"
        );
        ensure!(
            git(repo, &["rev-parse", &format!("{}^{{tree}}", self.commit)])? == self.tree,
            "artifact tree mismatch"
        );
        ensure!(
            self.binary_sha256.len() == 64
                && self.binary_sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid binary SHA-256"
        );
        ensure!(
            self.archive_sha256.len() == 64
                && self.archive_sha256.bytes().all(|b| b.is_ascii_hexdigit())
                && !self.rustc.is_empty(),
            "incomplete build provenance"
        );
        let archive = super::checked(Command::new("git").arg("-C").arg(repo).args([
            "archive",
            "--format=tar",
            &self.commit,
        ]))?;
        ensure!(
            super::digest(&archive.stdout) == self.archive_sha256,
            "archive does not correspond to recorded Git commit"
        );
        ensure!(
            self.build_command == build_command(),
            "unsupported artifact build selection"
        );
        Ok(())
    }
}

fn build_command() -> Vec<String> {
    [
        "cargo",
        "build",
        "--locked",
        "--release",
        "--bin",
        "lm-resizer",
        "--target-dir",
        "<cache>",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

impl Artifact {
    pub fn verify(&self) -> Result<()> {
        ensure!(
            super::digest(&fs::read(&self.binary)?) == self.provenance.binary_sha256,
            "executed binary does not match provenance SHA-256"
        );
        Ok(())
    }

    pub fn load(binary: &Path, repo: &Path) -> Result<Self> {
        let binary = binary.canonicalize()?;
        let provenance: Provenance = serde_json::from_slice(
            &fs::read(sidecar(&binary))
                .context("artifact provenance missing; use --build-only or omit --binary")?,
        )?;
        provenance.validate(repo)?;
        let artifact = Self { binary, provenance };
        artifact.verify()?;
        Ok(artifact)
    }

    pub fn build(repo: &Path, reference: &str, directory: &Path, cache: &Path) -> Result<Self> {
        ensure!(
            git(repo, &["status", "--porcelain"])?.is_empty(),
            "commit changes before building benchmark artifacts"
        );
        ensure!(!directory.exists(), "artifact directory already exists");
        fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let commit = git(
            repo,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{reference}^{{commit}}"),
            ],
        )?;
        let tree = git(repo, &["rev-parse", &format!("{commit}^{{tree}}")])?;
        println!("Building CLI from commit {commit} (only --bin lm-resizer)");
        let archive = directory.join("source.tar");
        super::checked(
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["archive", "--format=tar", "--output"])
                .arg(&archive)
                .arg(&commit),
        )?;
        let archive_sha256 = super::digest(&fs::read(&archive)?);
        let source = directory.join("source");
        fs::create_dir(&source)?;
        super::checked(
            Command::new("tar")
                .arg("-xf")
                .arg(&archive)
                .arg("-C")
                .arg(&source),
        )?;
        let mut cargo = Command::new("cargo");
        cargo
            .args([
                "build",
                "--locked",
                "--release",
                "--bin",
                "lm-resizer",
                "--target-dir",
            ])
            .arg(cache)
            .current_dir(&source);
        for name in [
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_TARGET_DIR",
        ] {
            cargo.env_remove(name);
        }
        let output = cargo.output()?;
        fs::write(directory.join("build.stdout"), &output.stdout)?;
        fs::write(directory.join("build.stderr"), &output.stderr)?;
        ensure!(
            output.status.success(),
            "CLI build failed; see artifact build.stderr"
        );
        let filename = if cfg!(windows) {
            "lm-resizer.exe"
        } else {
            "lm-resizer"
        };
        let binary = directory.join(filename);
        fs::copy(cache.join("release").join(filename), &binary)?;
        let rustc = String::from_utf8(super::checked(Command::new("rustc").arg("-Vv"))?.stdout)?;
        let provenance = Provenance {
            commit,
            tree,
            binary_sha256: super::digest(&fs::read(&binary)?),
            archive_sha256,
            rustc,
            build_command: build_command(),
        };
        fs::write(sidecar(&binary), serde_json::to_vec_pretty(&provenance)?)?;
        Self::load(&binary, repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_changed_binary_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let binary = directory.path().join("cli");
        fs::write(&binary, b"original").unwrap();
        let artifact = Artifact {
            binary: binary.clone(),
            provenance: Provenance {
                commit: String::new(),
                tree: String::new(),
                binary_sha256: super::super::digest(b"original"),
                archive_sha256: String::new(),
                rustc: String::new(),
                build_command: build_command(),
            },
        };
        artifact.verify().unwrap();
        fs::write(&binary, b"replacement").unwrap();
        assert!(artifact.verify().is_err());
    }
}
