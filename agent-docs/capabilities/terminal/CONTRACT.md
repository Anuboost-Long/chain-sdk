# Terminal Capability — Contract

## What this is

Runs an interactive program in a pseudo-terminal: a real TTY on the
program's side, a window size, and raw keystrokes in. Sessions are
**owned by Chain Core, not the page.** They keep running and keep
buffering their output while the webview reloads or navigates, and a
page finds them again with `list()` and picks up where it left off with
`attach()`.

Driven by Lazify's request 04. Its main surfaces are dev servers with
coloured, redrawing output and AI agent CLIs with full-screen TUIs that
read keystrokes and resize with the window. The terminal *is* the
product, so these can't run headless the way Mneme's agents do through
`process-runner`.

This capability knows nothing about what the output means. Lazify's
attention detection, autopilot and usage parsing all stay in the app and
read the stream.

## `start(options)`

```
start({
  command, args?, cwd?, env?, cols? = 80, rows? = 24,
  label?, metadata?, backlogBytes? = 524288
}): Promise<TerminalSession>
```

- `command` + `args` is an argv array, never a shell string, the same
  rule as `process-runner`. `command` is found on the login-shell `PATH`
  (see `process-runner`'s CONTRACT.md), so a Finder-launched app finds
  `claude` or `npm` too.
- `cwd`, if given, must be an existing folder inside a `desktop.folders`
  grant (`NOT_GRANTED` / `NOT_FOUND` / `INVALID_ARGUMENT` otherwise).
- The environment is the app's, then the login-shell `PATH`, then
  `TERM=xterm-256color`, then `env`, so `env` can override any of them.
- `label` and `metadata` are the app's own. They come back in `list()`,
  which is how Lazify keeps a session's project and agent identity
  without a side table.
- The program starts as the leader of a new session and process group,
  with the pty as its controlling terminal.

Resolves once the program has started, with its `TerminalSession`
(`exit: null`). The id is unique for the app's lifetime and never reused.

## Output: sequence numbers and the backlog

- Output is UTF-8, decoded **statefully**: a character split across two
  reads is held back until it's complete, never turned into `�`. Bytes
  that are genuinely invalid become `�`.
- Output arriving within ~4 ms is batched into one chunk, up to 64 KB, so
  bursts from TUIs or `npm install` don't become thousands of events.
  Chunk boundaries carry no meaning.
- Each chunk gets a `seq`: 1 for the session's first, then +1 per chunk.
- Every chunk is kept in the session's **backlog** before it's reported,
  **whether or not anything is listening**. Nothing is lost while no page
  is attached. The backlog keeps the most recent chunks whose total is at
  most `backlogBytes`, dropping whole chunks from the front so an escape
  sequence is never cut in half. The newest chunk is always kept, even if
  it's bigger than the limit.
- `backlog(id)` returns `{ data, seq }`: everything retained, and the
  `seq` of its newest chunk (`0` before any output).

### `attach(id, onOutput)` — gap-free reattach

Starts listening, then reads the backlog, then delivers the backlog as
one chunk (carrying the backlog's `seq`), then every live chunk after
it. Nothing is delivered twice and nothing is skipped, even though a live
chunk can reach the page before the backlog's own reply. Resolves, with
an unsubscribe function, once the backlog has been delivered. Use this
after a reload, or whenever a view opens on a running session.

### `onOutput(handler)` and `onExit(handler)`

Every live chunk of every session, and every exit, from the moment of
subscribing. This is for app-wide observers (attention detection,
keep-awake), which rebuild their state from `backlog()` after a reload
and then ignore chunks at or below that `seq`. Any number of
subscribers. Native reports each chunk once, and there's only ever one
process per session, however many views show it.

## Input, size and lifetime

- `write(id, data)` sends `data` to the program as typed: keystrokes,
  pasted text, and escape sequences such as bracketed paste. Rejects
  `NOT_FOUND` for an unknown session and `UNAVAILABLE` once it has
  exited, so a failure is never silent.
- `resize(id, cols, rows)` sets the window size; the program gets
  `SIGWINCH`. Same errors, and `INVALID_ARGUMENT` for a zero size.
- `kill(id)` stops the session's whole process tree, the same guarantee
  as `process-runner`'s `kill()`: SIGTERM to the group, SIGKILL after
  2 s, rejects `TIMEOUT` if anything is still alive at 5 s. Resolves once
  it's all gone. Idempotent: an exited session resolves.
- When the program exits, `onExit` receives `{ code, killed }` after the
  session's last chunk. `code` is `null` when a signal ended it, and
  `killed` is `true` if `kill()` or `remove()` caused it. The session
  stays in `list()` with its `exit` set, and its backlog stays readable,
  so a page that was reloading at the time can still find out how it
  ended.
- `remove(id)` forgets the session and frees its backlog, killing it
  first (immediately, no grace period) if it's still running.
  Idempotent. Exited sessions stay until removed, so the app should remove
  the ones it no longer shows.
- If the program exits but a background job still holds the terminal,
  output keeps being read for up to one second before the exit is
  reported.
- When the app quits normally, every running session is killed.

## `list()`

Every session not yet removed, running or exited, oldest first:
`{ id, label, metadata, command, args, cwd, pid, startedAtMs, cols,
rows, exit }`.

## Errors

| Code | When |
| --- | --- |
| `INVALID_ARGUMENT` | Empty `command`, a zero size, or a `cwd` that isn't a folder. |
| `NOT_GRANTED` | `cwd` is outside every `desktop.folders` grant. |
| `NOT_FOUND` | No such program, a missing `cwd`, or no session with that id. |
| `PERMISSION_DENIED` | The program exists but can't be executed. |
| `UNAVAILABLE` | `write`/`resize` on a session that has exited. |
| `TIMEOUT` | `kill` couldn't stop the whole tree within 5 s. |
| `UNSUPPORTED` | Outside a Chain runtime. |
| `NATIVE_FAILURE` | Anything else (opening the pty failed, ...). |

## Non-goals

- **No terminal emulation or rendering.** The app renders the stream
  (Lazify uses xterm.js). Chain never interprets escape sequences.
- **No output parsing** — no prompt, attention or completion detection.
- **No shell strings.** To run a shell command line, start the shell
  (`zsh`, `-l`, `-c`, `"<line>"`) as argv; the app takes responsibility
  for what's in it.
- **No persistence across app restarts.** Sessions die with the app, as
  they did in Lazify's Electron app.
- **No byte-for-byte parity across OSes.** Windows' ConPTY re-renders
  output with its own escape sequences; visual parity is the goal.
- **No configurable kill signal or timings, and no `backlog` paging** —
  nothing asked for them.
