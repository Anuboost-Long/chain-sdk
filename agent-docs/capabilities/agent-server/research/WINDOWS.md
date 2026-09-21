# AgentServer — Windows Research

Source: initial survey in
`docs/chain-sdk-requests/09-agent-tool-server.md` (mneme's capability
request), which flagged no known platform-specific risk on either OS.
**Not yet verified on real Windows hardware — no implementation exists
yet (see `AGENTS.md`'s Status), let alone a Windows run of it. Per
chain-sdk rule 3 (no single-platform contracts), this capability's
contract/`component.json` stay Draft, not stable, until confirmed here.**

## Approach

Same as macOS — binding a `TcpListener` to `127.0.0.1` is a plain,
portable Rust standard-library (or `tiny_http`/`hyper`, depending which
is chosen — see `research/MACOS.md`) operation on Windows too. No
Windows-specific Rust code is expected in
`crates/core/src/agent_server.rs`.

## Windows Defender Firewall — needs empirical confirmation

Windows Defender Firewall typically prompts ("Windows Security Alert —
Windows Defender Firewall has blocked some features of this app") the
first time an app opens a *listening* socket, historically regardless of
whether the bind is loopback-only. This has not been confirmed against a
recent Windows version for a plain `127.0.0.1`-only bind specifically —
worth checking explicitly, since an unexpected firewall prompt on first
use would be a real UX surprise for an app that otherwise runs
prompt-free (`http`'s outbound-only traffic doesn't trigger this at
all, which is why this is a genuinely new risk for this capability, not
one already covered by prior research).

## Checklist for whoever verifies this on Windows

- [ ] `cargo build`/`cargo check`/`cargo test -p chain-core` succeed once
      `agent_server.rs` exists.
- [ ] A fresh `chain init` (or `chain update` on an existing app) +
      `npm run dev` produces a working app whose `agentServer.start()`
      command works end to end against a real local HTTP client (mirror
      whatever macOS verification steps land in that platform's
      `AGENTS.md`/research doc).
- [ ] Confirm whether Windows Defender Firewall prompts for a
      loopback-only (`127.0.0.1`) listener at all — and if it does,
      whether declining the prompt breaks the server (it shouldn't, since
      loopback traffic doesn't need firewall allowance to reach the same
      machine's own listening socket, but this needs confirming in
      practice, not assumed).
- [ ] Confirm ephemeral-port binding (`options.port` omitted) picks an
      available port reliably, and that an explicit `options.port`
      already in use rejects with `UNAVAILABLE` as `CONTRACT.md`
      specifies.
- [ ] Update `docs/CAPABILITY_MATRIX.md`'s Windows column and this
      capability's `component.json` once confirmed.
