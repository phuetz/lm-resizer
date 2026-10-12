//! Unix terminal regressions: a producer reading inherited stdin or the
//! controlling terminal must stay in the foreground group. Run by the Unix
//! pilot; no Windows emulation.
#![cfg(unix)]

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::raw::c_int;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
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
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
}

#[cfg(target_os = "linux")]
const TIOCSCTTY: std::os::raw::c_ulong = 0x540e;
#[cfg(not(target_os = "linux"))]
const TIOCSCTTY: std::os::raw::c_ulong = 0x20007461;

const F_SETFD: c_int = 2;
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
const FD_CLOEXEC: c_int = 1;
#[cfg(target_os = "linux")]
const O_NONBLOCK: c_int = 0o4000;
#[cfg(not(target_os = "linux"))]
const O_NONBLOCK: c_int = 0x4;

/// Past this, the watchdog kills the case in flight and fails the test: CI
/// must get a verdict even from a child the kernel will not let die.
const TEST_DEADLINE: Duration = Duration::from_secs(30);

/// One pty at a time: the slave stays inheritable for the child under test,
/// and a test running in parallel would hand it to its own children too.
static ONE_PTY: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug)]
enum Input {
    /// stdin is the controlling terminal itself.
    Terminal,
    /// stdin is a pipe, as in `echo | lm-resizer exec -- ssh …`.
    Pipe,
    /// stdin is /dev/null, as for a command launched by an agent.
    Null,
}

/// The case in flight, for the watchdog's kill and message.
#[derive(Default)]
struct Flight {
    case: String,
    step: &'static str,
    lm_resizer: Option<i32>,
    ready: Option<PathBuf>,
    terminal: Option<Arc<Mutex<Vec<u8>>>>,
}

#[derive(Default)]
struct Watch {
    flight: Mutex<Flight>,
    abandoned: AtomicBool,
}

impl Watch {
    fn flight(&self) -> MutexGuard<'_, Flight> {
        self.flight.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn step(&self, step: &'static str) {
        self.flight().step = step;
    }
}

/// Run `body` on its own thread and fail the test after `TEST_DEADLINE`.
/// The test thread never waits on a child: on macOS a process stuck in its
/// own exit ignores SIGKILL, so only abandoning the wait gives a verdict.
/// The test binary exits afterwards, which releases such a process.
fn within_deadline(body: impl FnOnce(&Watch) + Send + 'static) {
    let _one_pty = ONE_PTY.lock().unwrap_or_else(PoisonError::into_inner);
    let watch = Arc::new(Watch::default());
    let (done, finished) = mpsc::channel();
    let worker = {
        let watch = Arc::clone(&watch);
        std::thread::spawn(move || {
            body(&watch);
            let _ = done.send(());
        })
    };
    match finished.recv_timeout(TEST_DEADLINE) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => {
            if let Err(panic) = worker.join() {
                std::panic::resume_unwind(panic);
            }
        }
        Err(RecvTimeoutError::Timeout) => {
            watch.abandoned.store(true, Ordering::Release);
            let flight = watch.flight();
            let killed = match (flight.lm_resizer, &flight.ready) {
                (Some(lm_resizer), Some(ready)) => {
                    kill_case(lm_resizer, ready);
                    format!("killed lm-resizer {lm_resizer} and its producer")
                }
                _ => "no lm-resizer to kill".to_owned(),
            };
            let terminal = flight.terminal.as_ref().map_or_else(String::new, |output| {
                String::from_utf8_lossy(&output.lock().unwrap_or_else(PoisonError::into_inner))
                    .into_owned()
            });
            let message = format!(
                "no verdict after {TEST_DEADLINE:?} ({}; {}): {killed}; terminal output so far \
                 {terminal:?}",
                flight.case, flight.step
            );
            drop(flight);
            panic!("{message}");
        }
    }
}

/// Kill lm-resizer, its group and the producer, even if SIGTTIN stopped the
/// old-regression producer in a separate group.
fn kill_case(lm_resizer: i32, ready: &Path) {
    if let Ok(pid) = std::fs::read_to_string(ready) {
        if let Ok(pid) = pid.parse::<i32>() {
            unsafe {
                kill(-pid, 9);
                kill(pid, 9);
            }
        }
    }
    unsafe {
        kill(-lm_resizer, 9);
        kill(lm_resizer, 9);
    }
}

/// Read the pty master like a terminal emulator does. On macOS a session
/// leader cannot finish exiting while its terminal holds unread output: XNU
/// drains it in `ttywait` from `proc_exit`, with no deadline, after marking
/// the process as exiting, which discards every signal. The echo of the
/// typed `hello` was enough to leave lm-resizer there and the test in
/// `wait` for hours. Linux never drains a terminal on exit.
struct TerminalReader {
    output: Arc<Mutex<Vec<u8>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl TerminalReader {
    /// `master` must be non-blocking, so that the reader stops on request
    /// even if a leftover slave descriptor keeps the end from ever coming.
    fn start(master: &File) -> Self {
        let mut master = master.try_clone().unwrap();
        let output = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (output, stop) = (Arc::clone(&output), Arc::clone(&stop));
            std::thread::spawn(move || {
                let mut buffer = [0; 4096];
                loop {
                    match master.read(&mut buffer) {
                        Ok(read) if read > 0 => output
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .extend_from_slice(&buffer[..read]),
                        _ if stop.load(Ordering::Acquire) => break,
                        _ => std::thread::sleep(Duration::from_millis(5)),
                    }
                }
            })
        };
        Self {
            output,
            stop,
            thread: Some(thread),
        }
    }

    fn finish(mut self) -> String {
        self.stop_thread();
        String::from_utf8_lossy(&self.output.lock().unwrap_or_else(PoisonError::into_inner))
            .into_owned()
    }

    fn stop_thread(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for TerminalReader {
    fn drop(&mut self) {
        self.stop_thread();
    }
}

/// Run `read_line` under lm-resizer in a new session whose controlling
/// terminal is a fresh pty, type `hello` on that terminal and require the
/// producer to finish within five seconds in every exec mode.
fn assert_terminal_read_finishes(watch: &Watch, input: Input, read_line: &str) {
    for option in [None, Some("--raw-on-failure"), Some("--stream")] {
        if watch.abandoned.load(Ordering::Acquire) {
            return;
        }
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
        // Only the test holds the master, as a terminal emulator does.
        unsafe {
            let flags = fcntl(master.as_raw_fd(), F_GETFL);
            assert!(
                fcntl(master.as_raw_fd(), F_SETFD, FD_CLOEXEC) != -1
                    && flags != -1
                    && fcntl(master.as_raw_fd(), F_SETFL, flags | O_NONBLOCK) != -1,
                "{}",
                std::io::Error::last_os_error()
            );
        }
        let terminal_reader = TerminalReader::start(&master);
        *watch.flight() = Flight {
            case: format!("stdin {input:?}, option {option:?}"),
            step: "starting lm-resizer",
            lm_resizer: None,
            ready: Some(ready.clone()),
            terminal: Some(Arc::clone(&terminal_reader.output)),
        };
        let terminal = slave.as_raw_fd();
        let mut command = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
        command
            .env("LM_RESIZER_STATE_DIR", state.path().join("state"))
            .stdin(match input {
                Input::Terminal => Stdio::from(slave.try_clone().unwrap()),
                Input::Pipe => Stdio::piped(),
                Input::Null => Stdio::null(),
            })
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
                &format!("printf '%s' \"$$\" > \"$1\"; {read_line}; printf 'got=%s\\n' \"$x\""),
                "terminal-producer",
            ])
            .arg(&ready);
        // SAFETY: only async-signal-safe libc calls in the child before exec.
        // `terminal` stays open in the parent until after spawn.
        unsafe {
            command.pre_exec(move || {
                if setsid() == -1 || ioctl(terminal, TIOCSCTTY, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command.spawn().unwrap();
        watch.flight().lm_resizer = Some(child.id() as i32);
        drop(slave);
        watch.step("waiting for the producer to start");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ready.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        master.write_all(b"hello\n").unwrap();
        watch.step("waiting for lm-resizer to exit");
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
            kill_case(child.id() as i32, &ready);
            let _ = child.kill();
        }
        watch.step("reaping lm-resizer");
        let output = child.wait_with_output().unwrap();
        watch.flight().lm_resizer = None;
        let terminal_output = terminal_reader.finish();
        assert!(
            finished,
            "terminal read hung (possible SIGTTIN), stdin {input:?}, option {option:?}: \
             {output:?}, terminal {terminal_output:?}"
        );
        assert!(
            output.status.success(),
            "{output:?}, terminal {terminal_output:?}"
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("got=hello"),
            "{output:?}, terminal {terminal_output:?}"
        );
    }
}

#[test]
fn inherited_terminal_input_finishes_in_every_exec_mode() {
    within_deadline(|watch| assert_terminal_read_finishes(watch, Input::Terminal, "read x"));
}

#[test]
fn controlling_terminal_read_finishes_when_stdin_is_not_a_terminal() {
    // sudo, ssh, gpg and git or npm password prompts read /dev/tty even when
    // stdin is a pipe or /dev/null.
    within_deadline(|watch| {
        for input in [Input::Pipe, Input::Null] {
            assert_terminal_read_finishes(watch, input, "read x < /dev/tty");
        }
    });
}
