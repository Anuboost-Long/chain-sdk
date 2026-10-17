# Terminal Capability (`desktop.terminal`)

## How it works

`start()` opens a pseudo-terminal (`portable-pty`: openpty on macOS,
ConPTY on Windows) and runs the program in it as the leader of its own
session. That gives it a real TTY, a window size and raw keystrokes. The
session lives in Chain Core (`Terminals`, held in Tauri state), not in
the page, so it keeps running when the webview reloads.

A reader thread decodes the output as UTF-8 statefully. A delivery
thread batches what arrives within 4 ms, numbers each chunk (`seq`),
appends it to the session's backlog (default 512 KB, trimmed by whole
chunks) and emits `chain://terminal-output`. Those three steps happen
under one lock, so a page can read the backlog and then continue with
live chunks without a gap or a repeat. The SDK's `attach()` does exactly
that.

`kill()` stops the program's whole process tree
(`crates/core/src/process_tree.rs`, shared with process-runner). Exited
sessions stay in `list()` until `remove()`. Quitting the app kills all of
them.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const session = await desktop.terminal.start({
  command: "claude",
  args: [],
  cwd: project.path, // inside a desktop.folders grant
  env: { LAZIFY_LINT: "1" },
  cols: 220,
  rows: 50,
  label: "Claude Code",
  metadata: { project: project.id, agent: "claude" }
});

// Show it (e.g. in xterm.js). attach() replays the backlog, then goes live.
const detach = await desktop.terminal.attach(session.id, (chunk) => xterm.write(chunk.data));
xterm.onData((data) => void desktop.terminal.write(session.id, data));
xterm.onResize(({ cols, rows }) => void desktop.terminal.resize(session.id, cols, rows));

// After a page reload: find the sessions again and reattach.
for (const s of await desktop.terminal.list()) {
  if (!s.exit) await desktop.terminal.attach(s.id, render(s));
}

// App-wide observers (attention detection, keep-awake):
const stopOutput = desktop.terminal.onOutput((chunk) => detector.feed(chunk));
const stopExit = desktop.terminal.onExit((id, exit) => markDone(id, exit));

await desktop.terminal.kill(session.id); // the whole tree; resolves once it's gone
await desktop.terminal.remove(session.id); // frees the backlog
```

Errors: `INVALID_ARGUMENT`, `NOT_GRANTED` (`cwd`), `NOT_FOUND` (program
or session), `PERMISSION_DENIED`, `UNAVAILABLE` (the session has exited),
`TIMEOUT` (`kill`). See `CONTRACT.md`.

Every scaffolded app gets this through `chain update`
(`.chain/native/src/terminal.rs`).

## Files to check

- `agent-docs/capabilities/terminal/CONTRACT.md` — sequence numbers, the
  backlog, `attach()`'s guarantee, lifetime and non-goals.
- `capabilities/terminal/contract.ts` — the exact types.
- `crates/core/src/terminal.rs` — sessions, the reader/delivery threads,
  stateful UTF-8, the backlog, and the unit tests.
- `crates/core/src/process_tree.rs` — the tree kill.
- `packages/cli/templates/terminal.rs` (copy in
  `apps/playground/src-tauri/src/`) — Tauri commands and the two events.
- `packages/cli/templates/lib.rs` — registration, and killing every
  session on `RunEvent::Exit`.
- `packages/sdk/src/terminal.ts` — shared listeners and `attach()`.
- `agent-docs/capabilities/terminal/research/` — macOS findings and the
  Windows checklist.
