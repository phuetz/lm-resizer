//! Unix terminal regression: a producer reading inherited stdin must stay
//! in the foreground group. Run by the Unix pilot; no Windows emulation.
#![cfg(unix)]

use std::fs::File;
use std::io::Write;
use std::os::fd::{FromRawFd, RawFd};
use std::os::raw::c_int;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg_attr(target_os = "linux", link(name = "util"))]
unsafe extern "C" {
    fn openpty(
        master: *mut c_int,
        slave: *mut c_int,
        name: *mut u8,
        termios: *const u8,
        winsize: *const u8,
    ) -> c_int;
    fn setsid() -> c_int;
    fn ioctl(fd: c_int, request: std::os::raw::c_ulong, ...) -> c_int;
    fn kill(pid: c_int, signal: c_int) -> c_int;
}

#[cfg(target_os = "linux")]
const TIOCSCTTY: std::os::raw::c_ulong = 0x540e;
#[cfg(not(target_os = "linux"))]
const TIOCSCTTY: std::os::raw::c_ulong = 0x20007461;

#[test]
fn inherited_terminal_input_finishes_in_every_exec_mode() {
    for option in [None, Some("--raw-on-failure"), Some("--stream")] {
        let state = tempfile::tempdir().unwrap();
        let ready = state.path().join("producer.pid");
        let (mut master, mut slave): (RawFd, RawFd) = (-1, -1);
        // SAFETY: openpty initializes two owned file descriptors on success.
        assert_eq!(
            unsafe {
                openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );
        let mut master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
        command
            .env("LM_RESIZER_STATE_DIR", state.path().join("state"))
            .stdin(slave)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(["exec", "--json"]);
        if let Some(option) = option {
            command.arg(option);
        }
        command
            .args([
                "--",
                "sh",
                "-c",
                "printf '%s' \"$$\" > \"$1\"; read x; printf 'got=%s\\n' \"$x\"",
                "terminal-producer",
            ])
            .arg(&ready);
        // SAFETY: only async-signal-safe libc calls in the child before exec.
        unsafe {
            command.pre_exec(|| {
                if setsid() == -1 || ioctl(0, TIOCSCTTY, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        master.write_all(b"hello\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut finished = false;
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                finished = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if !finished {
            // Reap the old-regression producer even if SIGTTIN stopped it in
            // a separate group. Also clean up the foreground session group.
            if let Ok(pid) = std::fs::read_to_string(&ready) {
                if let Ok(pid) = pid.parse::<i32>() {
                    unsafe {
                        kill(-pid, 9);
                        kill(pid, 9);
                    }
                }
            }
            unsafe {
                kill(-(child.id() as i32), 9);
            }
            let _ = child.kill();
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            finished,
            "terminal read hung (possible SIGTTIN): {output:?}"
        );
        assert!(output.status.success(), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("got=hello"),
            "{output:?}"
        );
    }
}
