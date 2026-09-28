# ProcessRunner — macOS Research

Source: initial survey in `docs/chain-sdk-requests/10-subprocess-runner.md`
(mneme's capability request), expanded at contract time. No
implementation exists yet (see `AGENTS.md`'s Status) — nothing below has
been exercised on real hardware.

## Approach

`std::process::Command::new(command).args(args)` resolves `command`
through `PATH` the normal Unix way (an `execvp`-family lookup) — no
macOS-specific behavior, no native Swift/Foundation code needed. This is
the same category `http`'s `reqwest` and `storage`'s `rusqlite` already
fall into: a portable Rust standard-library operation, not an OS
surface requiring a native adapter.

## Streaming stdout/stderr incrementally

`Command::stdout(Stdio::piped())` / `.stderr(Stdio::piped())` give
`ChildStdout`/`ChildStderr` handles implementing `Read`. Reading each on
its own dedicated thread (one thread per stream, forwarding each
successful `read()` call's bytes to the dispatch callback as a chunk) is
the natural shape — mirrors `agent_server.rs`'s own
one-thread-per-concern approach (its accept loop runs on a dedicated
thread) rather than pulling an async runtime into `crates/core`'s
production path. No macOS-specific reasoning needed here either; two
blocking `Read::read()` loops on two plain `std::thread::spawn` threads
is portable.

## Killing a process

`std::process::Child::kill()` is already a portable, cross-platform
method in the standard library — sends `SIGKILL` on Unix, calls
`TerminateProcess` on Windows. No per-OS code needed for `kill()`
itself; see `research/WINDOWS.md` for the actual platform-divergent risk
(which is about *spawning*, not killing).

## GUI-launched apps don't get the login-shell `PATH`

Confirmed real (not hypothetical) once mneme actually shipped a
packaged build: a macOS GUI app launched by launchd (double-clicked
from Finder/Dock, or via `chain build`'s output) inherits a minimal
system `PATH` — `.zshrc`/`.zprofile` are never sourced, because those
only run for an interactive login shell, which launchd's spawn is not.
`chain dev` doesn't have this problem because the dev process inherits
the *terminal's* `PATH`, which already went through a login shell. So a
bare command name only reachable via nvm/homebrew/asdf/cargo (exactly
the shape `claude`, `codex`, `gemini` take when installed via a version
manager) resolves under `chain dev` and fails `NOT_FOUND` in a packaged
build — same as the long-standing Electron GUI-app-PATH problem
(usually patched with the `fix-path-env` crate/npm package).

Fix: resolve the real login-shell `PATH` once by spawning
`$SHELL -lic '...'` and reading its `$PATH`, then use that resolved
value for the child's `PATH` env var instead of the app's own inherited
`PATH`. Two things worth remembering if this is ever revisited:

- **`$PATH` needs bracing when a literal suffix follows it in the same
  shell string.** `echo -n START$PATH END` (unbraced, no space) is fine,
  but `echo -n START$PATHEND` — no separator before the suffix — is
  parsed by the shell as one variable name, `PATHEND`, which is unset
  and silently expands to nothing. Concretely: wrapping the resolved
  value in start/end markers to survive rc-file banner noise on stdout
  (`echo -n {START}$PATH{END}`) silently produced `{START}` with nothing
  after it, because `$PATH{END}`'s literal `{END}` isn't a shell
  metacharacter that stops variable-name parsing the way whitespace
  does. Fixed by bracing: `${PATH}` unambiguously ends the variable
  reference regardless of what immediately follows it.
- **Don't block `run()` synchronously on the shell spawn.** A login
  shell that hangs (network-dependent rc file, broken prompt hook, etc.)
  shouldn't stall every process spawn indefinitely — resolve on a
  background thread with a short timeout (3s), falling back to the
  app's inherited `PATH` if it doesn't return in time. Cache whatever
  the result is (`Some` or `None`) for the process's lifetime either
  way; don't retry per spawn.

## One-shot stdin payload (`options.stdin`)

Added for mneme's request 12. macOS findings behind it:

- **Why argv isn't enough.** `getconf ARG_MAX` is 1,048,576 bytes on
  macOS, and that covers all arguments *plus* the environment. (Linux
  also caps one argument at `MAX_ARG_STRLEN` = 131,072 bytes; Windows
  caps the whole command line at 32,767 characters.)
- **Pipe buffer.** A macOS pipe holds about 64 KB. Writing a larger
  payload blocks until the child reads, so the write runs on its own
  thread, alongside the stdout/stderr readers. Otherwise a child that
  echoes as it reads (like `cat`) fills its stdout pipe while we're
  still blocked writing its stdin, and the two deadlock.
- **Broken pipe is a return value, not a crash.** The Rust runtime sets
  `SIGPIPE` to ignored in every Rust binary (the Tauri app and `cargo
  test` included), so writing to a child that already exited returns
  `EPIPE` from `write_all` instead of killing the app. We discard that
  error. `std` restores `SIGPIPE` to its default in the spawned child,
  so the child itself behaves normally.
- **Null vs. pipe is observable.** `[ -p /dev/stdin ]` is false for a
  child spawned without a payload (`Stdio::null()`) and true with one.
  That's why "no payload" stays null and never becomes an empty pipe.

Verified end to end in `apps/playground`'s real window (`npm run tauri
dev`, temporary probe logging with `console.error`, then reverted),
through the SDK and IPC, not just `chain-core`:

| Case | Result |
|---|---|
| 3 MB payload ending in `\0end` → `/bin/cat` | `{code: 0}`, 3,145,732 of 3,145,732 chars back, ~0.9 s |
| `"héllo PELICAN"` → `/bin/cat` | echoed exactly, `{code: 0}` |
| No payload → `/bin/cat` | exits at once, no output (immediate EOF) |
| No payload → `[ -p /dev/stdin ]` | `not-a-pipe` |
| Payload → `[ -p /dev/stdin ]` | `pipe` |
| 2 MB payload → `sh -c "exit 3"` (never reads) | `{code: 3, killed: false}`, `run()` didn't reject |

Not run through this path: a real `claude -p` / `codex exec` with piped
stdin. mneme verified `claude -p` with a shell pipe (same OS-level
shape), and Codex was left unrun to avoid spending the user's quota.

## App Sandbox / entitlements

Not relevant to local `chain dev`/unsigned-build testing (same caveat
every other capability's research already carries), but worth naming
for completeness: spawning arbitrary executables is restricted or
disallowed entirely under the macOS App Sandbox (Mac App Store
distribution) regardless of entitlements — a sandboxed build would need
this capability to fail gracefully (`PERMISSION_DENIED` or
`NATIVE_FAILURE`) rather than silently do nothing, if `mneme` is ever
distributed that way. Not a concern for the current unsigned/direct-
distribution build.

## Not yet verified (no implementation exists yet)

- No unit tests exist yet (`crates/core/src/process_runner.rs` isn't
  written).
- Real end-to-end verification (spawning a real process, e.g. `/bin/cat`
  or a tiny shell script that prints incrementally with delays, and
  confirming chunks actually arrive before the process exits rather than
  being silently buffered to EOF).
- Whether killing a process that has already spawned children of its own
  (unlikely for `claude -p`-style one-shot CLIs, but not something this
  research has confirmed either way) leaves orphans — see CONTRACT.md's
  Non-goals on process groups.

## File-reference arguments (request 14)

Source: mneme's `docs/chain-sdk-requests/14-process-runner-file-arguments.md`.
`codex exec` only takes images as `-i/--image <FILE>`, a path on disk.
The `files` contract never gives JS a real path, so `run()` accepts an
`args` element of `{ fileReference }` and native substitutes the managed
file's absolute path as exactly one argv element.

- Resolution happens in the Tauri command layer (`templates/lib.rs`),
  which already owns `FilesState`, via `chain_core::files::Files::process_path`
  — any reference that doesn't resolve to an existing managed file
  (malformed, deleted, never issued) is `NOT_FOUND`, and `run()` rejects
  before anything is spawned.
- On macOS/Linux the path is used as is: `ARG_MAX` is about the whole
  argv, and an app-data path is far below any per-argument limit.
- The path is passed through `Command::args` like every other element —
  no shell, no quoting, spaces in `~/Library/Application Support/...`
  are fine.
