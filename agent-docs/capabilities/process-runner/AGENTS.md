# ProcessRunner Capability — Agent Memory

Scope: `desktop.processRunner.run(command, args, onOutput)` — spawns an
executable by name + argv array and streams its stdout/stderr back to a
JS callback incrementally, returning a handle with `kill()` and an
`exited` promise. Requested by mneme (see
`docs/chain-sdk-requests/10-subprocess-runner.md` in the mneme repo) as
its tenth real capability gap: an in-app chat UI backed by AI coding
agent CLIs the user already has installed/authenticated (Claude Code,
Codex, Gemini CLI, or an arbitrary user-configured custom command) needs
to spawn one process per chat turn and show its output as it streams,
not after the process exits.

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `contract.ts`
5. `research/MACOS.md` / `research/WINDOWS.md`
6. Relevant source in `crates/core/src/process_runner.rs` /
   `packages/sdk/src/process-runner.ts` / `packages/cli/templates/lib.rs`

## What's already decided

- **Fully generic — no AI-CLI awareness.** No stdout parsing, no
  `stream-json` knowledge, no session-id concept, no idea Claude/Codex/
  Gemini exist. Mirrors `agent-server`'s zero-MCP-awareness precedent
  exactly. See `CONTRACT.md`'s "The two open design questions" — this
  was the request's own lean, confirmed here as the actual decision.
- **No caller-executable allowlist, no permission/trust model.** The
  executable name/args are fully caller-supplied at runtime (a handful
  of presets *plus* an open-ended user-typed custom command), so this
  capability trusts its caller the same way `desktop.storage`'s raw SQL
  and `desktop.files`' reference storage already do. Restricting *which*
  processes an app may spawn is explicitly Phase 28's ("Agent
  Permission System") job — don't add allowlist/confirmation logic here
  incrementally. This was the one genuinely open call in the request
  (it explicitly said "chain-sdk's call, not assumed") — resolved with
  full rationale in `CONTRACT.md`.
- **No shell interpretation, ever — the one non-negotiable invariant**,
  unlike the allowlist question above which really was a policy choice.
  `args` is always a literal array passed to the OS's process-creation
  call, never joined into a string. Even the Windows npm-shim
  workaround (see `research/WINDOWS.md`) must preserve this — routing
  through `cmd /c` with `args` still as distinct argv elements is
  acceptable; concatenating into one command string is not.
- **stdout AND stderr are both captured**, as a single `onOutput`
  callback tagged `{ stream: "stdout" | "stderr", data }` rather than
  two separate callbacks — the request only mentioned stdout, but
  stderr is where a real CLI's actual failure messages (auth errors,
  rate limits, "command not found") tend to land, and it's free to
  capture once you're already piping. Not scope creep — completes the
  "know what happened" story `code`/`killed` alone can't (a process can
  exit non-zero with the real explanation only on stderr).
- **Chunks are NOT line-buffered.** A chunk is whatever bytes one native
  `read()` call returned — may split or combine lines arbitrarily. The
  caller reassembles lines itself. Deliberate: assuming line-oriented
  output would itself be small AI-CLI-shaped awareness leaking into a
  supposedly generic primitive (mneme's own `stream-json` format happens
  to be line-oriented, but this capability doesn't get to assume that).
- **`exited` is a `Promise` on the handle, not a second callback.**
  Ongoing, repeating output → callback (rule 6, events over polling).
  Process exit is a one-shot terminal value → `Promise` is the more
  idiomatic async/await shape, same reasoning `run()` itself resolving
  once (not repeatedly) already follows.
- **`Child::kill()` needs no per-OS code** — it's already a portable
  std method (SIGKILL on Unix, `TerminateProcess` on Windows). The real
  Windows-specific risk in this capability is entirely on the *spawn*
  side (npm `.cmd`/`.ps1` shims), not kill — see `research/WINDOWS.md`.
- **No interactive stdin/PTY/persistent process** — each `run()` is
  one-shot, matching mneme's actual usage (one process per chat turn,
  resumed via `--resume <session_id>` as a fresh invocation, not a
  long-lived pipe).
- **One-shot stdin payload via `options.stdin`**, added for mneme's
  request 12 (`docs/chain-sdk-requests/12-process-runner-stdin.md`:
  module/course-sized context outgrows argv on every OS). Written once on
  its own thread, concurrently with the stdout/stderr readers (no
  pipe-buffer deadlock), then closed. Write errors (EPIPE from a child
  that exits early) are swallowed, not surfaced. Omitted means
  `Stdio::null()`, unchanged, not an empty pipe. Deliberately a string
  only (no `Uint8Array`) and no `handle.write()`: nothing asked for them.
- **`id` is generated client-side (JS), not native-side.** A fast
  process (spawn → print → exit) can finish within a single IPC round
  trip. If native generated the id and handed it back from
  `process_runner_run`'s own return value, a fast process's output could
  already have fired — and been lost, no listener registered yet —
  before that id ever reached JS. Generating it in
  `packages/sdk/src/process-runner.ts` and registering the listener
  entry for it *before* invoking `process_runner_run` at all closes this
  race completely. Mirrors the *reasoning* behind `agent-server`'s
  listen-before-invoke ordering, but the actual mechanism is different
  since there's no "reply" to wait for here — see next point.
- **One shared, module-level pair of listeners, not one pair per
  `run()` call.** `chain://process-output`/`chain://process-exit` are
  each listened to exactly once (lazily, on first `run()`), dispatching
  to a `Map<id, handler>` — verified this actually matters, not just
  theoretical, when React StrictMode's double-invoke during the
  end-to-end pass ran two full concurrent `run()` calls and both
  resolved correctly with zero cross-contamination between their ids.
- **chain_core::process_runner has zero Tauri dependency**, same split
  `agent_server.rs` already established: real spawn/pipe-reading logic
  and real unit tests live in `crates/core`, only the "reach the
  webview" event-emit plumbing lives in the Tauri layer
  (`packages/cli/templates/lib.rs`'s `process_runner_run`/
  `process_runner_kill`).

## Status

**Implemented and verified end to end on macOS.** Not yet on Windows —
see `research/WINDOWS.md` (rule 3: no single-platform contracts, so
`component.json`/the contract stay Draft/Experimental, not Stable). The
Windows spawn-shim decision (`research/WINDOWS.md`'s three candidates)
was **not** made during this pass — `crates/core/src/process_runner.rs`
uses a plain `std::process::Command::new(command)` with no extension
probing or `cmd /c` fallback yet, which will need real Windows
verification and very likely one of those three approaches before this
capability works there at all against an npm-installed CLI.

- `crates/core/src/process_runner.rs` — `run()`/`ProcessHandle`/
  `ProcessExit`/`ProcessRunnerError`, registered in
  `crates/core/src/lib.rs`. 7 passing unit tests (`cargo test -p
  chain-core`): stdout capture + clean exit code, stderr captured
  separately from stdout, non-zero exit code without erroring, **a real
  timing-based proof that output arrives incrementally and isn't
  buffered to exit** (asserts ≥60ms spread between first/last chunk
  timestamps against a script with real `sleep`s in between), `kill()`
  stopping a long-running process well before its own sleep would exit
  it, empty-command → `InvalidArgument`, nonexistent command →
  `NotFound`. No Tauri dependency.
- `packages/cli/templates/lib.rs` and
  `apps/playground/src-tauri/src/lib.rs` (identical, kept in sync by
  hand) — `ProcessRunnerState`, `process_runner_run`,
  `process_runner_kill`, registered in `generate_handler!`. `cargo
  check` passes for both `chain-core` and `apps/playground/src-tauri`.
- `packages/sdk/src/process-runner.ts` — `run(command, args, onOutput):
  Promise<ProcessHandle>`, exported from `desktop.processRunner` and
  `packages/sdk/src/index.ts`'s type exports. `tsc --noEmit` passes
  clean in both `packages/sdk` and `apps/playground`.
- **Verified end to end for real** in `apps/playground`'s actual running
  window (`npm run tauri dev`, dev port temporarily bumped to 1430 to
  avoid mneme's own already-running dev server, reverted after —
  `git diff --stat apps/playground/` is clean): a temporary probe spawned
  `/bin/sh -c "printf a; sleep 0.3; printf b; sleep 0.3; printf err 1>&2;
  sleep 0.1; exit 3"` and logged every chunk with an elapsed-time stamp.
  Real results: `"a"` at +10ms, `"b"` at +318ms, stderr `"err"` at
  +632ms, exit `{code: 3, killed: false}` at +746ms — timing that
  matches the script's real `sleep` calls almost exactly, which is
  concrete proof output streams incrementally rather than being
  buffered until the process exits. A second probe spawned `sleep 5`,
  called `kill()` at 300ms, and got back `{code: null, killed: true}`
  well before the 5s would have elapsed. One methodology note for future
  probes: Vite's terminal console-forwarding in this dev setup only
  relays `console.warn`/`console.error`, not `console.log` — the first
  attempt at this verification used `console.log` and appeared to
  produce nothing at all until switched to `console.error`; don't
  mistake that for the capability not working.
- **Not separately verified**: `PERMISSION_DENIED` (no easy portable
  fixture for "exists but not executable" was set up during this pass)
  and process-tree/orphan behavior (see CONTRACT.md's Non-goals on
  process groups).

- **`PATH` resolution uses the login shell, not the app's raw inherited
  env, on macOS/Linux.** A GUI app launched by launchd/Finder (a
  packaged build) inherits a minimal system `PATH` that never sources
  `.zshrc`/`.zprofile`, so a bare command like `claude`/`codex`/`gemini`
  installed via nvm/homebrew/asdf resolved under `chain dev` (which
  inherits the terminal's full `PATH`) but failed `NOT_FOUND` once
  packaged — reported by mneme against agent-chat's CLI picker (its
  `detectAgents()` spawns these bare names via
  `desktop.processRunner.run`). Fixed by resolving the real login-shell
  `PATH` once (spawn `$SHELL -lic '...'`, cached via `OnceLock` for the
  process's lifetime) and substituting it into the child's env instead
  of leaving `Command::new` to resolve against the app's own `PATH`. See
  `research/MACOS.md`'s "GUI-launched apps don't get the login-shell
  PATH" for the `${PATH}`-must-be-braced shell-quoting gotcha this hit
  along the way. Deliberately stayed internal to PATH resolution — no
  new caller-facing env/cwd option was added; `CONTRACT.md`'s existing
  "no environment-variable or working-directory options (yet)" non-goal
  still holds.

- **`options.stdin` (request 12), verified on macOS.** 4 more unit
  tests in `process_runner.rs` (11 total): payload echoed back by `cat`
  (non-ASCII and `\0` included) with a clean exit proving EOF, a 4 MB
  payload through `cat` without deadlock, a child that exits without
  reading a 1 MB payload still resolving with its real code, and no
  payload giving an immediate EOF. Also run end to end in
  `apps/playground`'s real window through the SDK and IPC (see
  `research/MACOS.md`). Propagated to mneme with `chain update` (only
  `.chain/native/src/lib.rs` changed); `cargo check` and `tsc --noEmit`
  clean there. Not verified on Windows, where pipe semantics differ
  (`ERROR_NO_DATA`/`ERROR_BROKEN_PIPE` instead of EPIPE); both should
  surface as ignored write errors, but that's unconfirmed.

### File-reference arguments (mneme request 14) — verified on macOS, 28 September 2026

- `args` elements may be `{ fileReference }` (`ProcessArg` in
  `contract.ts`). `templates/lib.rs`'s untagged `ProcessArg` enum is
  resolved through `FilesState` with `chain_core::files::Files::process_path`
  **before** `chain_core::process_runner::run` is called, so the Rust
  spawn code is unchanged and still takes `&[String]`. Unit tests:
  `process_path_resolves_only_existing_managed_files`,
  `extended_length_prefix_only_at_max_path`.
- Verified in a throwaway `chain init` app under `chain dev`: `cat` with
  `[{ fileReference }]` printed the managed file's exact bytes;
  `/bin/sh -c 'echo $#; printf "[%s]" "$@"' sh --image {ref}` printed
  `2` and `[--image][/Users/…/Application Support/dev.chain.reqtest/files/<id>.png]`
  (exactly one argv element, space in the path intact); an unknown
  reference and `../../etc/passwd` both rejected `NOT_FOUND`, and the
  `touch` that would have run with the unknown reference never created
  its file (nothing spawned); plain string args unchanged.
- Windows `\\?\` handling is unit-tested only — see
  `research/WINDOWS.md`.

## What's NOT done yet (next steps for an agent to pick up)

- [ ] Decide between the Windows spawn approaches in
      `research/WINDOWS.md` (extension-probing vs. `cmd /c` vs. a
      dedicated crate) and actually implement one — needs real Windows
      hardware with a real npm-installed CLI to verify against, not just
      a compile check. Currently a bare `Command::new` with no fallback.
- [ ] Verify on Windows specifically against a real npm-installed CLI —
      do NOT mark `component.json`/the contract stable on macOS alone
      (rule 3); this capability's Windows risk is real, not
      boilerplate-only, per `research/WINDOWS.md`.
- [ ] Verify `options.stdin` on Windows: a multi-megabyte payload, and a
      child exiting before reading it all, against a real npm-installed CLI.
- [ ] Add contract tests under `capabilities/process-runner/tests/` —
      currently the only tests are `crates/core/src/process_runner.rs`'s
      Rust unit tests, same gap every other capability here still has.
- [ ] Propagate to mneme via `chain update`, run from *inside* mneme's
      own directory/session — this crosses chain-sdk's project boundary,
      so it needs the user's explicit go-ahead first, same as
      `agent-server`'s propagation.
- [ ] mneme actually building its chat UI (preset flag sets for each
      known CLI, the custom-agent raw-passthrough rendering, usage/stats
      aggregation from the stdout stream, session persistence in
      `desktop.storage`) on top of this — all explicitly mneme's job per
      the request doc, not this capability's.

- [ ] Verify `{ fileReference }` arguments on Windows, including a
      ≥ 260-char path in `\\?\` form against a real CLI.

## Rules specific to this capability

- Never add shell-string construction anywhere in the spawn path — even
  the Windows `cmd /c` workaround (if chosen) must keep `args` as
  distinct argv elements, never one concatenated string.
- Never add a compiled-in executable allowlist, permission prompt, or
  trust/confirmation logic — that's Phase 28's job, a separate
  capability. See `CONTRACT.md`'s "two open design questions."
- Never add stdout/stderr parsing, line-buffering, or any assumption
  about output structure (JSON, line-oriented, etc.) — see "Chunks are
  NOT line-buffered" above. If a real need for line-buffered delivery
  shows up, that's a contract change to discuss, not something to sneak
  in as an implementation convenience.
- Never add interactive stdin (`handle.write()`), PTY, or
  persistent-process support without a real, separate capability request
  driving it (rule 7) — this one is deliberately one-shot-process-only.
  `options.stdin` is write-once-then-close and must stay that way.
