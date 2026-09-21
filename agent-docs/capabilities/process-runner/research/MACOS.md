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
