#[cfg(windows)]
mod windows {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn repository_root() -> PathBuf {
        option_env!("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().expect("current repository directory"))
    }

    fn sandbox(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = repository_root()
            .join(".omx/windows-fix/scripts")
            .join(format!("{name}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).expect("create sandbox");
        path
    }

    fn run_isolated_powershell(sandbox: &Path, harness: &Path) -> Output {
        let profile = sandbox.join("profile");
        let app_data = profile.join("AppData/Roaming");
        let local_app_data = profile.join("AppData/Local");
        let temp = sandbox.join("temp");
        for directory in [&profile, &app_data, &local_app_data, &temp] {
            fs::create_dir_all(directory).expect("create isolated PowerShell directory");
        }

        Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(harness)
            .env("HOME", &profile)
            .env("USERPROFILE", &profile)
            .env("APPDATA", &app_data)
            .env("LOCALAPPDATA", &local_app_data)
            .env("TMP", &temp)
            .env("TEMP", &temp)
            .env(
                "PSModulePath",
                r"C:\Windows\System32\WindowsPowerShell\v1.0\Modules",
            )
            .output()
            .expect("run isolated Windows PowerShell contract harness")
    }

    fn marked_section<'a>(text: &'a str, start_marker: &str, end_marker: &str) -> Option<&'a str> {
        let start = text.find(start_marker)?;
        let end = text[start..].find(end_marker)? + start + end_marker.len();
        Some(&text[start..end])
    }

    fn braced_block_from<'a>(text: &'a str, needle: &str) -> &'a str {
        let start = text.find(needle).expect("historical PATH update block");
        let mut depth = 0usize;
        for (offset, character) in text[start..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth = depth.checked_sub(1).expect("balanced PATH block");
                    if depth == 0 {
                        return &text[start..start + offset + character.len_utf8()];
                    }
                }
                _ => {}
            }
        }
        panic!("unterminated historical PATH update block")
    }

    /// Windows-only because this executes the shipped PowerShell PATH helper.
    /// The getter/setter are injected mocks, so the real user profile is untouched.
    #[test]
    fn installer_updates_user_path_when_session_path_already_contains_destination() {
        let install_path = std::env::var_os("LM_RESIZER_TEST_INSTALL_SOURCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| repository_root().join("install.ps1"));
        let install = fs::read_to_string(&install_path).expect("read install.ps1 contract source");
        let start_marker = "# LM_RESIZER_PATH_HELPER_START";
        let end_marker = "# LM_RESIZER_PATH_HELPER_END";
        let contract_action = if let Some(helper) =
            marked_section(&install, start_marker, end_marker)
        {
            format!(
                r#"{helper}
Add-LmResizerInstallPath -Directory $destination `
  -GetUserPath {{ $script:mockUserPath }} `
  -SetUserPath {{ param($value) $script:writtenUserPath = $value }}
"#
            )
        } else {
            let historical = braced_block_from(
                &install,
                "if ($env:LM_RESIZER_SKIP_PATH_UPDATE -ne \"1\") {",
            )
            .replace(
                "[Environment]::GetEnvironmentVariable(\"Path\", \"User\")",
                "& $GetUserPath",
            )
            .replace(
                "[Environment]::SetEnvironmentVariable(\"Path\", \"$userPath;$destDir\".TrimStart(';'), \"User\")",
                "& $SetUserPath (\"$userPath;$destDir\".TrimStart(';'))",
            );
            format!(
                r#"$destDir = $destination
$GetUserPath = {{ $script:mockUserPath }}
$SetUserPath = {{ param($value) $script:writtenUserPath = $value }}
{historical}
"#
            )
        };

        let sandbox = sandbox("path-contract");
        let harness = sandbox.join("path-contract.ps1");
        let destination = sandbox.join("bin");
        let escaped_destination = destination.display().to_string().replace('\'', "''");
        let script = format!(
            r#"
$destination = '{escaped_destination}'
$env:PATH = "$destination;C:\Windows\System32"
$env:LM_RESIZER_SKIP_PATH_UPDATE = '0'
$script:mockUserPath = 'C:\existing'
$script:writtenUserPath = $null
{contract_action}
if ($script:writtenUserPath -ne "C:\existing;$destination") {{
  throw "expected persistent PATH update, got '$script:writtenUserPath'"
}}
"#
        );
        fs::write(&harness, script).expect("write PowerShell harness");

        let output = run_isolated_powershell(&sandbox, &harness);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        fs::remove_dir_all(&sandbox).expect("remove sandbox");
        assert!(
            output.status.success(),
            "PowerShell PATH contract failed for {}:\nstdout:\n{}\nstderr:\n{}",
            install_path.display(),
            stdout,
            stderr
        );
    }

    /// Windows-only because this executes the shipped signing script end to end.
    /// Get-Command resolves signtool to a local byte-appending mock, so no key is used.
    #[test]
    fn signing_rebuilds_windows_zip_and_sidecar_from_signed_staging_tree() {
        let signing_path = std::env::var_os("LM_RESIZER_TEST_SIGN_SOURCE")
            .map(PathBuf::from)
            .unwrap_or_else(|| repository_root().join("scripts/sign-windows-release.ps1"));
        let signing = fs::read_to_string(&signing_path).expect("read signing contract source");
        let checksums =
            fs::read_to_string(repository_root().join("scripts/generate-checksums.ps1"))
                .expect("read generate-checksums.ps1");

        let sandbox = sandbox("signed-archive-contract");
        let fake_root = sandbox.join("release-root");
        let fake_scripts = fake_root.join("scripts");
        fs::create_dir_all(&fake_scripts).expect("create fake release scripts directory");
        fs::write(fake_scripts.join("sign-windows-release.ps1"), signing)
            .expect("copy signing script into fake release root");
        fs::write(fake_scripts.join("generate-checksums.ps1"), checksums)
            .expect("copy checksum script into fake release root");
        fs::write(
            fake_scripts.join("fake-sign.ps1"),
            r#"$target = $args[$args.Count - 1]
$before = [IO.File]::ReadAllBytes($target)
$marker = [byte[]](83, 73, 71, 78, 69, 68, 45, 66, 89, 45, 77, 79, 67, 75)
$after = New-Object byte[] ($before.Length + $marker.Length)
[Array]::Copy($before, 0, $after, 0, $before.Length)
[Array]::Copy($marker, 0, $after, $before.Length, $marker.Length)
[IO.File]::WriteAllBytes($target, $after)
$global:LASTEXITCODE = 0
"#,
        )
        .expect("write fake signtool");

        let harness = sandbox.join("signed-archive-contract.ps1");
        let escaped_root = fake_root.display().to_string().replace('\'', "''");
        let script = format!(
            r#"$root = '{escaped_root}'
$scripts = Join-Path $root 'scripts'
$global:fakeSignPath = Join-Path $scripts 'fake-sign.ps1'
function global:Get-Command {{
  param([string]$Name)
  if ($Name -eq 'signtool.exe') {{ return [pscustomobject]@{{ Source = $global:fakeSignPath }} }}
  throw "unexpected Get-Command call: $Name"
}}

$dist = Join-Path $root 'dist'
$stage = Join-Path $dist 'lm-resizer-0.2.6-windows-x86_64'
New-Item -ItemType Directory -Force -Path $stage | Out-Null
$unsignedBytes = [byte[]](77, 90, 0, 85, 78, 83, 73, 71, 78, 69, 68, 255)
$stagedExe = Join-Path $stage 'lm-resizer.exe'
[IO.File]::WriteAllBytes($stagedExe, $unsignedBytes)
$zip = "$stage.zip"
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip
[IO.File]::WriteAllText("$zip.sha256", ('0' * 64) + '  ' + [IO.Path]::GetFileName($zip))

& (Join-Path $scripts 'sign-windows-release.ps1') -CertificateThumbprint 'simulation-no-key'
if ($LASTEXITCODE -ne 0) {{ throw "signing script failed with $LASTEXITCODE" }}
$signedBytes = [IO.File]::ReadAllBytes($stagedExe)
if ([BitConverter]::ToString($signedBytes) -eq [BitConverter]::ToString($unsignedBytes)) {{
  throw 'fake signtool did not modify the staged executable'
}}

Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($zip)
try {{
  $entry = $archive.GetEntry('lm-resizer.exe')
  if (-not $entry) {{ throw 'rebuilt ZIP is missing lm-resizer.exe' }}
  $stream = $entry.Open()
  try {{
    $actualBytes = New-Object byte[] $entry.Length
    [void]$stream.Read($actualBytes, 0, $actualBytes.Length)
  }} finally {{ $stream.Dispose() }}
}} finally {{ $archive.Dispose() }}
if ([BitConverter]::ToString($actualBytes) -ne [BitConverter]::ToString($signedBytes)) {{
  throw 'ZIP still contains unsigned executable bytes after signing'
}}
$sha = [Security.Cryptography.SHA256]::Create()
$inputFile = [IO.File]::OpenRead($zip)
try {{
  $expectedHash = [BitConverter]::ToString($sha.ComputeHash($inputFile)).Replace('-', '').ToLowerInvariant()
}} finally {{
  $inputFile.Dispose()
  $sha.Dispose()
}}
$expectedLine = "$expectedHash  $([IO.Path]::GetFileName($zip))"
$actualLine = (Get-Content -LiteralPath "$zip.sha256" -Raw).Trim()
if ($actualLine -ne $expectedLine) {{ throw "stale ZIP sidecar: '$actualLine'" }}
"#
        );
        fs::write(&harness, script).expect("write PowerShell harness");

        let output = run_isolated_powershell(&sandbox, &harness);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        fs::remove_dir_all(&sandbox).expect("remove sandbox");
        assert!(
            output.status.success(),
            "PowerShell signed archive contract failed for {}:\nstdout:\n{}\nstderr:\n{}",
            signing_path.display(),
            stdout,
            stderr
        );
    }
}
