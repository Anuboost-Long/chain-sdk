# AgentServer Capability — Contract

## What this is

A local server, bound to `127.0.0.1` only, that lets an **external
process** — an AI agent such as Claude Code, Claude Desktop, or any other
MCP-compatible client, running outside the app's webview entirely — call
into app logic that only exists inside that webview. A browser/webview
has no API to open a listening socket; this is squarely native/OS
territory, so it has to be a capability, not something worked around in
app code.

Backs mneme's Phase 25 ("AI Agent Tools" — `read_page`, `create_page`,
`search_workspace`, ...): those tool implementations are already plain
app-level functions in mneme's own `src/features/courses/lib/*.ts`. This
capability's only job is the round trip an external process needs to
reach them at all — it knows nothing about pages, courses, or tools.

The round-trip shape — native owns the socket, forwards each request to
a JS-registered handler, blocks until the handler replies, then sends
that reply back over the wire — is not new in this app shell:
`chain-dev-inspector`'s `run_eval` (`dev_inspector.rs`, gated behind the
`chain-dev-inspector` Cargo feature, dev-only) already does exactly this
for a different purpose, using a `TcpListener` + `window.eval()` +
`mpsc` channel unblocked by a `__chain_inspector_report` callback. This
capability generalizes that same proven shape for release builds, for
any HTTP request instead of only raw `eval`.

## The design fork: raw HTTP, not a native MCP implementation

The request that drove this capability explicitly flagged a fork:
should chain-sdk speak MCP's actual wire protocol (JSON-RPC 2.0 framing,
`initialize` handshake, `tools/list`, `tools/call`, session semantics)
natively in Rust, so any MCP client works against it with zero app-side
glue? Or should the native layer stay protocol-agnostic and let the app
translate?

**Decision: protocol-agnostic.** Native Rust only understands "HTTP
request in, HTTP response out" — method, path, headers, a raw body
string. It has no knowledge of JSON-RPC, MCP method names, or tool
schemas. mneme's own JS-side handler is responsible for parsing the
MCP Streamable HTTP transport's JSON-RPC envelopes and dispatching
`tools/call` to its own `read_page`/`create_page`/etc. functions — the
same place that dispatch logic already has to live regardless.

Why, given the "zero glue" appeal of the alternative:

- **Every other capability in this SDK draws this same line** — `http`
  does a generic GET and knows nothing about HTML/LMS parsing;
  `files` stores bytes and knows nothing about MIME meaning. MCP's
  JSON-RPC semantics are app-level by that identical precedent — they're
  "what does a request mean," not "how do bytes reach the webview."
- **Rule 2** ("no platform-specific naming leaks... native method names
  never dictate the public SDK shape") applies equally to a third-party
  wire protocol: baking MCP's method names and session model into
  `crates/core` would make this capability *only* useful for MCP, and
  would tie chain-sdk to tracking an external, still-evolving spec at
  the Rust layer instead of at the app layer where it can iterate freely.
- **Rule 7** ("minimum framework first") — a raw HTTP envelope is the
  strictly more general primitive. It serves MCP's Streamable HTTP
  transport today and any future local-request need (a webhook receiver,
  a different agent protocol) without a second capability, whereas an
  MCP-shaped native server would serve nothing else.
- The JSON-RPC/MCP framing logic mneme must write is comparable in size
  to the tool-dispatch routing it needs regardless — pushing it app-side
  doesn't meaningfully add work, it just keeps a fast-moving external
  spec out of Rust.

## `desktop.agentServer.start(handler, options?)`

```
start(
  handler: (request: AgentServerRequest) => Promise<AgentServerResponse>,
  options?: { port?: number }
): Promise<{ port: number }>
```

Binds a `127.0.0.1`-only TCP listener and resolves once it's actually
listening, with the bound port. `options.port` requests a specific port
(e.g. so an app can show a stable port in Settings); omitted or `0`
binds an OS-assigned ephemeral port instead, the same default
`chain-dev-inspector`'s own listener already uses. Binding to any
interface other than the loopback address is never possible through this
API — there is no `host`/interface option, deliberately (see Non-goals).

Every accepted HTTP request is forwarded to `handler` as an
`AgentServerRequest` (`method`, `path`, `headers`, and `body` as the raw
request text — this capability does not parse the body; JSON-RPC framing
is the caller's concern). Native blocks that one connection until
`handler`'s returned promise resolves, then writes the
`AgentServerResponse` (`status`, optional `headers`, `body` as text) back
as the HTTP response. This is the same "native listens, JS handles,
native replies" round trip `run_eval` already proved works in this exact
app shell.

Only one server is active per app run. Calling `start()` again while
already running rejects (see Errors) rather than starting a second
listener or silently replacing the handler — call `stop()` first.

**Starting the server is entirely the calling app's decision** — this
capability never auto-starts on app launch. mneme deciding *when* to
expose it (e.g. only behind a "Developer / Agent Tools" setting the user
turns on) is app-level policy, not something this capability enforces.

## `desktop.agentServer.stop()`

```
stop(): Promise<void>
```

Stops accepting new connections and releases the port. **Idempotent** —
calling `stop()` when no server is running resolves successfully rather
than rejecting, the same reasoning `files.delete()` already uses for a
reference that's already gone.

## Errors

- `start()` rejects with `ChainError { code: "UNAVAILABLE" }` if a
  server is already running (call `stop()` first), or if the requested
  port (explicit or ephemeral) can't be bound because it's already in
  use.
- `start()` rejects with `ChainError { code: "PERMISSION_DENIED" }` if
  the OS refuses the bind for permission reasons (e.g. an explicit
  privileged port the process isn't allowed to claim).
- `start()` rejects with `ChainError { code: "INVALID_ARGUMENT" }` if
  `options.port` is set but outside the valid TCP port range.
- Any other unexpected native failure during bind/serve rejects with
  `ChainError { code: "NATIVE_FAILURE" }`, the underlying OS/IO error in
  `message`.
- If `handler` itself throws or its returned promise rejects, that is
  **not** a rejection of `start()`'s promise (the server keeps running)
  — the external HTTP caller for that one request instead receives a
  native-generated `500` response. If `handler` never resolves within a
  bounded timeout, that one external caller instead receives a
  native-generated `504` response, mirroring `run_eval`'s own
  `recv_timeout` behavior — one slow/broken request never wedges the
  whole server for other callers waiting on the same connection queue
  (see Non-goals on concurrency for what "other callers waiting" means
  here).
- Calling `start()`/`stop()` outside a Chain (Tauri) runtime rejects with
  `ChainError { code: "UNSUPPORTED" }`, same as every other capability.

## Non-goals

- **No MCP (or any other) protocol awareness in native code.** See "The
  design fork" above — this is the central non-goal and the one most
  likely to attract scope creep; do not add JSON-RPC parsing, method
  routing, or tool-schema knowledge to `crates/core` without revisiting
  that decision explicitly, not just adding it incrementally.
- **No auth or permission model beyond "bound to `127.0.0.1` only."**
  Any local process can call a running server — there is no token,
  allowlist, or per-tool confirmation here. That safety layer is Phase
  28 ("Agent Permission System" — approval dialogs, trusted permissions,
  confirming destructive actions), a separate, later capability request;
  this one is scoped to "can a local process reach the app's handler at
  all."
- **No non-loopback binding, ever.** `options.port` selects a port, never
  a host/interface — this capability cannot be made reachable from the
  network even by a caller mistake.
- **No TLS/HTTPS.** Loopback traffic to the app's own process doesn't
  need encrypting against itself.
- **No CORS handling.** Callers are local processes speaking HTTP
  directly (MCP clients), not a browser `fetch()` — there's no origin
  model to negotiate.
- **No concurrency guarantees beyond one in-flight request at a time.**
  One accepted connection is forwarded to `handler` and blocks until it
  replies (or times out) before the next connection is served — the same
  serial model `run_eval`'s accept loop already uses. Fine for
  interactive, human-paced agent tool calls; revisit only if a real
  throughput need shows up (rule 7 — don't build for hypothetical
  scale).
- **No request body streaming or size limit.** The whole body is
  buffered into a string before `handler` is invoked, the same "no
  streaming" non-goal `http.get()` and `files` already carry.
- **No request logging, auditing, or persistence.** This capability
  doesn't record what came through the server; an audit trail (if
  needed) is app-level, and ties naturally into Phase 28's permission
  work rather than this one.
- **No multiple simultaneous servers / no port range management.** One
  server per app run, one port; an app needing more than that is a new
  requirement to discuss, not something to build speculatively now.
