# ProcessRunner — Windows Research

Source: initial survey in
`docs/chain-sdk-requests/10-subprocess-runner.md` (mneme's capability
request), which flagged a **real, specific, non-speculative risk** here
— unlike most prior capabilities, this one is not "portable by
construction." **Not yet verified on real Windows hardware — no
implementation exists yet (see `AGENTS.md`'s Status). Per chain-sdk rule
3 (no single-platform contracts), this capability's contract/
`component.json` stay Draft/not-started until confirmed here.**

## The npm-shim risk the request doc raised

Most people install `claude`/`codex`/`gemini` globally via `npm install
-g`. On Windows, npm's global-install mechanism doesn't produce a single
`.exe` the way it might on Unix (a real executable or a symlink to one)
— it produces a **shim**: typically `<name>.cmd` (a batch-file wrapper
that internally invokes `node` on the actual JS entry point), plus
often a `<name>.ps1` (PowerShell wrapper) and a extensionless `<name>`
file (for Git Bash/WSL-style shells), all three sitting in the same npm
global bin directory, which is on `PATH`.

**The specific Rust gotcha this creates:** `std::process::Command::new`
on Windows calls `CreateProcessW` directly. Unlike `cmd.exe`'s own
built-in command resolution (which consults the `PATHEXT` environment
variable — typically `.COM;.EXE;.BAT;.CMD;...` — and tries each
extension in turn when you type a bare command name at a prompt),
`CreateProcessW` does **not** do PATHEXT-aware extension search on its
own. Handing `Command::new("claude")` straight to `CreateProcessW` when
the actual file on disk is `claude.cmd` is a well-documented real-world
failure mode for exactly this reason — it can fail to launch at all
(the OS has no bare `claude` file to find) rather than falling back to
the `.cmd` shim the way a human typing `claude` at a `cmd.exe` prompt
would get it to work.

## Candidate approaches — not yet decided, left for whoever implements

Deliberately not resolved here, since it's an implementation detail that
doesn't change the public contract shape (`run(command, args,
onOutput)` stays identical either way):

1. **Explicit extension probing**: on Windows, if a bare `Command::new(command)`
   fails to spawn, retry with `command` plus each of `.cmd`, `.bat`,
   `.exe` in turn (mirroring what `PATHEXT` would try), before giving up
   with `NOT_FOUND`. Keeps every spawn as a direct `CreateProcessW` call
   — no shell involved at any point, cleanest fit with the "no shell
   interpretation, ever" invariant.
2. **Route through `cmd.exe /c`**: spawn `cmd.exe` with `["/c", command,
   ...args]` as the argv array. `cmd.exe`'s own PATHEXT-aware resolution
   then finds the `.cmd` shim the same way an interactive prompt would.
   **Worth being careful about wording here**: this still passes `args`
   as distinct argv elements to `Command::new("cmd").args([...])` — it
   is not building one shell *string* by concatenation, so it doesn't
   reintroduce the specific injection class "no shell interpretation"
   exists to prevent. But `cmd.exe` itself does its own internal
   re-parsing/re-quoting of the arguments it receives after `/c`
   (documented, sometimes-surprising `cmd.exe` quoting rules,
   particularly around `&`, `|`, `^`, `%`, and embedded quotes) — so this
   approach is *meaningfully* safer than raw string concatenation, but
   not identically zero-risk to a direct `CreateProcessW` call with no
   intermediary. If this approach is chosen, that distinction needs to
   be documented plainly in `AGENTS.md`, not glossed over.
3. **Use a crate that already handles this** (e.g. one of the existing
   Rust ecosystem crates built specifically around the Windows
   `PATHEXT`/shim problem) instead of hand-rolling either of the above —
   worth checking what's actively maintained at implementation time
   rather than assuming crate 1 or 2 above is the only option.

## What needs real verification (not assumable from documentation alone)

- Whether approach 1 (extension probing) actually resolves a real
  npm-global-installed `claude.cmd` shim end to end (spawns, streams
  stdout, reports the right exit code) — the request doc is explicit
  this needs testing against a *real* Windows npm install of a real CLI,
  not just a compile-time check.
- Whether the chosen approach preserves argv-element boundaries
  correctly for an argument containing spaces/quotes (a real chat
  message will contain both) — this is exactly the kind of thing
  `cmd.exe`'s re-quoting behavior (approach 2) could silently mangle.
- Whether `Child::kill()` actually terminates a `cmd.exe`-wrapped shim
  process's real underlying `node` process, or only the immediate
  `cmd.exe`/`.cmd` layer, leaving the actual CLI process orphaned and
  still running/still writing to the (now-closed) pipe — a real risk
  specifically *because* of approach 2's extra process layer, not
  present in approach 1.

## Checklist for whoever verifies this on Windows

- [ ] `cargo build`/`cargo check`/`cargo test -p chain-core` succeed
      once `process_runner.rs` exists.
- [ ] Install a real CLI globally via `npm install -g` on a real Windows
      machine (doesn't need to be `claude` itself — any npm-global CLI
      that produces a `.cmd` shim reproduces the risk) and confirm
      `desktop.processRunner.run()` can actually spawn it, stream its
      stdout, and report the correct exit code.
- [ ] Confirm an argument containing spaces and a literal `"` character
      survives the round trip unmangled.
- [ ] Confirm `kill()` actually stops the real underlying process, not
      just an intermediary shim layer, on whichever approach was chosen.
- [ ] Update `docs/CAPABILITY_MATRIX.md`'s Windows column and this
      capability's `component.json` once confirmed.
