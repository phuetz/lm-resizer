//! One producer and one shared pipe for both output channels. Drain before
//! waiting, without a byte limit or per-stream reconstruction; EOF also covers
//! inherited writers that outlive the immediate producer.
use std::io::{Read, Write};
use std::process::{Command, Stdio};

pub struct Capture {
    pub raw: Vec<u8>,
    pub code: i32,
    pub launch_error: Option<String>,
}

/// Windows `cmd.exe /c <line>` re-parses its own command line: Rust's usual
/// `\"` escaping of embedded quotes is not understood by it and corrupts
/// quoted paths. Everything after `/c` (or `/k`) is therefore handed over as
/// a raw tail, exactly as the caller typed it.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_cmd_program(program: &std::path::Path) -> bool {
    program
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("cmd"))
}

/// Index of the first `/c` or `/k` switch (case-insensitive), when present.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn cmd_switch_index(args: &[String]) -> Option<usize> {
    args.iter()
        .position(|a| a.eq_ignore_ascii_case("/c") || a.eq_ignore_ascii_case("/k"))
}

/// Raw command line for the arguments following `/c`, rebuilt from `argv`.
/// Only a fallback: a Windows PowerShell 5.1 caller strips the inner quotes of
/// an argument before it reaches `argv`, so the real line is preferred.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn cmd_raw_tail(tail: &[String]) -> String {
    tail.iter()
        .map(|a| {
            if !a.contains('"') && a.chars().any(char::is_whitespace) {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whitespace-separated tokens of a raw Windows command line, with the byte
/// offset of the token start and end. Double quotes group but are kept.
#[cfg_attr(not(windows), allow(dead_code))]
fn raw_tokens(line: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        if c == '"' {
            quoted = !quoted;
            start.get_or_insert(i);
        } else if c.is_whitespace() && !quoted {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
        } else {
            start.get_or_insert(i);
        }
    }
    if let Some(s) = start {
        out.push((s, line.len()));
    }
    out
}

#[cfg_attr(not(windows), allow(dead_code))]
fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| *c != '"' && !c.is_whitespace())
        .collect()
}

/// The text `cmd.exe` must receive after `/c` or `/k`, as the user typed it.
/// `raw_line` is the real process command line (`GetCommandLineW`): it still
/// holds the inner quotes of `'dir /b "C:\x y"'` that PowerShell 5.1 drops
/// from `argv`. It is used only when it provably carries the same words as
/// `argv_tail` (quotes and blanks aside); otherwise the `argv` rebuild stays.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn cmd_tail_line(raw_line: Option<&str>, argv_tail: &[String]) -> String {
    let fallback = || cmd_raw_tail(argv_tail);
    let Some(line) = raw_line else {
        return fallback();
    };
    let tokens = raw_tokens(line);
    let Some(sep) = tokens.iter().position(|&(s, e)| &line[s..e] == "--") else {
        return fallback();
    };
    let Some(sw) = tokens[sep + 1..]
        .iter()
        .position(|&(s, e)| matches!(&line[s..e].to_ascii_lowercase()[..], "/c" | "/k"))
    else {
        return fallback();
    };
    let Some(&(_, end)) = tokens.get(sep + 1 + sw) else {
        return fallback();
    };
    let tail = line[end..].trim_start();
    if tail.is_empty() || squash(tail) != squash(&argv_tail.concat()) {
        return fallback();
    }
    tail.to_string()
}

#[cfg(windows)]
fn process_command_line() -> Option<String> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCommandLineW() -> *const u16;
    }
    // SAFETY: GetCommandLineW returns a NUL-terminated string owned by the
    // process for its whole lifetime; it is read once and copied.
    unsafe {
        let p = GetCommandLineW();
        if p.is_null() {
            return None;
        }
        let mut len = 0;
        while *p.add(len) != 0 {
            len += 1;
        }
        String::from_utf16(std::slice::from_raw_parts(p, len)).ok()
    }
}

pub fn build_command(program: &std::path::Path, args: &[String]) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if is_cmd_program(program) {
            if let Some(i) = cmd_switch_index(args) {
                command.args(&args[..=i]);
                if i + 1 < args.len() {
                    let raw = process_command_line();
                    command.raw_arg(cmd_tail_line(raw.as_deref(), &args[i + 1..]));
                }
                return command;
            }
        }
    }
    command.args(args);
    command
}

pub fn run(command: &[String]) -> anyhow::Result<Capture> {
    run_with_stream(command, false, false)
}

pub fn run_streaming(command: &[String], live_stderr: bool) -> anyhow::Result<Capture> {
    run_with_stream(command, true, live_stderr)
}

fn run_with_stream(command: &[String], stream: bool, live_stderr: bool) -> anyhow::Result<Capture> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("missing producer"))?;
    let resolved = crate::resolve_command_path(program).unwrap_or_else(|| program.into());
    let (mut reader, writer) = std::io::pipe()?;
    let mut producer = build_command(&resolved, args);
    producer
        .stdin(Stdio::inherit())
        .stdout(Stdio::from(writer.try_clone()?))
        .stderr(Stdio::from(writer));
    crate::capture_interrupt::configure_process_group(&mut producer);
    // Install before spawn so a signal cannot terminate lm-resizer in the
    // interval between creating the producer and registering the relay.
    let interrupt_guard = crate::capture_interrupt::relay_interruptions()
        .map_err(|error| anyhow::anyhow!("cannot install producer interruption relay: {error}"))?;
    let result = producer.spawn();
    // `Command` retains its configured Stdio handles after spawn. Close those
    // parent-side writer copies so EOF reflects the producer group exiting.
    drop(producer);
    match result {
        Ok(mut child) => {
            if let Err(error) = interrupt_guard.set_child(&child) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow::anyhow!(
                    "cannot register producer interruption relay: {error}"
                ));
            }
            let mut tee = crate::capture_interrupt::DurableTee::create();
            let mut raw = Vec::new();
            let mut chunk = [0u8; 8192];
            let mut show_live = stream;
            loop {
                let count = match reader.read(&mut chunk) {
                    Ok(count) => count,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        // A broken capture pipe can otherwise leave a producer
                        // blocked forever. End it, reap it, and keep the
                        // incrementally flushed tee available for diagnosis.
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = tee.finish(&raw);
                        return Err(error.into());
                    }
                };
                if count == 0 {
                    break;
                }
                tee.append(&chunk[..count]);
                raw.extend_from_slice(&chunk[..count]);
                if show_live {
                    // Reserve stdout for metadata in JSON mode; otherwise
                    // retain the live output on stdout for pipelines.
                    let result = if live_stderr {
                        let mut output = std::io::stderr().lock();
                        output
                            .write_all(&chunk[..count])
                            .and_then(|()| output.flush())
                    } else {
                        let mut output = std::io::stdout().lock();
                        output
                            .write_all(&chunk[..count])
                            .and_then(|()| output.flush())
                    };
                    if result.is_err() {
                        // A closed consumer of the live view must not discard
                        // the tee or abandon the producer before it is reaped.
                        show_live = false;
                    }
                }
            }
            let status = child.wait()?;
            let _ = tee.finish(&raw);
            Ok(Capture {
                raw,
                code: crate::child_exit_code(status),
                launch_error: None,
            })
        }
        Err(error) => Ok(launch_failure(program, &error)),
    }
}

pub fn launch_failure(program: &str, error: &std::io::Error) -> Capture {
    #[cfg(windows)]
    let invalid_image = matches!(error.raw_os_error(), Some(193 | 216));
    #[cfg(unix)]
    let invalid_image = error.raw_os_error() == Some(8); // ENOEXEC
    #[cfg(not(any(windows, unix)))]
    let invalid_image = false;
    Capture {
        raw: Vec::new(),
        code: if error.kind() == std::io::ErrorKind::PermissionDenied || invalid_image {
            126
        } else {
            127
        },
        launch_error: Some(launch_error_message(program, error)),
    }
}

/// Message d'échec de lancement. Pour une commande introuvable, on nomme la
/// commande et on dit où elle a été cherchée, au lieu du seul « os error 2 »
/// qui laisse croire à un fichier d'lm-resizer manquant.
fn launch_error_message(program: &str, error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        format!(
            "lm-resizer: cannot execute {program}: command not found (not in PATH, or no such file). \
             Install it, or check the spelling; nothing was run.\n"
        )
    } else {
        format!("lm-resizer: cannot execute {program}: {error}\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_command_is_named_in_the_error() {
        let result = run(&v(&["lmr-commande-inexistante-4242"])).unwrap();
        assert_eq!(result.code, 127);
        let message = result.launch_error.unwrap();
        assert!(
            message.starts_with(
                "lm-resizer: cannot execute lmr-commande-inexistante-4242: command not found"
            ),
            "{message}"
        );
        assert!(!message.contains("os error"), "{message}");
    }

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cmd_tail_keeps_embedded_quotes_verbatim() {
        let tail = v(&[r#"dir /b "C:\projet été avec espaces""#]);
        assert_eq!(cmd_raw_tail(&tail), tail[0]);
    }

    #[test]
    fn cmd_tail_quotes_bare_arguments_with_spaces_only() {
        assert_eq!(cmd_raw_tail(&v(&["dir", "a b", "/b"])), r#"dir "a b" /b"#);
    }

    fn raw(line: &str, tail: &[&str]) -> String {
        cmd_tail_line(Some(line), &v(tail))
    }

    #[test]
    fn single_argument_with_inner_quotes_powershell_51() {
        // PowerShell 5.1 hands over the quotes unescaped: argv is split at the
        // spaces of the path, the real line keeps the user's quotes.
        let line = r#"lm-resizer.exe exec -- cmd.exe /d /c "dir /b "C:\T\projet x y"""#;
        let argv = ["dir /b C:\\T\\projet", "x", "y"];
        assert_eq!(raw(line, &argv), r#""dir /b "C:\T\projet x y"""#);
        // The previous argv-only rebuild loses the quotes: it must differ.
        assert_ne!(cmd_raw_tail(&v(&argv)), raw(line, &argv));
    }

    #[test]
    fn separate_arguments_keep_their_quotes() {
        let line = r#"lm-resizer.exe exec -- cmd.exe /d /c dir /b "C:\x y""#;
        let argv = ["dir", "/b", "C:\\x y"];
        assert_eq!(raw(line, &argv), r#"dir /b "C:\x y""#);
    }

    #[test]
    fn options_before_separator_and_quoted_program() {
        let line = r#""C:\Program Files\lmr\lm-resizer.exe" exec --json -- cmd /C echo "a b""#;
        assert_eq!(raw(line, &["echo", "a b"]), r#"echo "a b""#);
    }

    #[test]
    fn unrelated_raw_line_falls_back_to_argv() {
        let line = "lm-resizer.exe hook --stdin";
        assert_eq!(raw(line, &["dir", "a b"]), r#"dir "a b""#);
        assert_eq!(cmd_tail_line(None, &v(&["echo", "x"])), "echo x");
        // Same switch but different words: the raw line is not trusted.
        let other = r#"lm-resizer.exe exec -- cmd /c something else"#;
        assert_eq!(raw(other, &["echo", "a b"]), r#"echo "a b""#);
    }

    #[test]
    fn cmd_detection() {
        assert!(is_cmd_program(std::path::Path::new("cmd.exe")));
        assert!(!is_cmd_program(std::path::Path::new("cargo")));
        assert_eq!(cmd_switch_index(&v(&["/d", "/C", "x"])), Some(1));
        assert_eq!(cmd_switch_index(&v(&["/d", "x"])), None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_process_command_line_is_available() {
        let line = process_command_line().expect("GetCommandLineW");
        assert!(!line.is_empty());
    }
}
