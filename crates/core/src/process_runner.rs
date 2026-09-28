//! ProcessRunner capability — see
//! /agent-docs/capabilities/process-runner/CONTRACT.md.
//! Spawns an executable by name + argv array (never a shell string) and
//! streams its stdout/stderr back incrementally via `on_output`, with a
//! `ProcessHandle` for `kill()` and a final `on_exit` callback once it's
//! done. There is no per-OS branching in this file — the Windows npm-shim
//! risk (see agent-docs/capabilities/process-runner/research/WINDOWS.md)
//! is a spawn-resolution detail that still needs a real decision, tracked
//! there, not baked in here yet.
//!
//! No AI-CLI awareness whatsoever: this module has no idea what a chunk
//! of stdout means, doesn't parse it, doesn't assume it's line-oriented.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub struct ProcessOutputChunk {
    pub stream: ProcessStream,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessExit {
    pub code: Option<i32>,
    pub killed: bool,
}

#[derive(Debug)]
pub enum ProcessRunnerError {
    /// `command` was empty.
    InvalidArgument(String),
    /// `command` couldn't be resolved to an executable at all (the OS's
    /// own ENOENT-equivalent).
    NotFound(String),
    /// The resolved file exists but the OS refused to execute it.
    PermissionDenied(String),
    Other(String),
}

impl ProcessRunnerError {
    pub fn message(&self) -> &str {
        match self {
            ProcessRunnerError::InvalidArgument(m)
            | ProcessRunnerError::NotFound(m)
            | ProcessRunnerError::PermissionDenied(m)
            | ProcessRunnerError::Other(m) => m,
        }
    }
}

/// A running (or just-exited) process. `kill()` is the documented,
/// idempotent way to stop it early.
pub struct ProcessHandle {
    child: Arc<Mutex<Child>>,
    killed: Arc<AtomicBool>,
}

impl ProcessHandle {
    /// Idempotent — killing a process that has already exited resolves
    /// successfully rather than erroring, same reasoning `files.delete()`
    /// and `agent_server`'s `stop()` already use.
    pub fn kill(&self) -> Result<(), ProcessRunnerError> {
        self.killed.store(true, Ordering::SeqCst);
        let mut guard = self.child.lock().expect("process mutex poisoned");
        match guard.kill() {
            Ok(()) => Ok(()),
            // "InvalidInput" is what std reports when the process has
            // already exited — not a real failure to report upward.
            Err(e) if e.kind() == std::io::ErrorKind::InvalidInput => Ok(()),
            Err(e) => Err(ProcessRunnerError::Other(e.to_string())),
        }
    }
}

/// Spawns `command` with `args` — a literal argv array passed straight to
/// the OS, never a shell string. `on_output` fires once per chunk of
/// bytes read from either stdout or stderr, as soon as it's available;
/// chunk boundaries carry no meaning (see CONTRACT.md). `on_exit` fires
/// exactly once, after both streams have hit EOF and the process has
/// actually exited (including via `kill()`).
///
/// `stdin`, when given, is written to the child's stdin on its own thread
/// and then stdin is closed (EOF); `None` keeps stdin the null device.
pub fn run(
    command: &str,
    args: &[String],
    stdin: Option<String>,
    on_output: impl Fn(ProcessOutputChunk) + Send + Sync + 'static,
    on_exit: impl FnOnce(ProcessExit) + Send + 'static,
) -> Result<ProcessHandle, ProcessRunnerError> {
    if command.is_empty() {
        return Err(ProcessRunnerError::InvalidArgument("command must not be empty".to_string()));
    }

    let mut cmd = Command::new(command);
    cmd.args(args);
    // A GUI app launched by launchd/Finder (not a terminal) inherits a
    // minimal system PATH that never sources .zshrc/.zprofile, so a bare
    // command name that's only on the user's login-shell PATH (nvm,
    // homebrew, asdf, ...) resolves in `chain dev` but not in a packaged
    // build. Substitute the real login-shell PATH so both resolve the
    // same executables. Still just `PATH` resolution, not a new
    // caller-facing env option (see CONTRACT.md's Non-goals).
    #[cfg(unix)]
    if let Some(path) = login_shell_path() {
        cmd.env("PATH", path);
    }
    // Null, not an empty pipe, when there's no payload — some CLIs behave
    // differently when stdin is a pipe (see CONTRACT.md's `options.stdin`).
    cmd.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(classify_spawn_error)?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    // Its own thread, concurrent with the stdout/stderr readers below: a
    // payload larger than the pipe buffer would otherwise deadlock against
    // a child that prints before it has read all its input. A write error
    // (the child exited or closed stdin early — EPIPE) is deliberately
    // ignored: the process still ran, and `on_exit` reports how it ended.
    // Dropping `child_stdin` at the end of the thread closes it (EOF).
    if let (Some(payload), Some(mut child_stdin)) = (stdin, child.stdin.take()) {
        std::thread::spawn(move || {
            let _ = child_stdin.write_all(payload.as_bytes());
        });
    }

    let child = Arc::new(Mutex::new(child));
    let killed = Arc::new(AtomicBool::new(false));
    let on_output = Arc::new(on_output);

    let stdout_output = Arc::clone(&on_output);
    let stdout_thread =
        std::thread::spawn(move || read_stream(stdout, ProcessStream::Stdout, &*stdout_output));

    let stderr_output = Arc::clone(&on_output);
    let stderr_thread =
        std::thread::spawn(move || read_stream(stderr, ProcessStream::Stderr, &*stderr_output));

    let wait_child = Arc::clone(&child);
    let wait_killed = Arc::clone(&killed);
    std::thread::spawn(move || {
        // Drain both pipes fully before reaping the exit status, so no
        // output is ever lost to a race with the process exiting.
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();
        let status = wait_child.lock().expect("process mutex poisoned").wait();
        let code = status.ok().and_then(|s| s.code());
        on_exit(ProcessExit { code, killed: wait_killed.load(Ordering::SeqCst) });
    });

    Ok(ProcessHandle { child, killed })
}

fn read_stream<R: Read, F: Fn(ProcessOutputChunk) + Send + Sync>(
    mut reader: R,
    stream: ProcessStream,
    on_output: &F,
) {
    let mut buf = [0u8; 8192];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let data = String::from_utf8_lossy(&buf[..n]).into_owned();
                on_output(ProcessOutputChunk { stream, data });
            }
            Err(_) => break,
        }
    }
}

/// The user's real login-shell `PATH` (nvm/homebrew/asdf dirs included),
/// resolved once by spawning `$SHELL -lic 'echo ...${PATH}...'` and cached
/// for the process's lifetime — a GUI app launched by launchd/Finder never
/// sources `.zshrc`/`.zprofile` on its own, so the bare `PATH` it inherits
/// is missing anything a login shell would add. `None` if the shell
/// couldn't be run, didn't exit cleanly, timed out, or produced an empty
/// `PATH` — callers fall back to the inherited `PATH` in that case.
#[cfg(unix)]
fn login_shell_path() -> Option<String> {
    static RESOLVED: OnceLock<Option<String>> = OnceLock::new();
    RESOLVED.get_or_init(resolve_login_shell_path).clone()
}

#[cfg(unix)]
fn resolve_login_shell_path() -> Option<String> {
    use std::sync::mpsc;
    use std::time::Duration;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        // Markers delimit the real PATH from anything a login shell's rc
        // files print to stdout on startup (banners, prompts, etc.), so
        // that noise can't leak into the value we hand to `Command::env`.
        const START: &str = "__chain_path_start__";
        const END: &str = "__chain_path_end__";
        // `${PATH}` (braced), not `$PATH` — unbraced, the shell reads
        // `$PATH__chain_path_end__` as one (undefined) variable name and
        // silently expands it to nothing, swallowing the END marker.
        let output = Command::new(&shell).args(["-lic", &format!("echo -n {START}${{PATH}}{END}")]).output();
        let resolved = output.ok().and_then(|out| {
            let text = String::from_utf8_lossy(&out.stdout).into_owned();
            let start = text.find(START)? + START.len();
            let end = start + text[start..].find(END)?;
            let path = text[start..end].to_string();
            if path.is_empty() { None } else { Some(path) }
        });
        let _ = tx.send(resolved);
    });
    // A login shell that hangs (e.g. an rc file blocking on network)
    // shouldn't stall every spawn indefinitely — fall back to the
    // inherited PATH instead.
    rx.recv_timeout(Duration::from_secs(3)).ok().flatten()
}

fn classify_spawn_error(e: std::io::Error) -> ProcessRunnerError {
    match e.kind() {
        std::io::ErrorKind::NotFound => ProcessRunnerError::NotFound(e.to_string()),
        std::io::ErrorKind::PermissionDenied => ProcessRunnerError::PermissionDenied(e.to_string()),
        _ => ProcessRunnerError::Other(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    fn wait_for_exit(rx: mpsc::Receiver<ProcessExit>) -> ProcessExit {
        rx.recv_timeout(Duration::from_secs(5)).expect("process did not exit in time")
    }

    #[test]
    fn captures_stdout_and_a_clean_exit_code() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let chunks_for_output = Arc::clone(&chunks);

        let handle = run(
            "/bin/echo",
            &["hello world".to_string()],
            None,
            move |chunk| {
                assert_eq!(chunk.stream, ProcessStream::Stdout);
                chunks_for_output.lock().unwrap().push(chunk.data);
            },
            move |exit| {
                let _ = exit_tx.send(exit);
            },
        )
        .unwrap();

        let exit = wait_for_exit(exit_rx);
        assert_eq!(exit, ProcessExit { code: Some(0), killed: false });
        assert_eq!(chunks.lock().unwrap().concat(), "hello world\n");

        // Idempotent: killing an already-exited process still succeeds.
        handle.kill().unwrap();
    }

    #[test]
    fn captures_stderr_separately_from_stdout() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let stdout_chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let stderr_chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let stdout_for_output = Arc::clone(&stdout_chunks);
        let stderr_for_output = Arc::clone(&stderr_chunks);

        run(
            "/bin/sh",
            &["-c".to_string(), "echo out-message; echo err-message 1>&2".to_string()],
            None,
            move |chunk| match chunk.stream {
                ProcessStream::Stdout => stdout_for_output.lock().unwrap().push(chunk.data),
                ProcessStream::Stderr => stderr_for_output.lock().unwrap().push(chunk.data),
            },
            move |exit| {
                let _ = exit_tx.send(exit);
            },
        )
        .unwrap();

        let exit = wait_for_exit(exit_rx);
        assert_eq!(exit.code, Some(0));
        assert_eq!(stdout_chunks.lock().unwrap().concat(), "out-message\n");
        assert_eq!(stderr_chunks.lock().unwrap().concat(), "err-message\n");
    }

    #[test]
    fn reports_a_non_zero_exit_code_without_erroring() {
        let (exit_tx, exit_rx) = mpsc::channel();
        run("/bin/sh", &["-c".to_string(), "exit 7".to_string()], None, |_| {}, move |exit| {
            let _ = exit_tx.send(exit);
        })
        .unwrap();

        assert_eq!(wait_for_exit(exit_rx), ProcessExit { code: Some(7), killed: false });
    }

    #[test]
    fn output_arrives_incrementally_not_buffered_to_exit() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let timestamps = Arc::new(Mutex::new(Vec::<Instant>::new()));
        let timestamps_for_output = Arc::clone(&timestamps);

        run(
            "/bin/sh",
            &[
                "-c".to_string(),
                "printf a; sleep 0.05; printf b; sleep 0.05; printf c".to_string(),
            ],
            None,
            move |_chunk| {
                timestamps_for_output.lock().unwrap().push(Instant::now());
            },
            move |exit| {
                let _ = exit_tx.send(exit);
            },
        )
        .unwrap();

        wait_for_exit(exit_rx);
        let stamps = timestamps.lock().unwrap();
        assert!(stamps.len() >= 2, "expected multiple separately-delivered chunks, got {}", stamps.len());
        let spread = *stamps.last().unwrap() - *stamps.first().unwrap();
        assert!(
            spread >= Duration::from_millis(60),
            "chunks arrived within {spread:?} of each other — looks buffered to exit, not streamed"
        );
    }

    #[test]
    fn kill_stops_a_long_running_process_early() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let handle = run("/bin/sh", &["-c".to_string(), "sleep 5".to_string()], None, |_| {}, move |exit| {
            let _ = exit_tx.send(exit);
        })
        .unwrap();

        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        handle.kill().unwrap();

        let exit = wait_for_exit(exit_rx);
        assert!(exit.killed);
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "kill() should stop the process well before its own 5s sleep would"
        );
    }

    #[test]
    fn writes_a_stdin_payload_then_closes_it() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let chunks_for_output = Arc::clone(&chunks);

        // `cat` only exits once it sees EOF, so a clean exit proves stdin was closed.
        run("/bin/cat", &[], Some("héllo\0world".to_string()), move |chunk| {
            chunks_for_output.lock().unwrap().push(chunk.data);
        }, move |exit| {
            let _ = exit_tx.send(exit);
        })
        .unwrap();

        assert_eq!(wait_for_exit(exit_rx), ProcessExit { code: Some(0), killed: false });
        assert_eq!(chunks.lock().unwrap().concat(), "héllo\0world");
    }

    #[test]
    fn a_payload_larger_than_the_pipe_buffer_does_not_deadlock() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let received = Arc::new(Mutex::new(0usize));
        let received_for_output = Arc::clone(&received);
        let payload = "0123456789abcdef".repeat(4 * 1024 * 1024 / 16);

        // `cat` echoes as it reads, so its stdout fills while stdin is still
        // being written — deadlocks if writing blocks reading.
        run("/bin/cat", &[], Some(payload.clone()), move |chunk| {
            *received_for_output.lock().unwrap() += chunk.data.len();
        }, move |exit| {
            let _ = exit_tx.send(exit);
        })
        .unwrap();

        assert_eq!(wait_for_exit(exit_rx).code, Some(0));
        assert_eq!(*received.lock().unwrap(), payload.len());
    }

    #[test]
    fn a_child_that_ignores_stdin_is_not_an_error() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let payload = "x".repeat(1024 * 1024);

        // Exits without reading, so the writer hits a broken pipe.
        run("/bin/sh", &["-c".to_string(), "exit 3".to_string()], Some(payload), |_| {}, move |exit| {
            let _ = exit_tx.send(exit);
        })
        .unwrap();

        assert_eq!(wait_for_exit(exit_rx), ProcessExit { code: Some(3), killed: false });
    }

    #[test]
    fn no_stdin_payload_gives_the_child_an_immediate_eof() {
        let (exit_tx, exit_rx) = mpsc::channel();
        let chunks = Arc::new(Mutex::new(Vec::<String>::new()));
        let chunks_for_output = Arc::clone(&chunks);

        run("/bin/cat", &[], None, move |chunk| {
            chunks_for_output.lock().unwrap().push(chunk.data);
        }, move |exit| {
            let _ = exit_tx.send(exit);
        })
        .unwrap();

        assert_eq!(wait_for_exit(exit_rx).code, Some(0));
        assert!(chunks.lock().unwrap().is_empty());
    }

    #[test]
    fn rejects_an_empty_command() {
        match run("", &[], None, |_| {}, |_| {}) {
            Err(ProcessRunnerError::InvalidArgument(_)) => {}
            other => panic!("expected InvalidArgument, got {}", other.is_ok()),
        }
    }

    #[test]
    fn rejects_a_nonexistent_command() {
        match run("chain-sdk-definitely-not-a-real-command", &[], None, |_| {}, |_| {}) {
            Err(ProcessRunnerError::NotFound(_)) => {}
            other => panic!("expected NotFound, got {}", other.is_ok()),
        }
    }
}
