#![cfg(windows)]

use std::ffi::c_void;
use std::io::Write;
use std::os::raw::c_int;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const CTRL_C_EVENT: u32 = 0;
const PROCESS_TERMINATE: u32 = 0x0001;
const SW_HIDE: c_int = 0;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn AllocConsole() -> c_int;
    fn FreeConsole() -> c_int;
    fn GenerateConsoleCtrlEvent(ctrl_event: u32, process_group_id: u32) -> c_int;
    fn GetConsoleWindow() -> *mut c_void;
    fn OpenProcess(access: u32, inherit: c_int, process_id: u32) -> *mut c_void;
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> c_int>,
        add: c_int,
    ) -> c_int;
    fn TerminateProcess(process: *mut c_void, exit_code: u32) -> c_int;
    fn CloseHandle(handle: *mut c_void) -> c_int;
}

unsafe extern "system" fn keep_driver_alive(ctrl_event: u32) -> c_int {
    (ctrl_event == CTRL_C_EVENT) as c_int
}

#[link(name = "user32")]
unsafe extern "system" {
    fn ShowWindow(window: *mut c_void, command: c_int) -> c_int;
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|arg| arg.to_str()) {
        Some("producer") => producer(args.next().unwrap().into()),
        Some("drive") => drive(
            args.next().unwrap().into(),
            args.next().unwrap().into(),
            args.next().unwrap().into(),
            args.next().unwrap().into(),
            args.next().unwrap().to_string_lossy().into_owned(),
        ),
        _ => std::process::exit(2),
    }
}

fn producer(ready: PathBuf) {
    // With the fixed lm-resizer this process is a new group root and initially
    // ignores the broadcast CTRL_C_EVENT. The parent must relay a targeted
    // CTRL_BREAK_EVENT to end it after keeping itself alive for drainage.
    std::io::stderr()
        .write_all(b"ERROR partial diagnostic\r\n")
        .unwrap();
    std::io::stderr().flush().unwrap();
    std::fs::write(ready, std::process::id().to_string()).unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn drive(lm_resizer: PathBuf, state: PathBuf, ready: PathBuf, fixture: PathBuf, mode: String) {
    unsafe {
        FreeConsole();
        assert_ne!(
            AllocConsole(),
            0,
            "AllocConsole: {}",
            std::io::Error::last_os_error()
        );
        let window = GetConsoleWindow();
        if !window.is_null() {
            ShowWindow(window, SW_HIDE);
        }
        // Clear a possible inherited ignore flag before adding the driver's
        // process-local handler. This matches the recipe launcher, which
        // explicitly restored ordinary Ctrl-C reception before spawning LM.
        assert_ne!(SetConsoleCtrlHandler(None, 0), 0);
        // The driver must survive the event that it generates for its console.
        // A handler is process-local, unlike the inheritable NULL/TRUE
        // "ignore Ctrl-C" flag which would incorrectly exempt lm-resizer too.
        assert_ne!(SetConsoleCtrlHandler(Some(keep_driver_alive), 1), 0);
    }

    let mut command = Command::new(lm_resizer);
    command
        .env("LM_RESIZER_STATE_DIR", state)
        .args(["exec", "--json"]);
    if mode != "-" {
        command.arg(mode);
    }
    let mut child = command
        .arg("--")
        .arg(fixture)
        .arg("producer")
        .arg(&ready)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() && Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            panic!("lm-resizer exited before producer readiness");
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(ready.exists(), "producer readiness timeout");
    assert_ne!(
        unsafe { GenerateConsoleCtrlEvent(CTRL_C_EVENT, 0) },
        0,
        "GenerateConsoleCtrlEvent: {}",
        std::io::Error::last_os_error()
    );
    let producer_pid = std::fs::read_to_string(&ready)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut exited = false;
    while Instant::now() < deadline {
        if child.try_wait().unwrap().is_some() {
            exited = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    if !exited {
        // Cleanup makes a deliberately failing old implementation safe, but
        // the dedicated status prevents cleanup from manufacturing a pass.
        unsafe {
            let producer = OpenProcess(PROCESS_TERMINATE, 0, producer_pid);
            if !producer.is_null() {
                TerminateProcess(producer, 1);
                CloseHandle(producer);
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        eprintln!("lm-resizer did not exit after relaying Ctrl-C");
        std::process::exit(123);
    }
    let output = child.wait_with_output().unwrap();
    std::io::stdout().write_all(&output.stdout).unwrap();
    std::io::stderr().write_all(&output.stderr).unwrap();
    std::process::exit(output.status.code().unwrap_or(1));
}
