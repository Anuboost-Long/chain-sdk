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
`ProcessHandle` immediately once spawned, whose `kill()` is a thin
wrapper over `std::process::Child::kill()` (already portable —
`SIGKILL` on Unix, `TerminateProcess` on Windows, no per-OS code
needed).

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

// To stop it early (e.g. a chat UI's "stop generating" button):
await handle.kill();
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
  job) and the Non-goals (no shell interpretation ever, no stdin/PTY, no
  line-buffering, no stream-interleaving guarantee). Check this before
  changing behavior.
- `capabilities/process-runner/contract.ts` — the exact types
  (`ProcessRunnerApi`, `ProcessHandle`, `ProcessOutputChunk`,
  `ProcessExit`); change this and every implementation below together,
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
