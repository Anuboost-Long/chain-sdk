# AgentServer Capability — Agent Memory

Scope: `desktop.agentServer.start(handler, options?)` /
`desktop.agentServer.stop()` — a local, `127.0.0.1`-only HTTP listener
that forwards each request to a JS handler and replies with what it
returns. Requested by mneme (see
`docs/chain-sdk-requests/09-agent-tool-server.md` in the mneme repo) as
its ninth real capability gap: Phase 25 ("AI Agent Tools") needs external
AI agents (Claude Code, Claude Desktop, any MCP client) to call into
mneme's existing `read_page`/`create_page`/etc. functions, which today
only get invoked by React event handlers inside the webview.

Read order for a task in this capability:

1. Root `/AGENTS.md`
2. `/docs/ARCHITECTURE.md`
3. This file
4. `CONTRACT.md` + `contract.ts`
5. `research/MACOS.md` / `research/WINDOWS.md`
6. Relevant source in `crates/core/src/agent_server.rs` /
   `packages/sdk/src/agent-server.ts` / `packages/cli/templates/lib.rs`

## What's already decided

- **Native is protocol-agnostic — no MCP/JSON-RPC awareness in Rust.**
  This is the single most important decision in `CONTRACT.md`, under
  "The design fork" — the request doc explicitly asked us to choose
  between chain-sdk speaking MCP's wire protocol directly vs. a simpler
  protocol the app translates. Chose the latter: native only forwards
  raw HTTP (method/path/headers/body) to a JS handler; mneme's own JS
  handler owns all JSON-RPC/MCP framing and `tools/call` dispatch to its
  existing app-level functions. Don't revisit this incrementally (e.g.
  "just add `initialize` handling natively to save mneme a little code")
  without re-reading that section's full rationale first.
- **Prior art confirmed by direct read, not just the request doc's
  claim**: `packages/cli/templates/dev_inspector.rs`'s `run_eval` (mirrored
  in `apps/playground/src-tauri/src/dev_inspector.rs`) really does
  implement `TcpListener::bind("127.0.0.1:0")` → per-connection forward
  via `window.eval()` → block on `mpsc::channel` until a
  `__chain_inspector_report` callback arrives, with a `recv_timeout`
  fallback. This capability generalizes that exact shape for release
  builds (`dev_inspector.rs` itself stays gated behind the
  `chain-dev-inspector` feature and is never compiled into
  `chain build` output — this is a *new*, separate module, not a
  reuse/un-gating of that dev-only file).
- **Public API takes the handler as a parameter to `start()`**, not a
  separate `onRequest()` registration step — reads as a single plain
  async callback to app code (rule 2: shape it for what it means to an
  app developer). The event-emit/listen/callback-command plumbing
  `run_eval` needs to get a JS function's result back across the Tauri
  IPC boundary is an SDK-wrapper implementation detail
  (`packages/sdk/src/agent-server.ts`, not yet written), not part of the
  public contract shape.
- **One server per app run**, `start()` while already running rejects
  `UNAVAILABLE` rather than queuing/replacing — simplest option from the
  three the request doc left open, and no real need for more than one
  surfaced yet.
- **Port: ephemeral by default (`0`), explicit `options.port` allowed.**
  Matches `run_eval`'s own default; the explicit-port option exists for
  mneme's later "show the port in Settings" idea without forcing it now.
- **Serial request handling — one in-flight request at a time**, same as
  `run_eval`'s single-connection-at-a-time accept loop. Documented as a
  non-goal (not a missing feature) in `CONTRACT.md`; revisit only if a
  real concurrency need shows up.
- **No auth beyond the `127.0.0.1` bind.** Phase 28 ("Agent Permission
  System") is the explicitly separate, later capability that adds
  approval/trust — this one only answers "can a local process reach the
  handler at all."
- **`tiny_http` chosen** for the Rust server (see `research/MACOS.md`'s
  two candidates) — synchronous, matches the serial/one-connection
  accept-loop model exactly, no tokio dependency added to `crates/core`'s
  production path.
- **`crates/core::agent_server` owns the 500/504 mapping, not the Tauri
  layer.** `start()`'s `dispatch` closure returns a `DispatchOutcome`
  (`Response`/`HandlerFailed`/`Timeout`) rather than a plain
  `AgentServerResponse` — the "handler failure → 500" / "handler timeout
  → 504" mapping happens inside chain_core, where it's covered by real
  unit tests (`cargo test -p chain-core`), instead of being untested
  Tauri glue. Only "forward to the webview and wait, with a 30s timeout"
  lives in `packages/cli/templates/lib.rs`'s `agent_server_start`, since
  that part genuinely needs an `AppHandle`.
- **SDK wrapper registers the webview listener *before* invoking the
  native `agent_server_start` command**, and only promotes it to the
  module-level `activeUnlisten` *after* that invoke succeeds — see
  `packages/sdk/src/agent-server.ts`'s comments. Getting this ordering
  wrong either race-loses the very first request (listener attached
  after native already started accepting) or, worse, silently orphans a
  genuinely running server's listener when a *second*, rejected
  `start()` call tears down the first one's handler as a side effect.
  Don't "simplify" this ordering without re-reading those comments.

## Status

**Implemented and verified end to end on macOS.** Not yet on Windows —
see `research/WINDOWS.md` (rule 3: no single-platform contracts, so
`component.json`/the contract stay Draft/Experimental, not Stable).

- `crates/core/src/agent_server.rs` — `start()`/`ServerHandle`/
  `DispatchOutcome`/`AgentServerError`, registered in
  `crates/core/src/lib.rs`. 5 passing unit tests (`cargo test -p
  chain-core`): a real loopback HTTP round trip through the dispatcher,
  handler-failure → 500, handler-timeout → 504, port-already-in-use →
  `Unavailable`, out-of-range port → `InvalidPort`. No Tauri dependency —
  fully testable without the app layer.
- `packages/cli/templates/lib.rs` and
  `apps/playground/src-tauri/src/lib.rs` (identical, kept in sync by
  hand) — `AgentServerState`, `agent_server_start`, `agent_server_stop`,
  `__chain_agent_server_respond`, all registered in `generate_handler!`.
  `cargo check` passes for both `chain-core` and
  `apps/playground/src-tauri`.
- `packages/sdk/src/agent-server.ts` — the `start(handler, options?)`/
  `stop()` wrapper, exported from `desktop.agentServer` and from
  `packages/sdk/src/index.ts`'s type exports. `tsc --noEmit` passes
  clean in both `packages/sdk` and `apps/playground`.
- **Verified end to end for real** in `apps/playground`'s actual running
  window (`npm run tauri dev`, dev port temporarily bumped to 1430 to
  avoid colliding with mneme's own already-running dev server —
  reverted after, `git diff --stat apps/playground/` is clean): a
  temporary probe (`desktop.agentServer.start(handler, { port: 47811
  })` in `App.tsx`, reverted after) started a real server, and a genuine
  external `curl` process — outside the webview entirely — sent three
  separate real HTTP requests (`POST /mcp`, `GET /ping`, `POST
  /tools/call`, each with a distinct body) and got back the live JS
  handler's actual response each time, method/path/body round-tripping
  correctly. React's dev-mode double-effect-invoke called `start()`
  twice for free, and the second call correctly rejected
  `{"code":"UNAVAILABLE","message":"agent server is already running"}`
  while the first server kept working — confirming the
  already-running rejection path against real (if accidental) reuse,
  not just a unit test. One real gotcha hit and resolved during this
  pass: Vite's dependency pre-bundling discovered the new
  `@tauri-apps/api/event` import mid-session and force-reloaded the
  page, which orphaned the first successful server (native still had it
  bound, but the JS listener died with the reload) — a dev-server-only
  artifact, not a capability bug; a clean restart (with the dependency
  already pre-bundled) didn't repeat it.
- **Not separately verified**: `stop()`'s SDK-level round trip (only
  exercised indirectly, and `ServerHandle::stop()`/`Drop` are unit-tested
  in isolation in `crates/core`) — no UI hook triggered it during the
  probe pass.

## What's NOT done yet (next steps for an agent to pick up)

- [ ] Verify on Windows — do NOT mark `component.json`/the contract
      stable on one platform only (rule 3). See `research/WINDOWS.md`
      for the specific risk to check first (Windows Defender Firewall
      prompting for a loopback-only listener).
- [ ] Add contract tests under `capabilities/agent-server/tests/` —
      currently the only tests are `crates/core/src/agent_server.rs`'s
      Rust unit tests, the same gap every other capability here still
      has.
- [ ] Verify `stop()`'s SDK-level round trip specifically (a real
      `start()` → `stop()` → confirm the port is actually released and a
      subsequent request is refused) — not covered by the probe pass
      above.
- [x] Propagated to mneme via `chain update` (with the user's explicit
      go-ahead to cross chain-sdk's project boundary). One file updated
      (`.chain/native/src/lib.rs`, merged cleanly, no conflicts — the
      three "skip" lines are files mneme itself already deleted from its
      scaffold, unrelated to this capability). `.chain/` is entirely
      gitignored in mneme by design, so no git diff there is expected.
      Verified: `cargo check` passes in `mneme/.chain/native` (pulls in
      `tiny_http`/`chain-core` through the path dependency), `tsc
      --noEmit` passes in mneme's root (picks up `desktop.agentServer`
      through the live `@chain/sdk` symlink, no `npm install` needed), a
      second `chain update` reports "Already up to date."
- [ ] mneme actually building its MCP Streamable HTTP transport + tool
      dispatch on top of this now that it has the capability — that's
      mneme's job, not this capability's.

## Rules specific to this capability

- Never add MCP/JSON-RPC parsing, method routing, or tool-schema
  knowledge to the native (Rust) side — see "What's already decided"
  above and `CONTRACT.md`'s "design fork" section. If that decision ever
  needs revisiting, it's a deliberate contract change with its own
  rationale, not an incremental addition.
- Never add auth/token/permission logic beyond the `127.0.0.1` bind
  itself — that's Phase 28's job, a separate capability.
- Never let `options.port` (or any future option) accept a host/interface
  — binding anywhere but loopback is a hard invariant, not a
  configurable default.
