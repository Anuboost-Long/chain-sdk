# Terminal — macOS Research

Verified on macOS (Darwin 27) while implementing
`crates/core/src/terminal.rs`, 9 October 2026.

## `portable-pty` (WezTerm), no native adapter

`portable_pty::native_pty_system().openpty(size)` is `openpty(3)`.
`SlavePty::spawn_command` forks and, in the child, calls `setsid()` and
`ioctl(0, TIOCSCTTY)`. So the program leads a new session *and* process
group (`pgid == pid`), with the pty as its controlling terminal. That is
exactly what `process_tree::terminate` needs to signal the whole tree.
No Swift.

Details that mattered:

- `CommandBuilder::new` starts from the parent's full environment
  (`get_base_env`), and its `PATH` lookup uses the builder's own `PATH`.
  Setting the login-shell `PATH` (shared with process-runner) makes bare
  names resolve under Finder launches too.
- Spawn errors are `anyhow::Error`s. A missing program is the message
  "No viable candidates found in PATH" or "does not exist", and a
  non-executable file is "not executable". `classify_spawn_error` maps
  those to `NOT_FOUND` / `PERMISSION_DENIED`.
- `ExitStatus::exit_code()` reports `1` for a signal death; the real
  information is `signal()`. A signal exit is reported as `code: null`.
- Drop the `SlavePty` in the parent right after spawning. Otherwise the
  reader never sees EOF.
- Reading the master after the last slave fd closes returns `EIO` on
  macOS, not `Ok(0)`. Both end the reader.

## Resize

`MasterPty::resize` is `ioctl(TIOCSWINSZ)`, and the kernel sends
`SIGWINCH` to the foreground process group itself. Verified: `stty size`
after `resize(132, 43)` printed `43 132`.

## Line discipline

Default termios: `ICRNL` (a written `\r` reaches `read` as `\n`), `ONLCR`
(output `\n` arrives as `\r\n`), and echo on. So keystrokes echo back in
the output, as in any terminal.

## Ordering, batching and the backlog

One reader thread decodes UTF-8 statefully (an incomplete trailing
sequence waits for the next read) and sends text into a channel. One
delivery thread batches what arrives within 4 ms (max 64 KB), assigns
`seq`, appends to the backlog and calls `on_output`, all under the
backlog lock. A `backlog()` call therefore returns exactly the chunks
`1..=seq`, and every later report has a higher seq. The SDK's `attach()`
listens first, holds live chunks until the backlog reply arrives, then
forwards only `seq > backlog.seq`.

The exit is reported by the same thread after the reader's EOF, so it
always follows the last chunk. If a background job keeps the slave open,
it waits at most 1 s after the program's exit, then reports anyway.

## End-to-end verification (throwaway `chain init` app, `chain dev`)

- A ticker (`echo tick$i; sleep 0.1` forever) with `cwd` in a declared
  folder, a label and metadata. Then `location.reload()`. Two seconds
  later the new page found it still `running` in `list()`, and
  `attach()` delivered `tick1`…`tick130` contiguous and in order: the
  backlog chunk carried seq 119, then live 120, 121, … with no
  duplicates.
- `write("héllo\r")` came back as `got:héllo` (non-ASCII intact). After
  `resize(132, 43)`, `stty size` printed `43 132`, and `$TERM` was
  `xterm-256color`. The exit came back as `{ code: 0, killed: false }`.
- `kill()` on the ticker: 121 ms, `onExit` with
  `{ code: null, killed: true }`. Then `remove()` dropped it from
  `list()`.

## Not verified

- A full-screen TUI (Claude Code, vim) through xterm.js. The bytes path is
  the same, but nobody looked at a rendered screen.
- Kill-on-quit with a real Cmd-Q.
- Throughput against a very chatty process (`yes`, a large `npm install`).
