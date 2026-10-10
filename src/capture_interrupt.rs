//! Process-group interruption relay and incremental recovery for command capture.
//!
//! Noninteractive producers belong to their own process group. Unix producers
//! that can reach a terminal (terminal stdin or a controlling terminal) keep
//! the caller's foreground group. While a producer runs,
//! catchable console/process signals received by lm-resizer are forwarded to
//! that group, leaving the parent alive long enough to drain the shared pipe
//! and finish the partial view. Capture bytes are also appended to a visible
//! `.log` file as they arrive, so an uncatchable parent termination still
//! leaves the bytes written before the termination recoverable with `tee`.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_PARTIAL_TEE: AtomicU64 = AtomicU64::new(0);

/// A tee file written and flushed incrementally during capture.
///
/// Failure to create or update state must never prevent the producer from
/// running. `None` state therefore turns every operation into a no-op.
pub struct DurableTee {
    file: Option<File>,
    path: Option<PathBuf>,
    complete: bool,
}

impl DurableTee {
    pub fn create() -> Self {
        if std::env::var("LM_RESIZER_TEE").ok().as_deref() == Some("0") {
            return Self::disabled();
        }
        let Ok(state_dir) = crate::default_state_dir() else {
            crate::warn_state_unwritable(&crate::state_path_for_warning(None));
            return Self::disabled();
        };
        let tee_dir = state_dir.join("tee");
        if crate::create_private_dir_all(&tee_dir).is_err() {
            crate::warn_state_unwritable(&state_dir);
            return Self::disabled();
        }
        let epoch = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let serial = NEXT_PARTIAL_TEE.fetch_add(1, Ordering::Relaxed);
        let path = tee_dir.join(format!(
            "partial-{}-{epoch:032x}-{serial:016x}.log",
            std::process::id()
        ));
        let mut options = std::fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = match options.open(&path) {
            Ok(file) => Some(file),
            Err(_) => {
                crate::warn_state_unwritable(&state_dir);
                None
            }
        };
        Self {
            path: file.as_ref().map(|_| path),
            complete: file.is_some(),
            file,
        }
    }

    fn disabled() -> Self {
        Self {
            file: None,
            path: None,
            complete: false,
        }
    }

    /// Persist one capture chunk before it is exposed to filtering.
    pub fn append(&mut self, bytes: &[u8]) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if file.write_all(bytes).and_then(|()| file.flush()).is_err() {
            if let Some(path) = &self.path {
                crate::warn_state_unwritable(path);
            }
            self.file = None;
            self.complete = false;
        }
    }

    /// Close the incremental file and give it the regular content-addressed
    /// tee name. The returned hint has the same shape as normal exec reports.
    pub fn finish(mut self, raw: &[u8]) -> Option<String> {
        let path = self.path.take()?;
        if let Some(file) = self.file.take() {
            if file.sync_all().is_err() {
                crate::warn_state_unwritable(&path);
            }
        }
        if !self.complete {
            return path
                .file_stem()
                .and_then(|name| name.to_str())
                .map(|name| format!("[raw: {name}]"));
        }
        let digest = format!("{:x}", Sha256::digest(raw));
        let final_path = path.with_file_name(format!("{digest}.log"));
        if final_path.exists() {
            let _ = std::fs::remove_file(&path);
        } else if std::fs::rename(&path, &final_path).is_err() {
            crate::warn_state_unwritable(&path);
            return path
                .file_stem()
                .and_then(|name| name.to_str())
                .map(|name| format!("[raw: {name}]"));
        }
        Some(format!("[raw: {}]", &digest[..12]))
    }
}

/// Configure a producer before spawn so its descendants share a process
/// group that can be interrupted without signalling unrelated processes.
pub fn configure_process_group(command: &mut Command) -> bool {
    platform::configure_process_group(command)
}

/// Install the relay before spawning the producer, closing the race where an
/// interruption could otherwise terminate lm-resizer between spawn and handler
/// installation. Call [`InterruptionGuard::set_child`] immediately after spawn.
pub fn relay_interruptions(grouped: bool) -> std::io::Result<InterruptionGuard> {
    platform::install(grouped).map(InterruptionGuard)
}

pub struct InterruptionGuard(platform::Guard);

impl InterruptionGuard {
    pub fn set_child(&self, child: &Child) -> std::io::Result<()> {
        self.0.set_child(child.id())
    }
}

#[cfg(unix)]
mod platform {
    use std::io::{self, IsTerminal};
    use std::os::raw::c_int;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::{Mutex, MutexGuard};

    const SIGHUP: c_int = 1;
    const SIGINT: c_int = 2;
    const SIGQUIT: c_int = 3;
    const SIGTERM: c_int = 15;
    const SIG_ERR: usize = usize::MAX;
    // Negative: producer group; positive: foreground child only.
    static CHILD_TARGET: AtomicI32 = AtomicI32::new(0);
    static PENDING_SIGNAL: AtomicI32 = AtomicI32::new(0);
    static SIGNAL_LOCK: Mutex<()> = Mutex::new(());

    unsafe extern "C" {
        fn kill(pid: c_int, signal: c_int) -> c_int;
        fn signal(signal: c_int, handler: usize) -> usize;
    }

    extern "C" fn forward(signal_number: c_int) {
        // Publish first so `set_child` can observe a signal delivered in the
        // small window between handler installation and process-id handoff.
        PENDING_SIGNAL.store(signal_number, Ordering::Release);
        let target = CHILD_TARGET.load(Ordering::Acquire);
        if target != 0 {
            let pending = PENDING_SIGNAL.swap(0, Ordering::AcqRel);
            if pending > 0 {
                unsafe {
                    kill(target, pending);
                }
            }
        }
    }

    pub struct Guard {
        previous: [(c_int, usize); 4],
        grouped: bool,
        _lock: MutexGuard<'static, ()>,
    }

    pub fn configure_process_group(command: &mut Command) -> bool {
        // A new group would be behind the controlling terminal: reading
        // inherited stdin, or /dev/tty as sudo, ssh and password prompts do
        // even when stdin is a pipe, would stop the producer with SIGTTIN.
        // Keep Unix terminal and job-control behavior in the caller's
        // foreground group whenever a controlling terminal exists. Opening
        // /dev/tty fails without one and cannot acquire one.
        if std::io::stdin().is_terminal() || std::fs::File::open("/dev/tty").is_ok() {
            return false;
        }
        // Preserve Rust's posix_spawn path and its ENOEXEC launch error.
        // A pre_exec callback forces execvp, which executes invalid images
        // through /bin/sh instead of reporting the failed launch.
        command.process_group(0);
        true
    }

    pub fn install(grouped: bool) -> io::Result<Guard> {
        let lock = SIGNAL_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        CHILD_TARGET.store(0, Ordering::SeqCst);
        PENDING_SIGNAL.store(0, Ordering::SeqCst);
        let mut previous = [(0, 0); 4];
        for index in 0..previous.len() {
            let signal_number = [SIGHUP, SIGINT, SIGQUIT, SIGTERM][index];
            let old = unsafe { signal(signal_number, forward as *const () as usize) };
            if old == SIG_ERR {
                CHILD_TARGET.store(0, Ordering::SeqCst);
                for &(installed_signal, installed_handler) in &previous[..index] {
                    unsafe {
                        signal(installed_signal, installed_handler);
                    }
                }
                return Err(io::Error::last_os_error());
            }
            previous[index] = (signal_number, old);
        }
        Ok(Guard {
            previous,
            grouped,
            _lock: lock,
        })
    }

    impl Guard {
        pub fn set_child(&self, child_id: u32) -> io::Result<()> {
            let child_group = i32::try_from(child_id).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "child pid exceeds i32")
            })?;
            let target = if self.grouped {
                -child_group
            } else {
                child_group
            };
            CHILD_TARGET.store(target, Ordering::Release);
            let pending = PENDING_SIGNAL.swap(0, Ordering::AcqRel);
            if pending > 0 {
                unsafe {
                    kill(target, pending);
                }
            }
            Ok(())
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            CHILD_TARGET.store(0, Ordering::SeqCst);
            PENDING_SIGNAL.store(0, Ordering::SeqCst);
            for &(signal_number, previous) in &self.previous {
                unsafe {
                    signal(signal_number, previous);
                }
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::os::raw::c_int;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Mutex, MutexGuard};
    use std::thread::JoinHandle;

    const CTRL_C_EVENT: u32 = 0;
    const CTRL_BREAK_EVENT: u32 = 1;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    static CHILD_GROUP: AtomicU32 = AtomicU32::new(0);
    static PENDING_EVENT: AtomicU32 = AtomicU32::new(0);
    static RELAY_ACTIVE: AtomicBool = AtomicBool::new(false);
    static SIGNAL_LOCK: Mutex<()> = Mutex::new(());

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(
            handler: Option<unsafe extern "system" fn(u32) -> c_int>,
            add: c_int,
        ) -> c_int;
        fn GenerateConsoleCtrlEvent(ctrl_event: u32, process_group_id: u32) -> c_int;
    }

    unsafe extern "system" fn forward(ctrl_event: u32) -> c_int {
        if ctrl_event != CTRL_C_EVENT && ctrl_event != CTRL_BREAK_EVENT {
            return 0;
        }
        // Console callbacks have a restricted execution environment. Record
        // the event even before the child id is published; the relay thread
        // delivers it once `set_child` closes that small spawn window.
        PENDING_EVENT.store(ctrl_event + 1, Ordering::Release);
        1
    }

    pub struct Guard {
        _lock: MutexGuard<'static, ()>,
        relay: Option<JoinHandle<()>>,
    }

    pub fn configure_process_group(command: &mut Command) -> bool {
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
        true
    }

    pub fn install(_grouped: bool) -> io::Result<Guard> {
        let lock = SIGNAL_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        CHILD_GROUP.store(0, Ordering::SeqCst);
        PENDING_EVENT.store(0, Ordering::SeqCst);
        // Register our handler before restoring normal Ctrl-C reception so a
        // failure cannot expose the parent to the default termination handler.
        if unsafe { SetConsoleCtrlHandler(Some(forward), 1) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // Some process launchers ignore Ctrl-C and Windows lets descendants
        // inherit that flag. Explicitly restore normal reception.
        if unsafe { SetConsoleCtrlHandler(None, 0) } == 0 {
            let error = io::Error::last_os_error();
            unsafe {
                SetConsoleCtrlHandler(Some(forward), 0);
            }
            return Err(error);
        }
        RELAY_ACTIVE.store(true, Ordering::SeqCst);
        let relay = std::thread::spawn(|| {
            while RELAY_ACTIVE.load(Ordering::Acquire) {
                let pending = PENDING_EVENT.swap(0, Ordering::AcqRel);
                if pending != 0 {
                    let group = CHILD_GROUP.load(Ordering::Acquire);
                    if group != 0 {
                        // CTRL_C cannot be scoped to a process group and a new
                        // group initially ignores it. CTRL_BREAK is targetable
                        // and follows the same console interruption path.
                        unsafe {
                            GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, group);
                        }
                    } else {
                        // `spawn` succeeded but `set_child` has not published
                        // the process-group id yet. Keep the event pending.
                        PENDING_EVENT.store(pending, Ordering::Release);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        });
        Ok(Guard {
            _lock: lock,
            relay: Some(relay),
        })
    }

    impl Guard {
        pub fn set_child(&self, child_id: u32) -> io::Result<()> {
            CHILD_GROUP.store(child_id, Ordering::SeqCst);
            Ok(())
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            RELAY_ACTIVE.store(false, Ordering::Release);
            if let Some(relay) = self.relay.take() {
                let _ = relay.join();
            }
            CHILD_GROUP.store(0, Ordering::SeqCst);
            unsafe {
                SetConsoleCtrlHandler(Some(forward), 0);
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use std::io;
    use std::process::Command;

    pub struct Guard;
    pub fn configure_process_group(_command: &mut Command) -> bool {
        false
    }
    pub fn install(_grouped: bool) -> io::Result<Guard> {
        Ok(Guard)
    }
    impl Guard {
        pub fn set_child(&self, _child_id: u32) -> io::Result<()> {
            Ok(())
        }
    }
}
