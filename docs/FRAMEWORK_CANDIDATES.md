# Framework Candidates

Tracks which capabilities are generalizable framework modules vs.
application-specific logic that only looks reusable. A capability only
moves from "used by one app" to "framework module" after a second real
application needs the same thing and the contract survives a second
platform implementation. See rule 7 in the root `AGENTS.md`.

## Platform / System

Used by:

- Mneme (real consumer — wired and running)

Generalizable:
Yes — every app needs basic OS/arch/runtime info.

Contract:
Draft (`agent-docs/capabilities/platform/CONTRACT.md`, `capabilities/platform/contract.ts`)

macOS:
Implemented, verified (`apps/playground` and `mneme`)

Windows:
Not started

Possible package:
`@chain/sdk` (bundled in core, not a separate package — too small to split out)

## Storage

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/01-local-storage.md` in the mneme repo)

Generalizable:
Yes — SQLite-backed local storage is a near-universal desktop-app need,
not mneme-specific (the actual schema on top of it is mneme-specific).

Contract:
Draft (`agent-docs/capabilities/storage/CONTRACT.md`, `capabilities/storage/contract.ts`)

macOS:
Implemented, verified end to end in `mneme`'s running window (including
persistence across restarts)

Windows:
Not started — see `agent-docs/capabilities/storage/research/WINDOWS.md` for
specific risks to check before implementing/verifying (rusqlite's
`bundled` feature is itself cross-platform, so the Rust code needs no
changes; only verification is pending)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## Files

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/02-files.md` in the mneme repo)

Generalizable:
Yes — managed local blob storage (images, attachments, exports) is a
near-universal desktop-app need, not mneme-specific (deciding which
attachments to keep and how to render them is mneme's own logic, built
on top of this).

Contract:
Draft (`agent-docs/capabilities/files/CONTRACT.md`, `capabilities/files/contract.ts`)

macOS:
Implemented, verified end to end in `apps/playground`'s running window
(byte-exact write/read round trip, asset-protocol URL, idempotent
delete) and propagated to `mneme` via `chain update`

Windows:
Not started — see `agent-docs/capabilities/files/research/WINDOWS.md` for
specific risks to check before implementing/verifying (`std::fs` is
itself cross-platform, so the Rust code needs no changes; MAX_PATH and
antivirus-locking risks are the same category `storage` already flagged)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## Http

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/04-lms-page-fetch.md` in the mneme repo)

Generalizable:
Yes — a webview `fetch()` to an arbitrary external origin hits ordinary
CORS restrictions in any desktop-app framework, not just mneme's case;
a native-side GET is a near-universal need for "import content from a
URL the user pastes in" (the actual HTML parsing on top is app-specific).

Contract:
Draft (`agent-docs/capabilities/http/CONTRACT.md`, `capabilities/http/contract.ts`)

macOS:
Implemented, verified end to end in `apps/playground`'s running window
against a real external HTTPS URL (200 success path and a real 404
resolving rather than rejecting)

Windows:
Not started — see `agent-docs/capabilities/http/research/WINDOWS.md` for
specific risks to check before implementing/verifying (`reqwest` is
itself cross-platform, so the Rust code needs no changes; whether the OS
certificate store is actually consulted behind a managed-machine
proxy/root-cert is the specific risk to confirm, not a portability gap)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## AgentServer

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/09-agent-tool-server.md` in the mneme repo)

Generalizable:
Yes — a local, loopback-only HTTP listener that forwards requests to a
JS handler is a near-universal need for any desktop app that wants an
external process (an AI agent, a CLI tool, a companion script) to reach
its own logic; native stays deliberately protocol-agnostic, so nothing
about it is MCP- or mneme-specific (mneme's own MCP/JSON-RPC framing and
tool dispatch is app-level, built on top).

Contract:
Draft (`agent-docs/capabilities/agent-server/CONTRACT.md`,
`capabilities/agent-server/contract.ts`)

macOS:
Implemented, verified end to end in `apps/playground`'s running window —
a real external `curl` process reached a live JS handler through the
native listener across several real requests, plus the
already-running/`UNAVAILABLE` rejection path

Windows:
Not started — see `agent-docs/capabilities/agent-server/research/WINDOWS.md`
for the specific risk to check before implementing/verifying (whether
Windows Defender Firewall prompts for a loopback-only bind; the
underlying `tiny_http`/`TcpListener` approach itself needs no Windows-
specific Rust code)

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)

## ProcessRunner

Used by:

- Mneme (real consumer — requested it; see
  `docs/chain-sdk-requests/10-subprocess-runner.md` in the mneme repo,
  plus `12-process-runner-stdin.md` for the one-shot `options.stdin`
  payload)

Generalizable:
Yes — spawning a named executable with an argv array and streaming its
stdout/stderr back incrementally is a near-universal desktop-app need
(any app that wants to run a CLI tool and show its progress live), not
mneme-specific; native has zero awareness of AI CLIs, `stream-json`, or
any other output format (mneme's own stdout parsing/dispatch is
app-level, built on top).

Contract:
Draft (`agent-docs/capabilities/process-runner/CONTRACT.md`,
`capabilities/process-runner/contract.ts`)

macOS:
Implemented, verified end to end in `apps/playground`'s running window —
a real subprocess's output arrival timing matched its own real `sleep`
calls (proof it streams rather than buffers to exit), stderr captured
separately from stdout, a real non-zero exit code round-tripped
correctly, and `kill()` stopped a long-running process well before its
own timeout

Windows:
Not started — see `agent-docs/capabilities/process-runner/research/WINDOWS.md`
for a real, specific (not boilerplate) risk to resolve before
implementing/verifying: npm-global-installed CLIs are frequently
`.cmd`/`.ps1` shims on Windows, and Rust's `Command::new` doesn't do the
PATHEXT-aware extension search `cmd.exe` does, so a bare spawn can fail
against exactly the kind of CLI this capability exists to run; three
candidate fixes are documented there, none chosen yet

Possible package:
`@chain/sdk` (bundled in core, not a separate package — same reasoning
as Platform/System)
