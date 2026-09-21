# AgentServer — macOS Research

Source: initial survey in
`docs/chain-sdk-requests/09-agent-tool-server.md` (mneme's capability
request), which already concluded this is portable like `http`/`storage`
— no per-OS branching expected. Expanded here at contract time; nothing
below has been exercised on real hardware yet since this capability has
no implementation (see `AGENTS.md`'s Status).

## Approach

Listening on `127.0.0.1` is a plain BSD-sockets operation on macOS, not a
native surface the way Clipboard/Audio need real Swift/AVFoundation code
— same category `http`'s outbound `reqwest` client and `storage`'s
`rusqlite` already fall into. No macOS-specific Rust code is expected in
`crates/core/src/agent_server.rs`.

## Server crate — not yet decided, listed here for whoever implements

`crates/core`'s existing dependencies already include `tokio` (`rt`,
`macros` features, added for `http.rs`'s async `reqwest::Client`). Two
reasonable directions, deliberately left open since neither affects the
public contract shape in `CONTRACT.md`:

- **`tiny_http`** — a minimal, synchronous, blocking HTTP server with
  essentially no dependency footprint. Closest in spirit to
  `run_eval`'s own hand-rolled `TcpListener` + `BufReader`-line-protocol
  approach (`dev_inspector.rs`), just with real HTTP parsing instead of
  a custom newline-JSON wire format. Fits this capability's serial,
  one-request-at-a-time contract naturally (a plain loop, no async
  runtime needed for the server itself).
- **`hyper`/`axum`** — async, built on the `tokio` runtime already in the
  dependency tree for `http.rs`. More natural if this capability ever
  needs to run concurrently with other async work on the same runtime,
  but pulls in more surface than the contract's serial model actually
  needs right now.

Leaning `tiny_http` for the simpler mental model that matches the
contract's serial-request non-goal exactly, but this is not a final
decision — whoever implements should re-check both crates' current
maintenance status before picking.

## App Sandbox / entitlements

Same caveat `http`'s own research already carries: a *listening* socket
under the macOS App Sandbox (Mac App Store distribution) needs
`com.apple.security.network.server` (distinct from `http.rs`'s outbound
`...network.client`) — not relevant to local `chain dev`/unsigned-build
testing, only to a future sandboxed-distribution build.

## Local Network privacy prompt — needs empirical confirmation

macOS has, in recent versions, added a "Local Network" privacy
permission prompt for apps that communicate on the local network (mDNS,
broadcast, or connections to other devices on the same subnet). Binding
and accepting connections strictly on the loopback interface
(`127.0.0.1`) is *not* the same thing that prompt is designed to gate —
loopback traffic never leaves the machine — but this has not been
empirically confirmed against a real recent macOS version for a plain
`TcpListener::bind("127.0.0.1:...")`. Flagging as a specific thing to
watch for during implementation, not assumed either way.

## Not yet verified (no implementation exists yet)

- No unit tests exist yet (`crates/core/src/agent_server.rs` isn't
  written) — see `AGENTS.md`'s "What's NOT done yet".
- Whether the Local Network privacy prompt above actually fires for a
  loopback-only bind on a current macOS version.
- Real end-to-end verification (a real local HTTP client calling a
  running server, exercising `UNAVAILABLE`-on-double-start, the timeout
  path, and `stop()`'s idempotency) — none of this has happened yet.
