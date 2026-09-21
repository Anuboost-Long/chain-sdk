# AgentServer Capability (`desktop.agentServer`)

## How it works

A local, `127.0.0.1`-only HTTP listener that lets an **external
process** — an AI agent such as Claude Code or Claude Desktop, running
outside the app entirely — call into app logic running inside the
webview. A browser/webview has no API to open a listening socket, so
this is native/OS territory.

`crates/core/src/agent_server.rs` owns the actual socket and HTTP
parsing, via `tiny_http` (synchronous, no async-runtime dependency,
unlike `http.rs`'s `reqwest` client). It knows nothing about Tauri or a
webview — its public `start(port, dispatch)` takes a plain `dispatch`
closure and calls it once per accepted request, serially (one in-flight
request at a time), mapping whatever the closure reports (`Response`,
`HandlerFailed`, or `Timeout`) onto a real HTTP response (200-ish,
`500`, or `504` respectively).

`packages/cli/templates/lib.rs`'s `agent_server_start` command is what
actually plugs a webview into that `dispatch` closure — the same
"native listens, JS handles, native replies" round trip
`chain-dev-inspector`'s `run_eval` (`dev_inspector.rs`) already proves
works in this app shell, generalized beyond raw `eval` and shipped
unconditionally (not gated behind a Cargo feature, since this is a real
capability, not a dev-only bridge). Each request is forwarded to the
webview as a `chain://agent-server-request` event; the Tauri layer
blocks on an `mpsc` channel (30s timeout) until the JS side calls the
`__chain_agent_server_respond` command back with the result.

**Crucially, native code has zero awareness of MCP, JSON-RPC, or tool
schemas** — it only understands "HTTP request in, HTTP response out"
(`method`/`path`/`headers`/`body`, all as plain strings). An app
building an MCP server on top of this (mneme's actual use case) owns
all of that framing and dispatch itself, in its own JS handler. See
`CONTRACT.md`'s "design fork" section for the full reasoning — this was
a deliberate choice, not the only option, so don't add MCP-awareness to
Rust without re-reading that first.

## How to use it

```ts
import { desktop } from "@chain/sdk";

const { port } = await desktop.agentServer.start(async (request) => {
  // request: { method, path, headers, body }
  // ... parse/dispatch it however your app needs (e.g. MCP JSON-RPC
  // framing + routing to your own app-level functions)
  return { status: 200, body: JSON.stringify({ ok: true }) };
});

console.log(`listening on 127.0.0.1:${port}`);

// later
await desktop.agentServer.stop();
```

`start()` rejects with `ChainError { code: "UNAVAILABLE" }` if a server
is already running — only one server (and one registered handler) is
allowed per app run; call `stop()` first. `stop()` is idempotent.
Starting the server is entirely the app's decision — this capability
never auto-starts, so gating it behind a user-facing setting (e.g. "only
when Developer/Agent Tools is turned on") is normal, expected usage, not
a workaround.

Every already-scaffolded app gets this automatically via `chain update`
(it's a tracked file in `packages/cli/templates/lib.rs`) — no manual
wiring needed per app.

## Files to check

- `agent-docs/capabilities/agent-server/CONTRACT.md` — the semantic
  contract, especially "The design fork" (why native stays
  protocol-agnostic) and the Non-goals (no auth beyond the `127.0.0.1`
  bind, no concurrency beyond one in-flight request, no MCP awareness).
  Check this before changing behavior.
- `capabilities/agent-server/contract.ts` — the exact types
  (`AgentServerApi`, `AgentServerRequest`, `AgentServerResponse`,
  `AgentServerHandler`, `AgentServerInfo`); change this and every
  implementation below together, never one without the others.
- `agent-docs/capabilities/agent-server/AGENTS.md` — what's already
  decided and why, and the actual TODO checklist (Windows verification,
  contract tests, mneme's own MCP-handler build-out).
- `agent-docs/capabilities/agent-server/research/MACOS.md` /
  `research/WINDOWS.md` — platform-specific findings, including two
  **not yet confirmed either way** risks: macOS's Local Network privacy
  prompt and Windows Defender Firewall's first-listen prompt, both
  specifically for a loopback-only bind.
- `crates/core/src/agent_server.rs` — the actual Rust implementation
  (`start()`, `ServerHandle`, `DispatchOutcome`, `AgentServerError`).
  Has real unit tests (`cargo test -p chain-core`) exercising a real
  loopback HTTP round trip, the 500/504 mapping, and port-in-use/
  out-of-range rejection — no Tauri dependency, extend tests here rather
  than only through the Tauri layer.
- `packages/sdk/src/agent-server.ts` — the SDK wrapper. The subtle part:
  it registers the webview event listener *before* invoking the native
  `agent_server_start` command (closes the race where a request could
  arrive before anything is listening), and only replaces the
  module-level `activeUnlisten` *after* the native call actually
  succeeds (so a rejected double-`start()` never orphans a genuinely
  running server's listener).
- `packages/cli/templates/lib.rs` — the Tauri command layer
  (`agent_server_start`/`agent_server_stop`/
  `__chain_agent_server_respond`, `AgentServerState`) that every
  scaffolded app gets. This is the file `chain update` propagates — see
  `agent-docs/framework/command/README.md`.
- `apps/playground/src-tauri/src/lib.rs` — same wiring, kept in sync by
  hand (playground isn't `chain init`-managed) so the framework's own
  proof app demonstrates every capability, not just the earlier ones.
