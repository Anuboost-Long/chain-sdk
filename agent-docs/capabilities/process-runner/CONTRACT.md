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

## `desktop.processRunner.run(command, args, onOutput, options?)`

```
run(
  command: string,
  args: string[],
  onOutput: (chunk: { stream: "stdout" | "stderr"; data: string }) => void,
  options?: { stdin?: string }
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

On macOS/Linux, `PATH` resolution uses the user's real login-shell
`PATH` (resolved once via the user's `$SHELL`, cached for the app's
lifetime), not the minimal `PATH` a GUI app launched by launchd/Finder
inherits by default. This is still "the OS's normal executable
lookup," not a chain-sdk-specific search path — it exists so a bare
command name (e.g. `"claude"`, installed via nvm/homebrew/asdf) resolves
the same way whether the app was started from a terminal (`chain dev`)
or double-clicked from Finder/Dock (a packaged build). If the
login-shell `PATH` can't be resolved for any reason, this falls back to
the app's inherited `PATH`, same as before.

### File-reference arguments

```
type ProcessArg = string | { fileReference: string };
```

An `args` element may be a `desktop.files` reference instead of a
string. Native replaces it with **exactly one argv element**: that
managed file's absolute path, resolved when the argv is built. JS never
sees the path — the `files` contract's no-real-paths rule holds.
Driven by mneme's request 14: `codex exec` takes images only as
`-i <FILE>`, a path on disk.

- Same literal-argv rule as every other element: no shell, no
  splitting, no substitution **inside** a string (`"see {0}"` is just
  that string).
- A reference that doesn't resolve to an existing managed file rejects
  `run()` with `ChainError { code: "NOT_FOUND" }` **before** anything is
  spawned.
- On Windows, a path of 260 characters or more is passed in its
  `\\?\` extended-length form (the `MAX_PATH` reason `files` hides
  paths at all). Whether the child program accepts that form is up to
  the child.
- No lifecycle: the capability doesn't create or delete the file. The
  app writes it with `files.write()` and deletes it with
  `files.delete()` after `exited` resolves.
- Plain string elements behave exactly as before; existing callers are
  unchanged.

### `options.stdin` — a one-shot input payload

When `options.stdin` is given, the process is spawned with a piped
stdin, the whole string is written to it as UTF-8, and stdin is then
**closed**, so the process sees EOF. That's the whole interaction:
nothing more can be written afterwards. It exists for input too large
for argv (macOS `ARG_MAX` is ~1 MB for all args plus the environment,
Linux caps a single argument at 128 KB, Windows caps the whole command
line at 32,767 characters). Driven by mneme's request 12 (sending a
whole module's or course's content to an agent CLI).

- The payload is written **concurrently** with reading stdout/stderr,
  never before it, so a payload bigger than the OS pipe buffer can't
  deadlock against a process that starts printing before it has read
  all of its input. Multi-megabyte payloads are supported.
- If the process exits or closes its stdin before reading everything
  (a broken pipe), that is **not** an error: `run()` still resolved
  when the process started, and `exited` resolves as normal with the
  real `code`. Same "resolving isn't success" rule as non-zero exits.
- The string is data, not argv: any character is allowed, including
  `\0`.
- When `options.stdin` is omitted (or `options` is), the process gets
  **no stdin at all** (the null device, so an immediate EOF), exactly
  as before this option existed. It is deliberately not an empty pipe,
  since some CLIs behave differently when stdin is a pipe.

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
- `run()` rejects with `ChainError { code: "NOT_FOUND" }` if an `args`
  element's `fileReference` doesn't resolve to an existing managed file.
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
- **No interactive stdin, no persistent/interactive process, no PTY.**
  Each `run()` call is a one-shot process that runs to completion or is
  killed. The only input it can get is `options.stdin`, written once and
  closed at spawn; there is no way to write further input into it after
  that (no `handle.write()`). Nor is there a binary (`Uint8Array`) payload,
  only a string. If a real
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
