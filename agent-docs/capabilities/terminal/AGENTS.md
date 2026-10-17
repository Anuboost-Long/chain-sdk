# Terminal Capability — Agent Memory

Scope: `desktop.terminal` — interactive programs in a pseudo-terminal,
owned by Chain Core so they outlive a page reload, with numbered output,
a bounded backlog, gap-free reattach, write, resize and tree kill.
Requested by Lazify (request 04 in `lazify-chain/docs/chain-sdk-requests/`),
whose Phase 4 exit ("run a command, give an agent a task, reload,
reconnect") depends on it.

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `capabilities/terminal/contract.ts`
5. `research/MACOS.md` / `research/WINDOWS.md`
6. `crates/core/src/terminal.rs`, `crates/core/src/process_tree.rs`,
   `packages/cli/templates/terminal.rs`, `packages/sdk/src/terminal.ts`

## What's already decided

- **Named `terminal`** (`desktop.terminal`): what it means to an app
  developer, not "pty".
- **Sessions live in `Terminals`, held in Tauri state** (`TerminalState`
  in `templates/terminal.rs`), so a webview reload doesn't touch them.
  This is the property the whole request exists for.
- **`portable-pty`** for openpty/ConPTY. No Swift or .NET.
- **Numbering, retaining and reporting happen together under the
  backlog lock** (`retain_and_report`), so `backlog()` plus later live
  chunks never overlap or leave a gap. Output is retained whether or not
  anything listens.
- **The SDK's `attach()` does the reattach dance once, correctly:**
  listen, read the backlog, hold live chunks that arrive before the
  backlog reply, forward only `seq > backlog.seq`. Lazify's own
  terminal pool did this by hand; every app would otherwise re-derive it.
- **One native event per chunk** (`chain://terminal-output`), multiplexed
  to any number of JS subscribers. Never a second process for a second
  viewer.
- **Session ids are native-generated** (`term-<counter>-<random>`),
  unlike process-runner's client-side ids: no output can be lost before
  the page knows the id, because it's all in the backlog.
- **Exited sessions stay listed until `remove()`**, so a page that
  reloaded during the exit can still read how it ended.
- **Interactive stdin over a pipe stayed in `process-runner`**
  (`keepStdinOpen`). This capability is for programs that need a real TTY.
- **`kill()` is `process_tree::terminate`**, the same guarantee as
  process-runner. The program leads its own session (`setsid` in
  portable-pty), so its group is everything it started, unless something
  left the group on purpose.
- **Blocking work is off the main thread** — `start` (the first one
  resolves the login-shell `PATH`, up to 3 s), `write` (a paste into a
  program that isn't reading can block) and `kill` (up to 5 s).

## Status

**Implemented and verified end to end on macOS**, 9 October 2026. Not
on Windows (rule 3).

- `crates/core/src/terminal.rs`: 7 unit tests. They cover a real TTY
  (`test -t`), the requested size and `TERM`; contiguous seqs matching
  the backlog; keystrokes and resize reaching the program, with
  `UNAVAILABLE`/`NOT_FOUND` afterwards; a character split across reads;
  the backlog trimming whole chunks; kill stopping a wrapper's child and
  reporting `killed`; and `remove`/missing program/empty command.
- End to end in a throwaway `chain init` app under `chain dev`: a session
  survived `location.reload()` and kept producing output, and `attach()`
  from the new page delivered ticks 1–130 contiguous with no repeats.
  Write (non-ASCII), resize, `$TERM`, exit, `kill()` (121 ms,
  `killed: true`) and `remove()` all worked. See `research/MACOS.md`.

## What's NOT done yet

- [ ] A full-screen TUI (Claude Code, vim) rendered with xterm.js in an
      app, including bracketed paste.
- [ ] Kill-on-quit with a real Cmd-Q.
- [ ] Throughput test with a very chatty program; tune `BATCH_WINDOW` /
      `BATCH_BYTES` only if events back up.
- [ ] Windows: `research/WINDOWS.md`'s checklist (ConPTY, `.cmd` shims,
      tree kill).
- [ ] Contract tests under `capabilities/terminal/tests/`.
- [ ] Playground UI demo (the playground's Rust wiring is in sync; no UI
      exercises it yet).

## Rules specific to this capability

- Never parse or interpret output here. No prompt detection, no escape
  sequence handling.
- Never report a chunk without retaining it first, or outside the
  backlog lock.
- Never cut the backlog inside a chunk.
- Never accept a shell string — argv only, as in process-runner.
