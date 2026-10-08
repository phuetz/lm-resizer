//! External interruption keeps partial output recoverable and stops producers.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn isolated_command(state: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    let profile = state.join("profile");
    std::fs::create_dir_all(&profile).unwrap();
    command
        .env("LM_RESIZER_STATE_DIR", state.join("state"))
        .env("HOME", &profile)
        .env("USERPROFILE", &profile);
    command
}

fn sandbox() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(".omx/windows-fix/tests");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}

fn wait_for_file(path: &Path, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        if let Some(status) = child.try_wait().unwrap() {
            panic!("lm-resizer exited before producer became ready: {status}");
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "producer readiness file was not created: {}",
        path.display()
    );
}

#[cfg(unix)]
fn wait_for_exit(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    panic!("lm-resizer did not finish draining after interruption");
}

fn tee_bytes(state: &Path) -> Vec<u8> {
    let tee = state.join("state").join("tee");
    let mut bytes = Vec::new();
    for entry in std::fs::read_dir(&tee)
        .unwrap_or_else(|error| panic!("partial tee directory missing: {error}"))
    {
        let path = entry.unwrap().path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("log") {
            bytes.extend(std::fs::read(path).unwrap());
        }
    }
    bytes
}

fn wait_for_partial_tee(state: &Path, marker: &[u8]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let tee_dir = state.join("state").join("tee");
        if tee_dir.exists()
            && tee_bytes(state)
                .windows(marker.len())
                .any(|window| window == marker)
        {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("incremental tee did not contain bytes already emitted by producer");
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::os::raw::c_int;

    const SIGTERM: c_int = 15;
    const SIGNAL_ZERO: c_int = 0;

    unsafe extern "C" {
        fn kill(pid: c_int, signal: c_int) -> c_int;
    }

    fn signal(pid: u32, number: c_int) -> bool {
        unsafe { kill(pid as c_int, number) == 0 }
    }

    #[test]
    fn signal_to_lm_resizer_preserves_partial_tee_and_stops_descendant() {
        let sandbox = sandbox();
        let ready = sandbox.path().join("ready");
        let descendant_pid = sandbox.path().join("descendant.pid");
        let script = format!(
            "printf '%s' \"$$\" > '{}'; printf ready > '{}'; \
             printf 'ERROR partial diagnostic\\n'; while :; do sleep 1; done",
            descendant_pid.display(),
            ready.display()
        );
        let mut command = isolated_command(sandbox.path());
        command
            .args(["exec", "--json", "--", "sh", "-c", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut lm_resizer = command.spawn().unwrap();
        wait_for_file(&ready, &mut lm_resizer);
        let producer_pid: u32 = std::fs::read_to_string(&descendant_pid)
            .unwrap()
            .parse()
            .unwrap();

        assert!(
            signal(lm_resizer.id(), SIGTERM),
            "could not signal lm-resizer"
        );
        let status = wait_for_exit(&mut lm_resizer);
        assert_eq!(status.code(), Some(143), "signal exit convention changed");

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline && signal(producer_pid, SIGNAL_ZERO) {
            thread::sleep(Duration::from_millis(20));
        }
        let orphaned = signal(producer_pid, SIGNAL_ZERO);
        if orphaned {
            // Cleanup makes the regression test safe when deliberately run
            // against the old implementation where the child is orphaned.
            let _ = signal(producer_pid, SIGTERM);
        }
        assert!(
            !orphaned,
            "producer descendant remained alive after parent signal"
        );
        assert!(
            tee_bytes(sandbox.path())
                .windows(b"ERROR partial diagnostic".len())
                .any(|window| window == b"ERROR partial diagnostic"),
            "bytes emitted before SIGTERM were not recoverable through tee"
        );
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::ffi::c_void;
    use std::sync::OnceLock;

    const PROCESS_TERMINATE: u32 = 0x0001;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> *mut c_void;
        fn TerminateProcess(process: *mut c_void, exit_code: u32) -> i32;
    }

    fn terminate(process_id: u32) {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, process_id);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }

    fn ctrl_c_driver() -> &'static Path {
        static DRIVER: OnceLock<std::path::PathBuf> = OnceLock::new();
        DRIVER.get_or_init(|| {
            let root =
                Path::new(env!("CARGO_MANIFEST_DIR")).join(".omx/windows-fix/interruption-driver");
            std::fs::create_dir_all(&root).unwrap();
            let executable = root.join("interruption-driver.exe");
            let compiled = Command::new("rustc")
                .args([
                    "--edition=2021",
                    "tests/fixtures/interruption_windows.rs",
                    "-o",
                ])
                .arg(&executable)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            executable
        })
    }

    #[test]
    fn console_interrupt_drains_partial_output_before_parent_exits() {
        // Windows needs a fresh console to deliver a real CTRL_C_EVENT. The
        // Rust driver allocates and hides one; TerminateProcess cannot prove
        // the required Ctrl-C drain behavior because it is not interceptable.
        for mode in [None, Some("--raw-on-failure"), Some("--stream")] {
            let sandbox = sandbox();
            let ready = sandbox.path().join("ready.txt");
            let output = Command::new(ctrl_c_driver())
                .arg("drive")
                .arg(env!("CARGO_BIN_EXE_lm-resizer"))
                .arg(sandbox.path().join("state"))
                .arg(&ready)
                .arg(ctrl_c_driver())
                .arg(mode.unwrap_or("-"))
                .env("HOME", sandbox.path().join("profile"))
                .env("USERPROFILE", sandbox.path().join("profile"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(0xC000_013Au32 as i32),
                "mode {mode:?} did not preserve the producer Ctrl-C status: {output:?}"
            );
            let visible = String::from_utf8_lossy(&output.stdout);
            assert!(
                visible.contains("ERROR partial diagnostic"),
                "mode {mode:?} did not emit the partial view after Ctrl-C: {visible}"
            );
            assert!(
                tee_bytes(sandbox.path())
                    .windows(b"ERROR partial diagnostic".len())
                    .any(|window| window == b"ERROR partial diagnostic"),
                "mode {mode:?} did not preserve bytes emitted before Ctrl-C"
            );
        }
    }

    #[test]
    fn forced_parent_stop_leaves_incremental_tee_in_all_exec_modes() {
        // TerminateProcess is deliberately uncatchable. This test therefore
        // proves durability of already captured chunks, independently of the
        // graceful console-control handler exercised above.
        for mode in [None, Some("--raw-on-failure"), Some("--stream")] {
            let sandbox = sandbox();
            let ready = sandbox.path().join("ready.txt");
            let mut command = isolated_command(sandbox.path());
            command.args(["exec", "--json"]);
            if let Some(mode) = mode {
                command.arg(mode);
            }
            command
                .arg("--")
                .arg(ctrl_c_driver())
                .arg("producer")
                .arg(&ready)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut lm_resizer = command.spawn().unwrap();
            wait_for_file(&ready, &mut lm_resizer);
            wait_for_partial_tee(sandbox.path(), b"ERROR partial diagnostic");
            let producer_pid = std::fs::read_to_string(&ready)
                .unwrap()
                .parse::<u32>()
                .unwrap();
            lm_resizer.kill().unwrap();
            lm_resizer.wait().unwrap();
            terminate(producer_pid);

            assert!(
                tee_bytes(sandbox.path())
                    .windows(b"ERROR partial diagnostic".len())
                    .any(|window| window == b"ERROR partial diagnostic"),
                "mode {mode:?} lost bytes captured before forced parent stop"
            );
        }
    }
}
