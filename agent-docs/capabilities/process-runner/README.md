# ProcessRunner Capability (`desktop.processRunner`)

## How it works

Spawns an executable by name + argv array (never a shell string) and
streams its stdout/stderr back to JS incrementally, as the process
produces it — not buffered to a single blob at exit. A webview has no
`child_process` equivalent, so this is native/OS territory.

`crates/core/src/process_runner.rs` owns the actual spawn and pipe
reading, via plain `std::process::Command`. It knows nothing about
Tauri — its public `run(command, args, on_output, on_exit)` spawns the
process, reads stdout and stderr on two dedicated threads (each chunk
of bytes read is forwarded to `on_output` as soon as it's available,
tagged `stdout`/`stderr`), and calls `on_exit` exactly once, after both
pipes hit EOF and the process has actually exited. Returns a
`ProcessHandle` immediately once spawned. Each process leads its own
process group, and `kill()` stops the whole tree through
`chain_core::process_tree` (SIGTERM to the group, SIGKILL after 2 s,
`TIMEOUT` if anything outlives 5 s; `taskkill /T /F` on Windows), so a
wrapper like `npm run dev` doesn't leave its server holding the port.
`process_runner_kill` runs it off the main thread. Processes still
running when the app quits normally are killed (`RunEvent::Exit` in
`templates/lib.rs`).

`RunOptions` also carries `cwd` (checked against `desktop.folders`
grants in `templates/lib.rs` before `run()` is called), `env` (set after
the login-shell `PATH`, so it can override it) and `keep_stdin_open`.
All stdin, the one-shot payload included, goes through a single writer
thread fed by a channel; `handle.write()` sends to it, and
`closeStdin()` drops the last sender, which closes stdin.

On macOS/Linux, `run()` substitutes the user's real login-shell `PATH`
(resolved once, on first use, by spawning `$SHELL -lic 'echo
...${PATH}...'` and caching the result for the process's lifetime) into
the spawned child's environment, instead of leaving `Command::new` to
resolve `command` against the app's own inherited `PATH`. This matters
because a GUI app launched by launchd/Finder — a packaged `chain build`
double-clicked from Finder/Dock — inherits a minimal system `PATH` that
never sources `.zshrc`/`.zprofile`, so a bare command like `"claude"`
that's only reachable via nvm/homebrew/asdf resolves fine under `chain
dev` (which inherits the terminal's full login-shell `PATH`) but fails
`NOT_FOUND` in the packaged build — the standard macOS GUI-app `PATH`
problem, same one Electron apps hit. See
`research/MACOS.md`'s "GUI-launched apps don't get the login-shell
PATH" for the shell-quoting gotcha this ran into (`$PATH` immediately
followed by a literal suffix merges into one variable name unless
braced as `${PATH}`) and why the resolution runs on a background
thread with a timeout rather than blocking `run()` synchronously.

`run()` also takes an optional stdin payload (`stdin: Option<String>`,
`options.stdin` on the JS side), for input too large for argv. When
given, the child's stdin is piped, a third thread writes the whole
payload and then drops the pipe, and the child sees EOF. That thread runs
alongside the stdout/stderr readers, so a multi-megabyte payload can't
deadlock against a child that prints before it has read all its input.
A write error, usually a broken pipe from a child that exited without
reading everything, is ignored: `on_exit` still reports how the process
ended. Without a payload, stdin stays the null device, as it always was
(not an empty pipe, which some CLIs treat differently).

`packages/cli/templates/lib.rs`'s `process_runner_run`/
`process_runner_kill` bridge this to the webview via
`chain://process-output`/`chain://process-exit` events — simpler than
`agent-server`'s bridge in one respect (no blocking wait for a JS reply,
since output/exit are purely outbound; the only inbound call is
`kill()`), but with one subtle correctness requirement: **the process
id is generated client-side** (`packages/sdk/src/process-runner.ts`),
not handed back from native, specifically so the JS listener for that
id can be registered *before* the native spawn ever happens. A fast
process (spawn → print → exit) can finish within a single IPC round
trip; if native generated the id, that output could already have fired
— and been lost — before JS ever learned the id to listen for it.

**Crucially, native code has zero awareness of any specific CLI or
output format** — it only understands "spawn this, stream what it
prints, tell me when it's done." An app building an AI-agent chat UI on
top of this (mneme's actual use case: Claude Code/Codex/Gemini CLI
headless-mode invocations) owns all stdout parsing itself. See
`CONTRACT.md`'s "The two open design questions" for the full reasoning
on that and on the deliberate absence of any executable allowlist.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const handle = await desktop.processRunner.run(
  "claude",
  ["-p", userMessage, "--resume", sessionId, "--output-format", "stream-json"],
  (chunk) => {
    // chunk: { stream: "stdout" | "stderr", data: string }
    // Not line-buffered — accumulate and split on "\n" yourself if you
    // need discrete lines/JSON objects.
  }
);

const exit = await handle.exited; // { code: number | null; killed: boolean }

// Input too large for argv (a whole course's content, say) goes on stdin:
// written once, then closed so the child sees EOF.
await desktop.processRunner.run("claude", ["-p", task], onOutput, { stdin: courseText });

// A desktop.files reference becomes one argv element: that managed
// file's absolute path, resolved natively — JS never sees it. Unknown
// references reject NOT_FOUND before anything spawns.
const image = await desktop.files.write(pngBytes, { extension: "png" });
await desktop.processRunner.run("codex", ["exec", "-i", { fileReference: image }, prompt], onOutput);

// To stop it early (e.g. a chat UI's "stop generating" button). This
// stops everything it started too, and resolves once all of it is gone:
await handle.kill();

// In a project folder the user granted (desktop.folders), with extra env:
const install = await desktop.processRunner.run("npm", ["install"], onOutput, {
  cwd: project.path,
  env: { PORT: "5174" }
});

// Answering a prompt while it runs:
const scaffold = await desktop.processRunner.run("npx", ["create-vite", "app"], (chunk) => {
  if (chunk.data.includes("(y/N)")) void scaffold.write("y\n");
}, { cwd: project.path, keepStdinOpen: true });
```

A nonexistent command rejects instead of resolving:

```ts
try {
  await desktop.processRunner.run("not-a-real-command", [], () => {});
} catch (e) {
  // e.code === "NOT_FOUND"
}
```

Every already-scaffolded app gets this automatically via `chain update`
(it's a tracked file in `packages/cli/templates/lib.rs`) — no manual
wiring needed per app.

## Files to check

- `agent-docs/capabilities/process-runner/CONTRACT.md` — the semantic
  contract, especially "The two open design questions" (no AI-CLI
  awareness, no compiled-in executable allowlist — that's Phase 28's
  job), `options.stdin` (one-shot payload, EPIPE isn't an error) and
  `cwd`/`env`, `keepStdinOpen`/`write()`, the tree-killing `kill()`, and
  the Non-goals (no shell interpretation ever, no PTY, no
  line-buffering, no stream-interleaving guarantee). Check this before
  changing behavior.
- `crates/core/src/process_tree.rs` — process groups, the SIGTERM →
  SIGKILL → give-up escalation, and Windows' `taskkill /T`. Shared with
  `terminal`.
- `capabilities/process-runner/contract.ts` — the exact types
  (`ProcessRunnerApi`, `ProcessRunOptions`, `ProcessHandle`,
  `ProcessOutputChunk`, `ProcessExit`); change this and every implementation below together,
  never one without the others.
- `agent-docs/capabilities/process-runner/AGENTS.md` — what's already
  decided and why (especially the client-side-id race-avoidance
  reasoning), and the actual TODO checklist (Windows spawn-shim
  decision, contract tests, mneme's own chat-UI build-out).
- `agent-docs/capabilities/process-runner/research/MACOS.md` /
  `research/WINDOWS.md` — the Windows note has a **real, unresolved**
  risk: npm-global-installed CLIs are often `.cmd`/`.ps1` shims on
  Windows, and `Command::new` doesn't do PATHEXT-aware extension search
  the way `cmd.exe` does. Three candidate fixes are documented, none
  chosen yet — read before touching the spawn path on Windows.
- `crates/core/src/process_runner.rs` — the actual Rust implementation
  (`run()`, `ProcessHandle`, `ProcessExit`, `ProcessRunnerError`). Has
  real unit tests (`cargo test -p chain-core`) including a genuine
  timing-based proof that output arrives incrementally, not buffered to
  exit — extend tests here rather than only through the Tauri layer.
- `packages/sdk/src/process-runner.ts` — the SDK wrapper. The subtle
  part: it generates the process id itself and registers a
  module-level, shared listener entry for it *before* invoking the
  native `process_runner_run` command — see its comments for why getting
  this ordering backwards would silently lose fast processes' output.
- `packages/cli/templates/lib.rs` — the Tauri command/event layer
  (`process_runner_run`/`process_runner_kill`, `ProcessRunnerState`)
  that every scaffolded app gets. This is the file `chain update`
  propagates — see `agent-docs/framework/command/README.md`.
- `apps/playground/src-tauri/src/lib.rs` — same wiring, kept in sync by
  hand (playground isn't `chain init`-managed) so the framework's own
  proof app demonstrates every capability, not just the earlier ones.
