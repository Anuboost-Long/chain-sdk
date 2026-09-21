# ProcessRunner Capability — Contract

## What this is

Spawns an external executable by name and argument array, and streams
its stdout/stderr back to JS incrementally as the process produces
output — not buffered to a single blob at exit. A webview has no
`child_process` equivalent, so this is native/OS territory, same
reasoning behind every prior capability here.

Backs mneme's in-app chat UI backed by AI coding agent CLIs the user
already has installed and authenticated (Claude Code, OpenAI Codex,
Gemini CLI, or an arbitrary user-configured custom command) — each chat
turn is one process invocation (e.g. `claude -p "<message>" --resume
"<id>" --output-format stream-json --include-partial-messages`) whose
stdout is newline-delimited JSON mneme parses itself for token-by-token
display. This capability has no idea any of that is happening — it only
knows how to run a program and hand back what it prints.

## The two open design questions, resolved

The request that drove this flagged two real forks rather than assuming
answers:

**1. Does this capability know it's "for AI CLIs" at all?**

**No — fully generic.** `run(command, args, onOutput)` doesn't parse
stdout, doesn't know about `stream-json`, session ids, or that
Claude/Codex/Gemini exist. This mirrors `agent-server`'s own precedent
exactly (zero MCP/JSON-RPC awareness there) and the same line every
capability here draws: `http` doesn't parse HTML, `files` doesn't know
what a MIME type means to the app, `agent-server` doesn't parse
JSON-RPC. A caller with a completely unrelated need — running `git
diff`, a build tool, anything that prints to stdout — gets the exact
same primitive `mneme` does for its chat turns.

**2. Should chain-sdk bake in a restriction on which executables can be
spawned (e.g. a caller-supplied allowlist)?**

**No — this capability trusts its caller, the same trust boundary
`desktop.storage`'s raw SQL execution and `desktop.files`'
capability-generated-reference storage already sit at.** The executable
name and arguments are entirely caller-supplied at runtime (the request
is explicit: a handful of built-in presets *plus* an open-ended,
user-typed custom command — not a small fixed set), so any allowlist
this capability enforced would have to be caller-supplied too, at which
point it isn't really a safety boundary chain-sdk is providing, just
bookkeeping the caller could do itself before calling `run()`. Deciding
*whether an app should be allowed to spawn a given process at all* —
prompting the user, remembering a trust decision, confirming a
destructive action — is exactly Phase 28's ("Agent Permission System")
job, the same non-goal `agent-server`'s CONTRACT.md already draws around
its own "no auth beyond `127.0.0.1`." This capability answers "can the
app run a process and read its output," not "should it be allowed to."

One thing this is **not** a policy call about, and stays a hard
invariant regardless: **no shell interpretation, ever.** See Non-goals.

## `desktop.processRunner.run(command, args, onOutput)`

```
run(
  command: string,
  args: string[],
  onOutput: (chunk: { stream: "stdout" | "stderr"; data: string }) => void
): Promise<{
  id: string;
  kill(): Promise<void>;
  exited: Promise<{ code: number | null; killed: boolean }>;
}>
```

Spawns `command` (resolved via `PATH`, the OS's normal executable
lookup — never a path chain-sdk or the app constructs by shell
expansion) with `args` as a literal argument array passed straight to
the OS process-creation call. **Never** a shell string built by
concatenation — `args` elements are never interpreted, escaped, or
tokenized by a shell; a user's chat message becomes exactly one argv
element, verbatim, whatever characters it contains.

`run()`'s promise resolves once the process has actually started,
with a handle — not once it finishes. `handle.id` is an opaque
identifier for this running process. `handle.exited` is a promise that
resolves once the process exits, **however** it exits (a normal exit,
a non-zero exit, or `kill()`) — it never rejects; a failure to even
launch the process rejects `run()` itself instead, before a handle ever
exists.

`onOutput` fires once per chunk of data read from either stream, as
soon as it's available — not buffered to process exit, and not
line-buffered either. **Chunk boundaries carry no meaning**: a chunk is
whatever bytes one native read returned, which may split a line across
two chunks, combine several lines into one chunk, or anything in
between. A caller that needs discrete lines (e.g. one JSON object per
line) accumulates chunks itself and splits on `\n` — this capability
doesn't assume the child's output is line-oriented, because assuming
that would itself be a small amount of AI-CLI-shaped awareness leaking
into a supposedly generic primitive.

`stdout` and `stderr` are each delivered in the order the process
produced them, but **interleaving between the two streams is not
guaranteed to reflect true wall-clock order** — both are read
independently, so a stdout chunk and a stderr chunk that were emitted
by the process only microseconds apart may arrive to `onOutput` in
either order.

## `handle.kill()`

```
kill(): Promise<void>
```

Terminates the process. **Idempotent** — killing a process that has
already exited (including one that already exited on its own) resolves
successfully rather than rejecting, same reasoning `files.delete()` and
`agentServer.stop()` already use. After `kill()`, `handle.exited`
resolves with `{ killed: true }`.

## Errors

- `run()` rejects with `ChainError { code: "INVALID_ARGUMENT" }` if
  `command` is empty.
- `run()` rejects with `ChainError { code: "NOT_FOUND" }` if `command`
  can't be resolved to an executable at all (no such command on `PATH`)
  — mirrors `files`' own `NOT_FOUND` for "the thing you referenced
  doesn't exist," and matches the OS-level `ENOENT` this condition
  actually is.
- `run()` rejects with `ChainError { code: "PERMISSION_DENIED" }` if the
  OS refuses to execute the resolved file (exists, but isn't
  executable/isn't permitted).
- Any other unexpected native failure to spawn rejects with
  `ChainError { code: "NATIVE_FAILURE" }`.
- Calling `run()` or `kill()` outside a Chain (Tauri) runtime rejects
  with `ChainError { code: "UNSUPPORTED" }`, same as every other
  capability.
- **A non-zero exit code is not an error.** `run()`'s promise resolving
  and `handle.exited` resolving only describe whether the process could
  be *observed* running to completion — same "resolving isn't success"
  shape `http.get()` already established for HTTP status codes. A
  process that exits `1` (or crashes, or is killed) still resolves
  `exited` with that real `code`/`killed` — the caller decides what a
  given exit code means for its own use case.

## Non-goals

- **No shell interpretation, ever.** `args` is always passed as a
  literal array to the OS's process-creation call, never joined into a
  string and handed to a shell. This is the one thing in this
  capability that is **not** a policy call left to the app (contrast
  with the allowlist question above) — it's a hard invariant with no
  legitimate use case on the other side of it, given the request itself
  requires argv-array spawning.
- **No caller-executable allowlist, no permission/trust model.** See
  "The two open design questions" above — this is deliberately Phase
  28's job, not incremental scope on this capability.
- **No AI-CLI awareness whatsoever.** No stdout parsing, no knowledge of
  `stream-json`, session ids, or any specific agent CLI's flags. See
  "The two open design questions" above.
- **No stdin, no persistent/interactive process, no PTY.** Each `run()`
  call is a one-shot process that runs to completion or is killed —
  there is no way to write further input into it after spawn. If a real
  need for an interactive/long-lived process with stdin shows up later,
  that's a different, future capability decision (rule 7 — don't build
  for a hypothetical need), not something bolted onto this one.
- **No environment-variable or working-directory options (yet).** The
  child inherits the app's own environment and working directory;
  nothing about either is configurable. Add this only when a real need
  shows up, not speculatively.
- **No output buffering limits or backpressure.** Chunks are forwarded
  as fast as the process produces them and the native side reads them —
  no max-chunk-size, no rate limiting, no coalescing. A process that
  produces output far faster than the JS side can usefully consume it is
  the caller's problem to manage (e.g. debouncing UI updates), not this
  capability's.
- **No stream-interleaving ordering guarantee across stdout/stderr.**
  See the "run()" section above — each stream is in-order internally,
  the two streams relative to each other are not.
- **No binary-safe / arbitrary-encoding output.** `data` is always
  UTF-8-decoded text (lossily — invalid byte sequences become U+FFFD),
  never raw bytes. This capability is for text-producing CLIs; a process
  whose stdout is meaningfully binary is out of scope.
- **No process groups / no killing a tree of child processes.**
  `kill()` terminates the spawned process itself; whether that process's
  own children (if any) are also terminated depends on OS/process
  behavior this capability makes no promise about.
