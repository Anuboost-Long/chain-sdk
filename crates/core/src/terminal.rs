//! Terminal capability — see /agent-docs/capabilities/terminal/CONTRACT.md.
//! Interactive programs in a pseudo-terminal, owned by Chain Core so they
//! outlive a page reload: output is numbered and kept in a bounded backlog
//! whether or not anyone is listening. `portable-pty` covers openpty on
//! macOS and ConPTY on Windows; stopping a session's tree is
//! process_tree.rs. No terminal emulation and no output parsing here.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{CommandBuilder, MasterPty, PtySize};
use serde::Serialize;

use crate::process_tree;

pub const DEFAULT_BACKLOG_BYTES: usize = 512 * 1024;
/// Output arriving within this long of a chunk joins it, up to BATCH_BYTES:
/// TUIs and `npm install` write in bursts of tiny reads.
const BATCH_WINDOW: Duration = Duration::from_millis(4);
const BATCH_BYTES: usize = 64 * 1024;
/// After the program exits, how long to keep reading output still in the
/// pty before reporting the exit (a background job may hold it open).
const DRAIN_AFTER_EXIT: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub enum TerminalError {
    InvalidArgument(String),
    /// No such program, or no such session.
    NotFound(String),
    PermissionDenied(String),
    /// The session has exited.
    Unavailable(String),
    TimedOut(String),
    Other(String),
}

impl TerminalError {
    pub fn message(&self) -> &str {
        match self {
            TerminalError::InvalidArgument(m)
            | TerminalError::NotFound(m)
            | TerminalError::PermissionDenied(m)
            | TerminalError::Unavailable(m)
            | TerminalError::TimedOut(m)
            | TerminalError::Other(m) => m,
        }
    }
}

type Result<T> = std::result::Result<T, TerminalError>;

pub struct StartOptions {
    pub command: String,
    pub args: Vec<String>,
    /// Already checked by the caller (an existing folder inside a grant).
    pub cwd: Option<PathBuf>,
    /// Set over the inherited environment, the login-shell `PATH` and
    /// `TERM=xterm-256color`, so any of those can be overridden.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    pub label: String,
    pub metadata: BTreeMap<String, String>,
    pub backlog_bytes: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionExit {
    /// `None` when a signal ended it.
    pub code: Option<i32>,
    pub killed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub label: String,
    pub metadata: BTreeMap<String, String>,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub pid: Option<u32>,
    pub started_at_ms: f64,
    pub cols: u16,
    pub rows: u16,
    pub exit: Option<SessionExit>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputChunk {
    pub session_id: String,
    pub seq: u64,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Backlog {
    pub data: String,
    /// The `seq` of the newest chunk in `data`; `0` before any output.
    pub seq: u64,
}

pub type OnOutput = Arc<dyn Fn(&OutputChunk) + Send + Sync>;
pub type OnExit = Arc<dyn Fn(&str, &SessionExit) + Send + Sync>;

struct Retained {
    chunks: VecDeque<(u64, String)>,
    bytes: usize,
    limit: usize,
    seq: u64,
}

struct Session {
    info: Mutex<SessionInfo>,
    retained: Mutex<Retained>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killed: AtomicBool,
}

impl Session {
    fn running_pid(&self) -> Result<Option<u32>> {
        let info = self.info.lock().expect("terminal mutex poisoned");
        if info.exit.is_some() {
            return Err(TerminalError::Unavailable(format!("session {} has exited", info.id)));
        }
        Ok(info.pid)
    }
}

/// Every session of one app, running or exited but not yet removed.
pub struct Terminals {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next: AtomicU64,
    on_output: OnOutput,
    on_exit: OnExit,
}

impl Terminals {
    /// `on_output` sees every chunk of every session, in order, after it's
    /// in the backlog; `on_exit` sees each exit once, after its last chunk.
    pub fn new(on_output: OnOutput, on_exit: OnExit) -> Self {
        Self { sessions: Mutex::new(HashMap::new()), next: AtomicU64::new(1), on_output, on_exit }
    }

    pub fn start(&self, options: StartOptions) -> Result<SessionInfo> {
        if options.command.is_empty() {
            return Err(TerminalError::InvalidArgument("command must not be empty".to_string()));
        }
        if options.cols == 0 || options.rows == 0 {
            return Err(TerminalError::InvalidArgument("cols and rows must be at least 1".to_string()));
        }
        let size = PtySize { rows: options.rows, cols: options.cols, pixel_width: 0, pixel_height: 0 };
        let pair = portable_pty::native_pty_system().openpty(size).map_err(|e| TerminalError::Other(e.to_string()))?;

        let mut command = CommandBuilder::new(&options.command);
        command.args(&options.args);
        #[cfg(unix)]
        if let Some(path) = crate::process_runner::login_shell_path() {
            command.env("PATH", path);
        }
        command.env("TERM", "xterm-256color");
        for (key, value) in &options.env {
            command.env(key, value);
        }
        if let Some(cwd) = &options.cwd {
            command.cwd(cwd);
        }

        let mut child = pair.slave.spawn_command(command).map_err(classify_spawn_error)?;
        // Only the child holds the slave now, so the reader sees EOF when it's gone.
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(|e| TerminalError::Other(e.to_string()))?;
        let writer = pair.master.take_writer().map_err(|e| TerminalError::Other(e.to_string()))?;

        let id = format!("term-{}-{}", self.next.fetch_add(1, Ordering::SeqCst), &crate::files::generate_id()[..6]);
        let info = SessionInfo {
            id: id.clone(),
            label: options.label,
            metadata: options.metadata,
            command: options.command,
            args: options.args,
            cwd: options.cwd.map(|c| c.to_string_lossy().into_owned()),
            pid: child.process_id(),
            started_at_ms: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0),
            cols: options.cols,
            rows: options.rows,
            exit: None,
        };
        let session = Arc::new(Session {
            info: Mutex::new(info.clone()),
            retained: Mutex::new(Retained { chunks: VecDeque::new(), bytes: 0, limit: options.backlog_bytes, seq: 0 }),
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killed: AtomicBool::new(false),
        });
        self.sessions.lock().expect("terminal mutex poisoned").insert(id, Arc::clone(&session));

        let (events, received) = mpsc::channel();
        let read_events = events.clone();
        std::thread::spawn(move || read_output(reader, read_events));
        std::thread::spawn(move || {
            let status = child.wait();
            let code = status.ok().and_then(|s| if s.signal().is_some() { None } else { Some(s.exit_code() as i32) });
            let _ = events.send(Event::Exited(code));
        });
        let (on_output, on_exit) = (Arc::clone(&self.on_output), Arc::clone(&self.on_exit));
        std::thread::spawn(move || deliver(&session, received, &on_output, &on_exit));
        Ok(info)
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        let sessions = self.sessions.lock().expect("terminal mutex poisoned");
        let mut list: Vec<SessionInfo> =
            sessions.values().map(|s| s.info.lock().expect("terminal mutex poisoned").clone()).collect();
        list.sort_by(|a, b| a.started_at_ms.total_cmp(&b.started_at_ms));
        list
    }

    fn session(&self, id: &str) -> Result<Arc<Session>> {
        self.sessions
            .lock()
            .expect("terminal mutex poisoned")
            .get(id)
            .cloned()
            .ok_or_else(|| TerminalError::NotFound(format!("no session {id}")))
    }

    pub fn backlog(&self, id: &str) -> Result<Backlog> {
        let session = self.session(id)?;
        let retained = session.retained.lock().expect("terminal mutex poisoned");
        Ok(Backlog { data: retained.chunks.iter().map(|(_, data)| data.as_str()).collect(), seq: retained.seq })
    }

    pub fn write(&self, id: &str, data: &str) -> Result<()> {
        let session = self.session(id)?;
        session.running_pid()?;
        let mut writer = session.writer.lock().expect("terminal mutex poisoned");
        writer.write_all(data.as_bytes()).and_then(|()| writer.flush()).map_err(|e| TerminalError::Other(e.to_string()))
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        if cols == 0 || rows == 0 {
            return Err(TerminalError::InvalidArgument("cols and rows must be at least 1".to_string()));
        }
        let session = self.session(id)?;
        session.running_pid()?;
        let size = PtySize { rows, cols, pixel_width: 0, pixel_height: 0 };
        // TIOCSWINSZ; the kernel sends SIGWINCH to the foreground job.
        session.master.lock().expect("terminal mutex poisoned").resize(size).map_err(|e| TerminalError::Other(e.to_string()))?;
        let mut info = session.info.lock().expect("terminal mutex poisoned");
        (info.cols, info.rows) = (cols, rows);
        Ok(())
    }

    /// Stops the session's whole tree, like process_runner's kill().
    /// Idempotent: an exited session resolves.
    pub fn kill(&self, id: &str) -> Result<()> {
        let session = self.session(id)?;
        let Ok(Some(pid)) = session.running_pid() else { return Ok(()) };
        session.killed.store(true, Ordering::SeqCst);
        process_tree::terminate(pid, process_tree::GRACE, process_tree::LIMIT).map_err(TerminalError::TimedOut)
    }

    /// Forgets a session and its backlog, killing it first (without
    /// waiting) if it's still running. Idempotent.
    pub fn remove(&self, id: &str) {
        let Some(session) = self.sessions.lock().expect("terminal mutex poisoned").remove(id) else { return };
        if let Ok(Some(pid)) = session.running_pid() {
            session.killed.store(true, Ordering::SeqCst);
            process_tree::kill_now(pid);
        }
    }

    /// For the app quitting.
    pub fn kill_all_now(&self) {
        for session in self.sessions.lock().expect("terminal mutex poisoned").values() {
            if let Ok(Some(pid)) = session.running_pid() {
                process_tree::kill_now(pid);
            }
        }
    }
}

enum Event {
    Output(String),
    Eof,
    Exited(Option<i32>),
}

/// Decodes UTF-8 across reads: a character split between two reads is
/// held back until its last byte arrives, never turned into U+FFFD.
fn read_output(mut reader: Box<dyn Read + Send>, events: mpsc::Sender<Event>) {
    let mut buf = [0u8; 16 * 1024];
    let mut pending = Vec::new();
    loop {
        match reader.read(&mut buf) {
            // EIO is how macOS reports the last slave fd closing.
            Ok(0) | Err(_) => break,
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                let text = take_complete_utf8(&mut pending);
                if !text.is_empty() && events.send(Event::Output(text)).is_err() {
                    return;
                }
            }
        }
    }
    if !pending.is_empty() {
        let _ = events.send(Event::Output(String::from_utf8_lossy(&pending).into_owned()));
    }
    let _ = events.send(Event::Eof);
}

/// Everything in `pending` that decodes, invalid sequences as U+FFFD,
/// leaving only an incomplete final character behind.
fn take_complete_utf8(pending: &mut Vec<u8>) -> String {
    let mut text = String::new();
    loop {
        match std::str::from_utf8(pending) {
            Ok(valid) => {
                text.push_str(valid);
                pending.clear();
                return text;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                text.push_str(std::str::from_utf8(&pending[..valid]).expect("checked by valid_up_to"));
                match e.error_len() {
                    None => {
                        pending.drain(..valid);
                        return text;
                    }
                    Some(invalid) => {
                        text.push('\u{FFFD}');
                        pending.drain(..valid + invalid);
                    }
                }
            }
        }
    }
}

/// Numbers, retains and reports output in order, then the exit after the
/// last of it.
fn deliver(session: &Session, events: mpsc::Receiver<Event>, on_output: &OnOutput, on_exit: &OnExit) {
    let session_id = session.info.lock().expect("terminal mutex poisoned").id.clone();
    let mut exited: Option<(Option<i32>, Instant)> = None;
    let mut eof = false;
    let mut batch = String::new();

    while !(eof && exited.is_some()) {
        let next = match exited {
            Some((_, deadline)) => events.recv_timeout(deadline.saturating_duration_since(Instant::now())),
            None => events.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        let mut event = match next {
            Ok(event) => Some(event),
            Err(_) => break,
        };
        while let Some(Event::Output(text)) = &event {
            batch.push_str(text);
            event = if batch.len() >= BATCH_BYTES { None } else { events.recv_timeout(BATCH_WINDOW).ok() };
        }
        if !batch.is_empty() {
            retain_and_report(session, &session_id, std::mem::take(&mut batch), on_output);
        }
        match event {
            Some(Event::Eof) => eof = true,
            Some(Event::Exited(code)) => exited = Some((code, Instant::now() + DRAIN_AFTER_EXIT)),
            _ => {}
        }
    }

    let exit = SessionExit {
        code: exited.and_then(|(code, _)| code),
        killed: session.killed.load(Ordering::SeqCst),
    };
    session.info.lock().expect("terminal mutex poisoned").exit = Some(exit.clone());
    on_exit(&session_id, &exit);
}

fn retain_and_report(session: &Session, session_id: &str, data: String, on_output: &OnOutput) {
    // Reported under the lock, so a backlog() read and the live chunks
    // after it never overlap or leave a gap.
    let mut retained = session.retained.lock().expect("terminal mutex poisoned");
    retained.seq += 1;
    let chunk = OutputChunk { session_id: session_id.to_string(), seq: retained.seq, data };
    retained.bytes += chunk.data.len();
    retained.chunks.push_back((chunk.seq, chunk.data.clone()));
    // Whole chunks only, so an escape sequence is never cut in half; the
    // newest chunk stays even if it alone is over the limit.
    while retained.bytes > retained.limit && retained.chunks.len() > 1 {
        let (_, dropped) = retained.chunks.pop_front().expect("more than one chunk");
        retained.bytes -= dropped.len();
    }
    on_output(&chunk);
}

fn classify_spawn_error(e: anyhow::Error) -> TerminalError {
    if let Some(io) = e.downcast_ref::<std::io::Error>() {
        return match io.kind() {
            std::io::ErrorKind::NotFound => TerminalError::NotFound(e.to_string()),
            std::io::ErrorKind::PermissionDenied => TerminalError::PermissionDenied(e.to_string()),
            _ => TerminalError::Other(e.to_string()),
        };
    }
    let message = e.to_string();
    if message.contains("not executable") {
        TerminalError::PermissionDenied(message)
    } else if message.contains("does not exist") || message.contains("No viable candidates") {
        TerminalError::NotFound(message)
    } else {
        TerminalError::Other(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Recorded {
        chunks: Mutex<Vec<OutputChunk>>,
        exits: Mutex<Vec<(String, SessionExit)>>,
    }

    fn terminals() -> (Terminals, Arc<Recorded>) {
        let recorded = Arc::new(Recorded { chunks: Mutex::new(Vec::new()), exits: Mutex::new(Vec::new()) });
        let (for_output, for_exit) = (Arc::clone(&recorded), Arc::clone(&recorded));
        let terminals = Terminals::new(
            Arc::new(move |chunk| for_output.chunks.lock().unwrap().push(chunk.clone())),
            Arc::new(move |id, exit| for_exit.exits.lock().unwrap().push((id.to_string(), exit.clone()))),
        );
        (terminals, recorded)
    }

    fn options(command: &str, args: &[&str]) -> StartOptions {
        StartOptions {
            command: command.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: None,
            env: Vec::new(),
            cols: 100,
            rows: 30,
            label: "test".to_string(),
            metadata: BTreeMap::new(),
            backlog_bytes: DEFAULT_BACKLOG_BYTES,
        }
    }

    fn wait_for_exit(recorded: &Recorded, id: &str) -> SessionExit {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some((_, exit)) = recorded.exits.lock().unwrap().iter().find(|(i, _)| i == id) {
                return exit.clone();
            }
            assert!(Instant::now() < deadline, "session {id} didn't exit");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn output_of(recorded: &Recorded, id: &str) -> String {
        recorded.chunks.lock().unwrap().iter().filter(|c| c.session_id == id).map(|c| c.data.as_str()).collect()
    }

    #[test]
    fn runs_in_a_real_terminal_with_the_requested_size() {
        let (terminals, recorded) = terminals();
        let mut opts = options("/bin/sh", &["-c", "test -t 0 && test -t 1 && echo tty; stty size; echo $TERM"]);
        opts.env = vec![("EXTRA".to_string(), "1".to_string())];
        let session = terminals.start(opts).unwrap();
        assert_eq!(wait_for_exit(&recorded, &session.id), SessionExit { code: Some(0), killed: false });
        assert_eq!(output_of(&recorded, &session.id), "tty\r\n30 100\r\nxterm-256color\r\n");
    }

    #[test]
    fn sequences_increase_and_the_backlog_matches_what_was_reported() {
        let (terminals, recorded) = terminals();
        let session = terminals.start(options("/bin/sh", &["-c", "for i in 1 2 3; do echo line$i; sleep 0.05; done"])).unwrap();
        wait_for_exit(&recorded, &session.id);
        let seqs: Vec<u64> = recorded.chunks.lock().unwrap().iter().map(|c| c.seq).collect();
        assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1) && seqs[0] == 1, "{seqs:?}");
        let backlog = terminals.backlog(&session.id).unwrap();
        assert_eq!(backlog.seq, *seqs.last().unwrap());
        assert_eq!(backlog.data, output_of(&recorded, &session.id));
    }

    #[test]
    fn keystrokes_reach_the_program_and_resize_reaches_its_terminal() {
        let (terminals, recorded) = terminals();
        let session = terminals.start(options("/bin/sh", &["-c", "read line; echo \"got:$line\"; read _; stty size"])).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        terminals.write(&session.id, "hello\r").unwrap();
        std::thread::sleep(Duration::from_millis(100));
        terminals.resize(&session.id, 132, 43).unwrap();
        terminals.write(&session.id, "\r").unwrap();
        wait_for_exit(&recorded, &session.id);
        let output = output_of(&recorded, &session.id);
        assert!(output.contains("got:hello"), "{output:?}");
        assert!(output.ends_with("43 132\r\n"), "{output:?}");
        assert!(matches!(terminals.write(&session.id, "x"), Err(TerminalError::Unavailable(_))));
        assert!(matches!(terminals.write("term-0-none", "x"), Err(TerminalError::NotFound(_))));
    }

    #[test]
    fn a_character_split_across_reads_is_never_corrupted() {
        let mut pending = vec![b'a', 0xE2, 0x82];
        assert_eq!(take_complete_utf8(&mut pending), "a");
        pending.extend_from_slice(&[0xAC, b'!']);
        assert_eq!(take_complete_utf8(&mut pending), "€!");
        let mut invalid = vec![b'x', 0xFF, b'y'];
        assert_eq!(take_complete_utf8(&mut invalid), "x\u{FFFD}y");
        assert!(invalid.is_empty());
    }

    #[test]
    fn the_backlog_keeps_whole_recent_chunks_within_its_limit() {
        let (terminals, recorded) = terminals();
        let mut opts = options("/bin/sh", &["-c", "for i in $(seq 1 40); do printf 'chunk%02d-%0200d\\n' $i 0; sleep 0.01; done"]);
        opts.backlog_bytes = 2000;
        let session = terminals.start(opts).unwrap();
        wait_for_exit(&recorded, &session.id);
        let backlog = terminals.backlog(&session.id).unwrap();
        assert!(backlog.data.len() <= 2000 + BATCH_BYTES, "{}", backlog.data.len());
        assert!(backlog.data.ends_with(&format!("chunk40-{}\r\n", "0".repeat(200))));
        assert!(backlog.data.starts_with("chunk"), "trimmed mid-chunk: {:?}", &backlog.data[..20]);
    }

    #[test]
    fn kill_stops_the_tree_and_reports_killed() {
        let (terminals, recorded) = terminals();
        let session = terminals.start(options("/bin/sh", &["-c", "sleep 30 & echo $!; wait"])).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let grandchild = loop {
            if let Ok(pid) = output_of(&recorded, &session.id).trim().parse::<i32>() {
                break pid;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        };
        terminals.kill(&session.id).unwrap();
        assert!(wait_for_exit(&recorded, &session.id).killed);
        assert_ne!(unsafe { libc::kill(grandchild, 0) }, 0, "the grandchild survived");
        terminals.kill(&session.id).unwrap();

        assert_eq!(terminals.list().len(), 1);
        terminals.remove(&session.id);
        terminals.remove(&session.id);
        assert!(terminals.list().is_empty());
    }

    #[test]
    fn a_missing_program_is_not_found() {
        let (terminals, _) = terminals();
        assert!(matches!(
            terminals.start(options("chain-sdk-definitely-not-a-real-command", &[])),
            Err(TerminalError::NotFound(_))
        ));
        assert!(matches!(terminals.start(options("", &[])), Err(TerminalError::InvalidArgument(_))));
    }
}
