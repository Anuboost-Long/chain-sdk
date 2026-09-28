# Capability Matrix

Status of every Chain capability, per platform. Update this whenever a
capability's status changes — this is the fastest way for a human or agent
to see what actually exists vs. what's just planned.

| Capability      | macOS | Windows | Linux | Contract |
| --------------- | ----- | ------- | ----- | -------- |
| Platform/System | 🧪    | ⏳      | ⏳    | Draft    |
| Storage         | 🧪    | ⏳      | ⏳    | Draft    |
| Files           | 🧪    | ⏳      | ⏳    | Draft    |
| Http            | 🧪    | ⏳      | ⏳    | Draft    |
| AgentServer     | 🧪    | ⏳      | ⏳    | Draft    |
| ProcessRunner   | 🧪    | ⏳      | ⏳    | Draft    |

Legend:

```
✅ Tested
🧪 Experimental
⏳ Not implemented
⚠ Partial
❌ Unsupported
```

`platform` has a native implementation wired end to end on macOS,
verified running in both `apps/playground` and `mneme`'s own window —
still marked Experimental, not Tested, until contract tests exist and
it's verified on Windows too (`agent-docs/capabilities/platform/AGENTS.md`).

`storage` (SQLite-backed `migrate`/`query`/`execute`) is implemented and
verified end to end on macOS — real inserts/queries through the full
React → SDK → Tauri → Rust → SQLite path in `mneme`'s running window,
including persistence across app restarts — but not yet verified on
Windows (`agent-docs/capabilities/storage/research/WINDOWS.md` has
specific risks to check first: WAL over network drives, antivirus file
locking).

`files` (managed local blob storage: `write`/`read`/`url`/`delete` by an
opaque reference) is implemented and verified end to end on macOS — a
real write/read/url/delete round trip through the full React → SDK →
Tauri → Rust → `std::fs` path in `apps/playground`'s running window,
confirmed byte-exact — and propagated to `mneme` via `chain update`, but
not yet verified on Windows
(`agent-docs/capabilities/files/research/WINDOWS.md` has the specific
risks to check first: MAX_PATH limits, antivirus file locking).

`http` (a single native-side HTTP GET, returning `status`/`ok`/`body` as
text) is implemented and verified end to end on macOS — a real request
through the full React → SDK → Tauri → Rust → `reqwest` path in
`apps/playground`'s running window, against a real external HTTPS URL
(`https://example.com`), confirming both the success path (200, real
HTML body) and that a non-2xx response resolves rather than rejects
(a real 404) — but not yet verified on Windows
(`agent-docs/capabilities/http/research/WINDOWS.md` has the specific
risk to check first: whether the OS certificate store is actually
consulted the way `native-tls`/SChannel is documented to on a real
managed-machine proxy/root-cert setup).

`agent-server` (a local `127.0.0.1`-only HTTP listener forwarding
requests to a JS handler, so an external AI agent process can call into
the app) is implemented and verified end to end on macOS — a real
external `curl` client hit a running server in `apps/playground`'s
window across several sequential requests (GET/POST, varying paths and
bodies), each correctly round-tripped through the native listener to a
live JS handler and back; the "only one server per app run" rejection
(`UNAVAILABLE`) was also exercised for real via React's dev-mode
double-effect-invoke naturally calling `start()` twice — but not yet
verified on Windows (`agent-docs/capabilities/agent-server/research/WINDOWS.md`
has the specific risk to check first: whether Windows Defender Firewall
prompts for a loopback-only listener). Native stays deliberately
protocol-agnostic — no MCP/JSON-RPC awareness — see that capability's
`CONTRACT.md` for why.

`process-runner` (spawn an executable by argv array and stream its
stdout/stderr back incrementally, with exit-code reporting and early
`kill()`) is implemented and verified end to end on macOS — a real
subprocess spawned in `apps/playground`'s window produced output whose
arrival timing matched its own real `sleep` calls almost exactly (proof
it streams rather than buffers to exit), stderr was captured separately
from stdout, a real non-zero exit code round-tripped correctly, and
`kill()` stopped a long-running process well before its own timeout —
but not yet verified on Windows, where the risk is real and specific
(`agent-docs/capabilities/process-runner/research/WINDOWS.md`: npm-
global-installed CLIs are often `.cmd`/`.ps1` shims, and `Command::new`
doesn't do PATHEXT-aware extension search the way `cmd.exe` does — three
candidate fixes documented, none implemented yet). Native stays
deliberately generic — no AI-CLI/output-format awareness, and no
compiled-in executable allowlist (that's Phase 28's job) — see that
capability's `CONTRACT.md` for why.

`files.pick()` (mneme request 16 — the OS open panel attached to the
app window as a sheet, returning names and bytes, never paths),
`files.save()` (request 17 — the save panel as a sheet on every call,
native writes the bytes, returns only the file name), and
`processRunner.run()`'s `{ fileReference }` arguments (request 14 — a
managed file's path substituted natively as one argv element) are both
verified end to end on macOS in a real `chain dev` app; Windows is
unverified for all three (see each capability's `research/WINDOWS.md`).

See `docs/FRAMEWORK_CANDIDATES.md` for what's next.
